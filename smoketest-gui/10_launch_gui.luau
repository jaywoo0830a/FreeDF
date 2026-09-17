--!strict
-- freedf-gui 셸 스모크 — 창이 뜨고 계약 위젯(툴바 버튼 · 캔버스)이 등록되는지 확인.
-- (Phase 3 계측 인프라 검증 — docs/freedf-gui-migration.md)
eguidev.root:expect({ frame_at_least = 1 })

-- 툴바 버튼 (어댑터가 그린 버튼이 `gui.<라벨 슬러그>` id로 등록된다)
eguidev.widget("gui.new_tab"):expect({ present = true })
eguidev.widget("gui.close_tab"):expect({ present = true })
eguidev.widget("gui.zoom_in"):expect({ present = true })
eguidev.widget("gui.zoom_out"):expect({ present = true })
eguidev.widget("gui.fit"):expect({ present = true })
eguidev.widget("gui.prev_page"):expect({ present = true })
eguidev.widget("gui.next_page"):expect({ present = true })
eguidev.widget("gui.open_pdf"):expect({ present = true })
eguidev.widget("gui.clear_ink"):expect({ present = true })
eguidev.widget("gui.fountain"):expect({ present = true })
eguidev.widget("gui.highlighter"):expect({ present = true })
eguidev.widget("gui.eraser"):expect({ present = true })
eguidev.widget("gui.red"):expect({ present = true })
eguidev.widget("gui.blue"):expect({ present = true })
eguidev.widget("gui.bookmark"):expect({ present = true })
eguidev.widget("gui.bookmarks"):expect({ present = true })
eguidev.widget("gui.outline"):expect({ present = true })
eguidev.widget("gui.about"):expect({ present = true })

-- 탭 (기본 "Untitled" 탭)
eguidev.widget("gui.untitled"):expect({ present = true })

-- 캔버스 (painter 영역 — 기하가 공개된다)
local surface = eguidev.widget("canvas.surface"):expect({ present = true })
assert(surface ~= nil, "canvas.surface 상태를 읽지 못했습니다")
assert(surface.rect.max.x > surface.rect.min.x, "캔버스 영역이 비어 있습니다")
assert(surface.rect.max.y - surface.rect.min.y > 300, "캔버스 높이가 비정상적으로 작습니다")

eguidev.log(eguidev.dump_text({ fields = "core" }))

return {
    ok = true,
    canvas_rect = surface.rect,
    screenshot = eguidev.root:screenshot(),
}
