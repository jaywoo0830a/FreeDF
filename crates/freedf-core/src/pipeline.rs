/**
 * Live ink pipeline: the in-progress stroke (`LiveStroke`) and its orchestrator
 * (`InkPipeline`).
 *
 * Two design wins, validated via TDD:
 *  1. **Incremental (frontier)** — points are append-only; the prefix before
 *     `mark_meshed()` is an immutable frontier. Renderers read only `tail()` to
 *     do O(new points) work per frame.
 *  2. **Commit-time immutability (freeze)** — `freeze()` copies the live stroke
 *     into a read-only `Stroke`; later appends never change the committed copy.
 */

use crate::model::{Stroke, StrokePoint, ToolType};
use crate::pen::{Materials, OneEuroFilter, WidthLocker};

/**
 * An in-progress stroke: **append-only**, with an incremental bounding box and a
 * mesh frontier.
 *
 * Points can only be appended. Everything before the frontier (`mesh_done`) is a
 * quasi-immutable (append-only) prefix:
 *  - `tail()` (= `points[mesh_done..]`, the "unbaked" tail) is the only input a
 *    renderer needs for **incremental O(new points)** meshing.
 *  - `fix_last()` refuses to rewrite points at or before the frontier, which
 *    preserves the immutable prefix.
 *  - `freeze()` copies the whole stroke into a read-only `Stroke`.
 *
 * Fields are public because this is a plain data holder (same style as
 * `model::Stroke`); behavior lives in the methods below.
 */
#[derive(Debug)]
pub struct LiveStroke {
    /// The drawing tool.
    pub tool: ToolType,
    /// RGBA color.
    pub color: [u8; 4],
    /// Base stroke width (points).
    pub width: f32,
    /// Appended points (page coordinates).
    pub points: Vec<StrokePoint>,
    /// Mesh frontier: indices below this are a committed (immutable) region.
    pub mesh_done: usize,
    /// Incremental bounding box `[min_x, min_y, max_x, max_y]`.
    pub bbox: Option<[f32; 4]>,
}

impl LiveStroke {
    /**
     * Begin a new stroke: empty points and the frontier zeroed.
     *
     * @param tool  the drawing tool.
     * @param color RGBA color of the stroke.
     * @param width base stroke width in points.
     * @return      a fresh, empty `LiveStroke`.
     */
    pub fn begin(tool: ToolType, color: [u8; 4], width: f32) -> Self {
        Self {
            tool,
            color,
            width,
            points: Vec::new(),
            mesh_done: 0,
            bbox: None,
        }
    }

    /**
     * Append a point (append-only).
     *
     * `mesh_done` is left alone so the new point immediately shows up in
     * `tail()`; the bounding box is expanded in O(1).
     *
     * @param p  the new stroke point.
     * @return   the new point count.
     */
    pub fn append(&mut self, p: StrokePoint) -> usize {
        self.points.push(p);
        match self.bbox {
            Some([x0, y0, x1, y1]) => {
                self.bbox = Some([
                    x0.min(p.x),
                    y0.min(p.y),
                    x1.max(p.x),
                    y1.max(p.y),
                ]);
            }
            None => self.bbox = Some([p.x, p.y, p.x, p.y]),
        }
        self.points.len()
    }

    /**
     * Replace the last point with its final (width-locked) version.
     *
     * Points at or before the frontier (`mesh_done`) are **never** rewritten —
     * in that case this is a no-op, preserving the immutable prefix.
     *
     * @param p  the finalized point to write as the new tail.
     */
    pub fn fix_last(&mut self, p: StrokePoint) {
        if self.points.is_empty() || self.points.len() <= self.mesh_done {
            return;
        }
        *self.points.last_mut().expect("live stroke is non-empty") = p;
    }

    /**
     * The "unbaked" tail — only the points after the frontier.
     *
     * This is the input to an incremental ribbon mesh: it contains exactly the
     * points not yet meshed, so a renderer appends just the new work.
     *
     * @return  slice of `points[mesh_done..]` (empty when fully meshed).
     */
    pub fn tail(&self) -> &[StrokePoint] {
        let from = self.mesh_done.min(self.points.len());
        &self.points[from..]
    }

    /**
     * Advance the frontier to the current length.
     *
     * After this call, those points are considered committed (meshed) and no
     * longer appear in `tail()`.
     */
    pub fn mark_meshed(&mut self) {
        self.mesh_done = self.points.len();
    }

    /**
     * Freeze the stroke into a read-only, committed `Stroke`.
     *
     * The returned stroke is deep-copied from the current point list; later
     * appends to this `LiveStroke` never change the committed copy.
     *
     * @param id         the committed stroke id.
     * @param created_ms the creation timestamp (Unix ms).
     * @return           an immutable `Stroke` snapshot.
     */
    pub fn freeze(&mut self, id: u64, created_ms: u64) -> Stroke {
        Stroke {
            id,
            tool: self.tool,
            color: self.color,
            width: self.width,
            points: self.points.clone(),
            created_ms,
        }
    }
}

/**
 * Orchestrator for live ink: bundles the 1€ filter, the width locker, and the
 * in-progress stroke (`LiveStroke`) into one object.
 *
 * The public surface is only `down` / `drag` / `up` — a single `drag()` call
 * internally does filter → width-lock (finalize previous point) → append, which
 * **compresses the call sequence**.
 */
pub struct InkPipeline {
    materials: Materials,
    max_width_pt: f32,
    smoothing: f32,
    filter_x: Option<OneEuroFilter>,
    filter_y: Option<OneEuroFilter>,
    filter_p: Option<OneEuroFilter>,
    locker: Option<WidthLocker>,
    live: Option<LiveStroke>,
}

impl InkPipeline {
    pub fn new(materials: Materials, max_width_pt: f32, smoothing: f32) -> Self {
        Self {
            materials,
            max_width_pt,
            smoothing,
            filter_x: None,
            filter_y: None,
            filter_p: None,
            locker: None,
            live: None,
        }
    }

    pub fn set_smoothing(&mut self, s: f32) {
        self.smoothing = s;
    }

    pub fn smoothing(&self) -> f32 {
        self.smoothing
    }

    pub fn is_drawing(&self) -> bool {
        self.live.is_some()
    }

    /**
     * Read-only handle to the in-progress stroke, if any.
     *
     * @return  the live stroke, or `None` when no stroke is being drawn.
     */
    pub fn live(&self) -> Option<&LiveStroke> {
        self.live.as_ref()
    }

    /**
     * Begin a stroke: (re)creates the 3-filter chain (x/y/p) and the width locker,
     * then feeds the first point through both and stores it.
     *
     * @param tool     the drawing tool.
     * @param color    RGBA color of the stroke.
     * @param x        page X to filter.
     * @param y        page Y to filter.
     * @param pressure raw pressure (0..1).
     * @param t        filter timestamp (seconds).
     * @param t_ms     point timestamp (Unix ms).
     * @param tilt_mag pen tilt magnitude 0..1 (drives italic/tilt width contrast).
     * @return         the first point, filtered and width-locked (render-ready).
     */
    pub fn down(
        &mut self,
        tool: ToolType,
        color: [u8; 4],
        x: f32,
        y: f32,
        pressure: f32,
        t: f64,
        t_ms: u64,
        tilt_mag: f32,
    ) -> StrokePoint {
        self.filter_x = Some(OneEuroFilter::from_smoothing(self.smoothing));
        self.filter_y = Some(OneEuroFilter::from_smoothing(self.smoothing));
        self.filter_p = Some(OneEuroFilter::from_smoothing(self.smoothing));
        self.locker = Some(WidthLocker::new(tool, self.max_width_pt, &self.materials, tilt_mag));
        self.live = Some(LiveStroke::begin(tool, color, self.max_width_pt));
        let tip = self.filter_lock(x, y, pressure, t, t_ms);
        self.live.as_mut().expect("drawing after down").append(tip);
        tip
    }

    /**
     * Append a point: filter → width-lock (finalizing the previous point) → append.
     *
     * @param x        page X to filter.
     * @param y        page Y to filter.
     * @param pressure raw pressure (0..1).
     * @param t        filter timestamp (seconds).
     * @param t_ms     point timestamp (Unix ms).
     * @return         the new tip point, or `None` when not drawing.
     */
    pub fn drag(
        &mut self,
        x: f32,
        y: f32,
        pressure: f32,
        t: f64,
        t_ms: u64,
    ) -> Option<StrokePoint> {
        if self.live.is_none() {
            return None;
        }
        let tip = self.filter_lock(x, y, pressure, t, t_ms);
        self.live.as_mut().expect("active").append(tip);
        Some(tip)
    }

    /**
     * Pen-up: finalizes the last point's width, **freezes** the stroke into an
     * immutable `Stroke`, and deactivates drawing state.
     *
     * @param id  the committed stroke id.
     * @return    the immutable `Stroke`, or `None` when not drawing.
     */
    pub fn up(&mut self, id: u64) -> Option<Stroke> {
        let mut live = self.live.take()?;
        if let Some(final_pt) = self.locker.take().and_then(|mut l| l.finish()) {
            live.fix_last(final_pt);
        }
        self.filter_x = None;
        self.filter_y = None;
        self.filter_p = None;
        Some(live.freeze(id, 0))
    }

    /**
     * Shared per-point path: filter → locker.push (finalize previous) → new tip.
     *
     * @return  the filtered, width-locked tip point.
     */
    fn filter_lock(
        &mut self,
        x: f32,
        y: f32,
        pressure: f32,
        t: f64,
        t_ms: u64,
    ) -> StrokePoint {
        // 스무딩 0이면 1€ 필터를 **건너뜁니다** — 앱의 \"스무딩 꺼짐 → raw 좌표\"
        // 경로와 정확히 일치시켜 행동 변경이 없게 합니다.
        let use_filter = self.smoothing > 0.001;
        let sx = if use_filter {
            self.filter_x.as_mut().map(|f| f.filter(x, t)).unwrap_or(x)
        } else {
            x
        };
        let sy = if use_filter {
            self.filter_y.as_mut().map(|f| f.filter(y, t)).unwrap_or(y)
        } else {
            y
        };
        let sp = if use_filter {
            self.filter_p
                .as_mut()
                .map(|f| f.filter(pressure, t))
                .unwrap_or(pressure)
        } else {
            pressure
        }
        .clamp(0.0, 1.0);
        let raw = StrokePoint::with_time(sx, sy, sp, t_ms);
        let (locked_prev, tip) = self
            .locker
            .as_mut()
            .expect("locker exists after down")
            .push(raw);
        if let Some(prev) = locked_prev {
            if let Some(l) = self.live.as_mut() {
                l.fix_last(prev);
            }
        }
        tip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> InkPipeline {
        InkPipeline::new(Materials::default(), 3.0, 0.4)
    }

    // ---- LiveStroke: append-only + frontier + incremental bbox ----

    #[test]
    fn live_stroke_incremental_bbox_matches_scan() {
        let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
        let pts = vec![
            StrokePoint::new(5.0, 5.0, 0.5),
            StrokePoint::new(1.0, 9.0, 0.5),
            StrokePoint::new(8.0, 2.0, 0.5),
        ];
        for p in pts {
            l.append(p);
        }
        let inc = l.bbox.expect("incremental bbox");
        let scanned = l.freeze(0, 0).bounding_box().expect("scan bbox");
        assert_eq!(inc, scanned, "incremental bbox == scanned bbox");
        let exp: [f32; 4] = [1.0, 2.0, 8.0, 9.0];
        assert_eq!(inc, exp);
    }

    #[test]
    fn live_stroke_frontier_makes_tail_shrink_and_grow() {
        let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
        assert_eq!(l.mesh_done, 0);
        assert_eq!(l.tail().len(), 0);
        l.append(StrokePoint::new(0.0, 0.0, 0.5));
        l.append(StrokePoint::new(1.0, 0.0, 0.5));
        assert_eq!(l.points.len(), 2);
        assert_eq!(l.mesh_done, 0, "append does not move the frontier");
        assert_eq!(l.tail().len(), 2, "everything is an unbaked tail");
        l.mark_meshed();
        assert_eq!(l.mesh_done, 2);
        assert_eq!(l.tail().len(), 0, "fully meshed");
        l.append(StrokePoint::new(2.0, 0.0, 0.5));
        assert_eq!(l.tail().len(), 1, "only the new point is in the tail");
        assert_eq!(l.points.len(), 3);
    }

    #[test]
    fn live_stroke_fix_last_blocked_at_frontier() {
        let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
        l.append(StrokePoint::new(0.0, 0.0, 0.5));
        l.append(StrokePoint::new(1.0, 0.0, 0.5));
        l.mark_meshed(); // frontier == len == 2
        let w0 = l.points.last().expect("last").width;
        l.fix_last(StrokePoint::new(1.0, 0.0, 0.9)); // at/before frontier — ignored (immutable prefix)
        assert_eq!(l.points.last().expect("last").width, w0, "immutable prefix must not change");
        l.append(StrokePoint::new(2.0, 0.0, 0.5)); // len 3 > frontier 2
        l.fix_last(StrokePoint {
            x: 2.0,
            y: 0.0,
            pressure: 0.5,
            t_ms: 0,
            width: 0.9,
        }); // after frontier — applied
        assert!((l.points.last().expect("last").width - 0.9).abs() < 1e-6);
    }

    #[test]
    fn live_stroke_freeze_detaches_from_live() {
        let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
        l.append(StrokePoint::new(0.0, 0.0, 0.5));
        l.append(StrokePoint::new(4.0, 4.0, 0.5));
        let frozen = l.freeze(7, 1000);
        assert_eq!(frozen.id, 7);
        assert_eq!(frozen.tool, ToolType::Pen);
        assert_eq!(frozen.points.len(), 2);
        // appended after freeze; the committed copy is unchanged.
        l.append(StrokePoint::new(9.0, 9.0, 0.5));
        assert_eq!(frozen.points.len(), 2, "frozen copy does not change");
        assert_eq!(l.points.len(), 3);
        let exp: [f32; 4] = [0.0, 0.0, 4.0, 4.0];
        assert_eq!(frozen.bounding_box().expect("bbox"), exp);
    }

    // ---- InkPipeline: down/drag/up orchestration ----

    #[test]
    fn pipeline_down_starts_with_locked_first_point() {
        let mut p = pipeline();
        let tip = p.down(ToolType::Pen, [10, 20, 30, 255], 100.0, 200.0, 0.7, 0.0, 0, 0.0);
        assert!(p.is_drawing());
        let l = p.live().expect("live");
        assert_eq!(l.points.len(), 1);
        assert!(l.points[0].width > 0.0, "first point width locked");
        assert!((tip.width - l.points[0].width).abs() < 1e-6);
    }

    #[test]
    fn pipeline_drag_appends_and_locks_widths() {
        let mut p = pipeline();
        p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
        p.drag(15.0, 22.0, 0.6, 0.016, 10);
        p.drag(20.0, 21.0, 0.7, 0.032, 20);
        let l = p.live().expect("live");
        assert!(l.points.len() >= 3, "each drag appends a point");
        for i in 0..l.points.len() {
            assert!(l.points[i].width > 0.0, "all points have locked widths");
        }
    }

    #[test]
    fn pipeline_up_preserves_wysiwyg_no_penup_change() {
        let mut p = pipeline();
        p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
        for i in 1..30 {
            p.drag(
                10.0 + i as f32 * 3.0,
                20.0 + (i as f32 * 0.4).sin(),
                0.6 + (i as f32 * 0.01).sin(),
                i as f64 * 0.016,
                i * 10,
            );
        }
        let before = p.live().expect("live").points.last().expect("last").width;
        let frozen = p.up(42).expect("up");
        let after = frozen.points.last().expect("last").width;
        assert!(
            (after - before).abs() < 1e-5,
            "penup width unchanged (WYSIWYG): seen={before} locked={after}",
        );
        assert!(!p.is_drawing(), "inactive after up");
        assert_eq!(frozen.id, 42);
    }

    #[test]
    fn pipeline_second_stroke_starts_fresh() {
        let mut p = pipeline();
        p.down(ToolType::Pen, [0, 0, 0, 255], 0.0, 0.0, 0.5, 0.0, 0, 0.0);
        p.drag(1.0, 0.0, 0.5, 0.016, 10);
        let _ = p.up(1);
        assert!(!p.is_drawing());
        p.down(ToolType::Pen, [0, 0, 0, 255], 50.0, 50.0, 0.5, 0.0, 0, 0.0);
        let l = p.live().expect("live");
        assert_eq!(l.points.len(), 1, "new stroke does not inherit previous points");
    }

    /// 계약: `down`에 준 `tilt_mag`가 WidthLocker까지 전달돼 폭에 반영됩니다.
    /// (BallPenProfile.tilt_k=0.35 → 기울기가 클수록 더 두꺼운 획)
    #[test]
    fn pipeline_down_threads_tilt_into_width_locker() {
        fn locked_width_after(tilt: f32) -> f32 {
            // smoothing 0 → 필터가 좌표를 왜곡하지 않고 틸트 폭만 비교합니다.
            let mut p = InkPipeline::new(Materials::default(), 3.0, 0.0);
            p.down(ToolType::Pen, [0, 0, 0, 255], 0.0, 0.0, 0.5, 0.0, 0, tilt);
            p.drag(10.0, 0.0, 0.5, 0.016, 16);
            p.drag(20.0, 0.0, 0.5, 0.032, 32);
            p.live().expect("live").points[1].width
        }
        let w0 = locked_width_after(0.0);
        let w1 = locked_width_after(1.0);
        assert!(
            w1 > w0,
            "tilt must widen the pen stroke (threaded through the pipeline): \
             tilt0={w0} vs tilt1={w1}"
        );
    }

    /// 계약: `smoothing <= 0.001`이면 1€ 필터를 건너뛰어 **raw 좌표**를 통과시킵니다
    /// (앱의 \"스무딩 꺼짐 → raw push\" 경로와 행동 일치).
    #[test]
    fn pipeline_smoothing_zero_passes_raw_coords() {
        let mut p = InkPipeline::new(Materials::default(), 3.0, 0.0);
        p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
        p.drag(15.0, 22.0, 0.6, 0.016, 16);
        p.drag(20.0, 21.0, 0.7, 0.032, 32);
        let pts = &p.live().expect("live").points;
        assert!((pts[1].x - 15.0).abs() < 1e-4, "raw x passthrough: {}", pts[1].x);
        assert!((pts[1].y - 22.0).abs() < 1e-4, "raw y passthrough: {}", pts[1].y);
        assert!((pts[2].x - 20.0).abs() < 1e-4, "raw x passthrough: {}", pts[2].x);
    }
}