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
