//! 잉크 커밋 경로 — 획 완료, 텍스트 인식 하이라이트, 점(탭).

use super::*;

impl FreeDfApp {
    pub(crate) fn finish_stroke(&mut self) {
        // 스로틀 캐시 무효화 — 완성된 획은 병합 메시(정확 지오메트리)로 넘어갑니다.
        self.active_mesh = None;
        if let Some(mut active) = self.active_stroke.take() {
            self.smooth_active = false;
            if active.points.is_empty() {
                return;
            }
            // ── 펜업 전환 진단: 표시되던 마지막 점들의 (필압, 폭) vs 펜 뗀 뒤.
            let before_penup: Vec<(f32, f32)> = active
                .points
                .iter()
                .rev()
                .take(4)
                .map(|p| (p.pressure, p.width))
                .collect();
            // 마지막 점의 폭을 확정합니다 (인과적 — 이후 절대 변하지 않음).
            if let Some(mut locker) = self.width_locker.take() {
                if let Some(final_pt) = locker.finish() {
                    if let Some(last) = active.points.last_mut() {
                        *last = final_pt;
                    }
                }
            }
            let after_penup: Vec<(f32, f32)> = active
                .points
                .iter()
                .rev()
                .take(4)
                .map(|p| (p.pressure, p.width))
                .collect();
            if before_penup != after_penup {
                pen_trace(&format!(
                    "PENUP-CHANGED: 표시={before_penup:?} 확정={after_penup:?} live_pressure={:?} ← 펜 떼는 순간 폭 데이터가 바뀜!",
                    self.live_pressure
                ));
            } else {
                pen_trace(&format!(
                    "penup tail (pressure,width): {after_penup:?} live_pressure={:?}",
                    self.live_pressure
                ));
            }
            // ── 펜 진단: 획이 끝나면 필압/**렌더 폭** 변화량을 로그로 남깁니다.
            if active.tool != ToolType::Highlighter {
                let n_pt = active.points.len();
                let (mut pmn, mut pmx) = (f32::MAX, f32::MIN);
                let (mut wmn, mut wmx) = (f32::MAX, f32::MIN);
                let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
                let mut unlocked = 0usize;
                for p in &active.points {
                    pmn = pmn.min(p.pressure);
                    pmx = pmx.max(p.pressure);
                    if p.width > 0.0 {
                        wmn = wmn.min(p.width);
                        wmx = wmx.max(p.width);
                        // 실제 렌더에 쓰이는 절반 폭 (freedf_canvas::halves_for_stroke와 동일 규칙).
                        let h = (p.width * 0.5).max(0.05);
                        hmn = hmn.min(h);
                        hmx = hmx.max(h);
                    } else {
                        unlocked += 1;
                    }
                }
                let verdict = if n_pt < 8 {
                    "점 부족"
                } else if pmx - pmn < 0.05 {
                    "필압 일정 → 입력 문제 (OTD 연결/필압 소스 확인)"
                } else if unlocked > 0 {
                    "폭 잠금 안 됨 → locker 버그"
                } else if hmx - hmn < 0.02 {
                    "필압은 변하는데 렌더 폭 고정 → 모델/바닥값 버그"
                } else {
                    "OK — 렌더 폭 변화 정상"
                };
                self.pen_verdict = Some(verdict.to_string());
                pen_trace(&format!(
                    "stroke end: tool={:?} n={n_pt} pressure=[{pmn:.3}..{pmx:.3}] width=[{wmn:.3}..{wmx:.3}] half=[{hmn:.3}..{hmx:.3}] unlocked={unlocked} live_pressure={:?} tilt=[{:+.0},{:+.0}] → {verdict}",
                    active.tool, self.live_pressure, self.pen_tilt[0], self.pen_tilt[1]
                ));
            }
            // 하이라이터 + 텍스트 인식 모드면 스와이프가 닿은 문서 텍스트 위로
            // 깔끔한 하이라이트를 만들어 저장하고, 원본 자유선은 버립니다.
            if active.tool == ToolType::Highlighter
                && self.text_highlight_snap
                && self.document.is_some()
                && self.add_text_highlights(&active)
            {
                return;
            }
            // 블리드 나이의 기준 = **획을 그리기 시작한 시각**(첫 점 t_ms).
            // 펜을 뗀 시각이 아니라 시작 시각이어야, 그리는 동안 자라던
            // 번짐이 펜을 떼는 순간에도 끊김 없이 이어집니다.
            let created_ms = active
                .points
                .first()
                .map(|p| p.t_ms)
                .filter(|t| *t > 0)
                .unwrap_or_else(now_ms);
            // DB 시퀀스에서 id를 미리 할당받아 스토어/히스토리/DB 행이 같은
            // id를 공유하게 합니다 (풀링 — 스트로크마다 왕복하지 않음).
            let db_id = self.next_stroke_ids(1).first().copied();
            let id = match (self.doc_id, db_id) {
                (Some(doc_id), Some(sid)) => {
                    self.store.add_stroke_with_id(
                        self.current_page,
                        sid as u64,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, sid as u64, created_ms);
                    let strokes: Vec<_> = self
                        .store
                        .strokes_on(self.current_page)
                        .iter()
                        .filter(|s| s.id == sid as u64)
                        .cloned()
                        .collect();
                    self.db
                        .insert_strokes(doc_id, self.current_page as i32, &strokes);
                    sid as u64
                }
                _ => {
                    let id = self.store.add_stroke(
                        self.current_page,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, id, created_ms);
                    // 풀 소진 폴백이어도 문서가 열려 있으면 write-behind 큐에
                    // 보냅니다 (메타-only 저장에서도 유실되지 않도록).
                    if let Some(doc_id) = self.doc_id {
                        let strokes: Vec<_> = self
                            .store
                            .strokes_on(self.current_page)
                            .iter()
                            .filter(|s| s.id == id)
                            .cloned()
                            .collect();
                        self.db
                            .insert_strokes(doc_id, self.current_page as i32, &strokes);
                    }
                    id
                }
            };
            self.last_finished_id = Some(id);
            if let Some(stroke) = self.store.stroke(self.current_page, id).cloned() {
                self.push_history(Edit::AddStrokes {
                    page: self.current_page,
                    strokes: vec![stroke.clone()],
                });
                self.logger.log(AppEvent::StrokeAdded {
                    page: self.current_page,
                    points: stroke.points.len(),
                    tool: tool_label(active.tool).to_string(),
                    width: active.width,
                });
            }
            // 유휴 자동 저장 타이머 시작 (pen-up 시각 기록).
            self.last_pen_up_ms = now_ms();
        }
    }

    /// 스트로크가 닿은 **글자**들을 줄 단위로 묶어 밴드 하이라이트를 만듭니다.
    ///
    /// pdfium `tight_bounds()`(글자별 박스)로 정밀 판정하며, 각 줄은 그 줄의
    /// 높이만큼의 반투명 밴드 하나로 칠합니다. **필압은 전혀 쓰지 않습니다.**
    /// 성공(텍스트 하이라이트를 만든 경우)하면 `true`를 반환합니다.
    pub(crate) fn add_text_highlights(&mut self, active: &ActiveStroke) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let (mut x0, mut y0, mut x1, mut y1) =
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &active.points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x1 < x0 || y1 < y0 {
            return false;
        }
        // 항상 현재 페이지의 글자 좌표를 새로 읽습니다 (캐시 없음 → 정확).
        let char_rects = doc.page_char_rects(self.current_page).unwrap_or_default();
        if char_rects.is_empty() {
            // 페이지에 선택 가능한 텍스트가 없음(스캔/이미지 PDF 등).
            self.status = Some(
                "No selectable text on this page — drew a free-form highlight."
                    .to_string(),
            );
            return false;
        }
        // 닿은 글자를 줄 단위로 합쳐 연속 밴드로 만듭니다.
        let rects = char_line_highlights(&char_rects, [x0, y0, x1, y1], 4.0);
        if rects.is_empty() {
            return false;
        }
        // DB 시퀀스에서 밴드 수만큼 id를 미리 할당합니다 (풀링).
        let ids = self.next_stroke_ids(rects.len());
        // 풀이 마르면(연결 늦음/끊김) 나머지 밴드는 로컬 id 폴백 — UI 왕복 없음.
        let mut local_next = ids
            .iter()
            .copied()
            .map(|i| i as u64)
            .max()
            .or_else(|| {
                self.store
                    .strokes_on(self.current_page)
                    .iter()
                    .map(|s| s.id)
                    .max()
            })
            .unwrap_or(0)
            + 1;
        let created_ms = now_ms();
        let mut strokes = Vec::new();
        for (k, r) in rects.iter().enumerate() {
            // 밴드 높이 = 그 줄의 글자 높이(포인트). 필압은 1.0(무시).
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            let sid = match ids.get(k) {
                Some(i) => *i as u64,
                None => {
                    let id = local_next;
                    local_next += 1;
                    id
                }
            };
            strokes.push(freedf_core::model::Stroke {
                id: sid,
                tool: ToolType::Highlighter,
                color: active.color,
                width: line_h,
                points: vec![
                    StrokePoint::new(r[0], yc, 1.0),
                    StrokePoint::new(r[2], yc, 1.0),
                ],
                created_ms,
            });
        }
        self.store.add_strokes(self.current_page, strokes.clone());
        if let Some(doc_id) = self.doc_id {
            self.db
                .insert_strokes(doc_id, self.current_page as i32, &strokes);
        }
        self.push_history(Edit::AddStrokes {
            page: self.current_page,
            strokes: strokes.clone(),
        });
        self.logger.log(AppEvent::StrokeAdded {
            page: self.current_page,
            points: strokes.len() * 2,
            tool: "Highlighter".to_string(),
            width: active.width,
        });
        true
    }

    pub(crate) fn commit_dot(&mut self, point: [f32; 2], pressure: f32) {
        let (color, width) = self.current_drawing_style();
        self.width_locker = Some(freedf_core::pen::WidthLocker::new(
            self.tool,
            width,
            self.pen_profile,
            self.fountain_profile,
            tilt_magnitude(&self.pen_tilt),
        ));
        let mut point = StrokePoint::with_time(point[0], point[1], pressure, now_ms());
        if let Some(locker) = &mut self.width_locker {
            let (_, tip) = locker.push(point);
            point = tip;
        }
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![point],
        });
        self.finish_stroke();
    }

    // ---------- Texture rendering ----------
}
