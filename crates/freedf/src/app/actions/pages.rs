//! pages — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    pub(crate) fn next_page(&mut self) {
        if let Some(doc) = &self.document {
            if self.current_page + 1 < doc.page_count() {
                self.current_page += 1;
                self.on_page_changed();
            }
        }
    }

    /// 다음 페이지로 이동 (키보드 PgDn). FreeDF 노트에서 이미 마지막
    /// 페이지라면 현재 페이지와 같은 크기/용지의 빈 페이지를 자동으로
    /// 추가해 계속 이어 씁니다.
    pub(crate) fn next_page_auto(&mut self) {
        let at_end = self
            .document
            .as_ref()
            .map(|d| self.current_page + 1 >= d.page_count())
            .unwrap_or(false);
        if at_end && self.current_note.is_some() {
            // 현재(마지막) 페이지의 크기/용지를 복사해 바로 다음에 삽입.
            self.insert_page_action(InsertTarget::FromCurrent);
        } else {
            self.next_page();
        }
    }

    pub(crate) fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.on_page_changed();
        }
    }

    /// 브라우저식 PgUp/PgDn 동작 (핵심 판정은 core의 `browser_page_step`).
    ///
    /// - 페이지가 캔버스보다 길면(세로 스크롤 여지) **한 뷰포트만** 이동하고,
    ///   페이지를 넘기지 않습니다.
    /// - 더 스크롤할 수 없을 때만 다음/이전 페이지로 넘어갑니다
    ///   (노트는 `next_page_auto`라서 마지막 페이지면 새 페이지가 자동 추가됨).
    pub(crate) fn page_key(&mut self, down: bool) {
        if self.document.is_none() {
            return;
        }
        let canvas_h = self.last_canvas[1].max(1.0);
        let step = freedf_core::transform::browser_page_step(
            self.page_size_pts[1],
            self.view.zoom,
            canvas_h,
            CANVAS_MARGIN,
            self.view.pan_y,
            down,
        );
        match step {
            freedf_core::transform::PageStep::ScrollTo { pan_y } => {
                self.view.pan_y = pan_y;
            }
            freedf_core::transform::PageStep::NextPage => self.next_page_auto(),
            freedf_core::transform::PageStep::PrevPage => self.prev_page(),
        }
    }

    pub(crate) fn goto_page(&mut self, index: PageIndex) {
        if let Some(doc) = &self.document {
            if index < doc.page_count() {
                self.current_page = index;
                self.on_page_changed();
            }
        }
    }

    pub(crate) fn on_page_changed(&mut self) {
        let from = self.transition_last_page;
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        self.scroll_vel = Vec2::ZERO;
        // 다음 페이지 프리페치 예약 (현재 페이지의 줌/크기 기준).
        self.prefetch_pending = true;
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
        }
        self.render_dirty = true;
        // Keep the current zoom across page changes; just re-align the new page
        // (instead of resetting the zoom to fit-width).
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.search_update();
        if let Some(doc) = &self.document {
            self.logger.log(AppEvent::PageChanged {
                page: self.current_page,
                total: doc.page_count(),
            });
        }
        self.start_page_anim(from, self.current_page);
        self.transition_last_page = self.current_page;
        // 현재 페이지/줌 상태를 세션에 기록합니다.
        self.save_session();
    }

    /// Captures the outgoing page texture and starts a slide transition.
    ///
    /// PgUp/PgDn 키는 `transition_vertical`을 세팅해 **세로**(위/아래로 넘기는
    /// 긴 페이지 목록처럼) 애니메이션을 만들고, 그 외(내비게이션 바/휠/화살표)는
    /// 가로 슬라이드를 유지합니다.
    pub(crate) fn start_page_anim(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let vertical = std::mem::take(&mut self.transition_vertical);
        // 프리페치된 새 페이지 텍스처가 있으면 즉시 사용 → 렌더 대기 없는 전환.
        let hit = self
            .prefetch
            .as_ref()
            .is_some_and(|(p, z, _)| *p == to && (*z - self.view.zoom).abs() < 1e-3);
        if hit {
            let (_, _, tex) = self.prefetch.take().expect("hit");
            self.prev_texture = self.texture.take();
            self.texture = Some(tex);
            self.render_dirty = false;
        } else {
            if self.texture.is_none() {
                return;
            }
            // The current texture still holds the old page; keep it for the
            // outgoing frame and force a fresh render for the new page.
            self.prev_texture = self.texture.take();
            self.render_dirty = true;
        }
        self.page_anim = Some(PageAnim {
            progress: 0.0,
            direction: if to > from { 1.0 } else { -1.0 },
            vertical,
        });
    }

    /// 새 빈 페이지를 1장 삽입합니다 (`insert_pages_action`의 단건 래퍼).
    pub(crate) fn insert_page_action(&mut self, target: InsertTarget) {
        self.insert_pages_action(target, 1);
    }

    /// 새 빈 페이지를 `count`장 삽입합니다. `target`에 따라 위치/크기/용지가
    /// 달라집니다 (Insert Page 메뉴의 개수 지정).
    pub(crate) fn insert_pages_action(&mut self, target: InsertTarget, count: usize) {
        let count = count.clamp(1, 200);
        // (self.document를 가변 빌리기 전에 self 값을 미리 계산)
        let default_paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
        };
        let default_size = self.new_page_size_pts();
        let Some(doc) = &mut self.document else {
            return;
        };
        let total = doc.page_count();
        if total == 0 {
            return;
        }
        let (idx, size, paper) = match target {
            // 현재 페이지의 크기/용지를 그대로 써서 바로 다음에 삽입.
            InsertTarget::FromCurrent => {
                let size = doc.page_size_pts(self.current_page);
                let paper = self
                    .store
                    .paper_on_or(self.current_page, default_paper);
                (self.current_page + 1, size, paper)
            }
            InsertTarget::AtVeryFront => (0, default_size, default_paper),
            InsertTarget::AtVeryBack => (total, default_size, default_paper),
            InsertTarget::BeforeCurrent => (self.current_page, default_size, default_paper),
            InsertTarget::AfterCurrent => (self.current_page + 1, default_size, default_paper),
        };
        // 같은 위치에 반복 삽입 → 연속된 count장 (빈 페이지라 순서 무관).
        for _ in 0..count {
            if let Err(e) = doc.insert_page_at(idx, size) {
                self.status = Some(e);
                return;
            }
        }
        // 끝에 삽입이면 기존 획 인덱스 불변(메타만), 중간이면 서버에서 count장 이동.
        let shift = (idx < total).then_some(StructureOp::Shift {
            from: idx as i32,
            delta: count as i32,
        });
        for _ in 0..count {
            self.store.insert_page(idx);
            self.store.set_paper(idx, paper);
        }
        self.current_page = idx;
        let total = doc.page_count();
        self.logger.log(AppEvent::PageAdded { page: idx, total });
        self.on_page_changed();
        self.flush_current_document_with(shift);
    }

    /// 현재 페이지를 시계/반시계 90° 회전합니다 (주석도 함께 회전).
    pub(crate) fn rotate_page_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let idx = self.current_page;
        if idx >= doc.page_count() {
            return;
        }
        let [w, h] = doc.page_size_pts(idx); // 회전 전 표시 크기
        if let Err(e) = doc.rotate_page(idx, clockwise) {
            self.status = Some(e);
            return;
        }
        self.store.rotate_strokes_on(idx, w, h, clockwise);
        // 회전은 좌표계를 바꾸는 구조 연산 — 회전 전 좌표가 담긴 undo 히스토리와
        // DB 편집 저널은 더 이상 유효하지 않으므로 초기화합니다.
        self.history.clear();
        if let Some(doc_id) = self.doc_id {
            self.db.clear_edits(doc_id);
        }
        self.page_size_pts = doc.page_size_pts(idx);
        let total = doc.page_count();
        self.logger
            .log(AppEvent::PageRotated { page: idx, total, clockwise });
        self.status = Some(format!(
            "Rotated page {} 90° {}",
            idx + 1,
            if clockwise { "clockwise" } else { "counter-clockwise" }
        ));
        self.on_page_changed();
        self.flush_current_document_with(Some(StructureOp::RotatePage {
            page: idx as i32,
            clockwise,
            w,
            h,
        }));
    }

    /// 문서의 모든 페이지를 시계/반시계 90° 회전합니다 (주석도 함께).
    pub(crate) fn rotate_all_pages_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let count = doc.page_count();
        if count == 0 {
            return;
        }
        // 각 페이지의 회전 전 표시 크기 스냅샷.
        let sizes: Vec<[f32; 2]> = (0..count).map(|i| doc.page_size_pts(i)).collect();
        if let Err(e) = doc.rotate_all_pages(clockwise) {
            self.status = Some(e);
            return;
        }
        for i in 0..count {
            self.store.rotate_strokes_on(i, sizes[i][0], sizes[i][1], clockwise);
        }
        // 좌표계가 바뀌었으므로 undo 히스토리/저널 초기화.
        self.history.clear();
        if let Some(doc_id) = self.doc_id {
            self.db.clear_edits(doc_id);
        }
        self.page_size_pts = doc.page_size_pts(self.current_page);
        self.logger.log(AppEvent::PageRotated {
            page: self.current_page,
            total: count,
            clockwise,
        });
        self.status = Some(format!(
            "Rotated all {count} pages 90° {}",
            if clockwise { "clockwise" } else { "counter-clockwise" }
        ));
        self.on_page_changed();
        self.flush_current_document_with(Some(StructureOp::RotateAll {
            clockwise,
            sizes,
        }));
    }

    pub(crate) fn delete_page_action(&mut self) {
        let Some(doc) = &mut self.document else {
            return;
        };
        if doc.page_count() <= 1 {
            self.status = Some("Cannot delete the last remaining page.".to_string());
            return;
        }
        let idx = self.current_page;
        if let Err(e) = doc.delete_page(idx) {
            self.status = Some(e);
            return;
        }
        let total = doc.page_count();
        self.store.remove_page(idx);
        if self.current_page >= total {
            self.current_page = total.saturating_sub(1);
        }
        self.logger.log(AppEvent::PageDeleted {
            page: idx,
            total,
        });
        self.on_page_changed();
        self.flush_current_document_with(Some(StructureOp::DeletePage {
            page: idx as i32,
        }));
    }

    // ---------- Zoom / fit ----------
}
