--!strict
-- FreeDF가 실제로 창을 띄우고 크롬(탭바 · 3단 툴바 · 캔버스)을 그리는지 확인합니다.
--
-- FreeDF에는 아직 픽스처가 없으므로 `eguidev.fixture(...)`를 부르지 않고
-- 시작 상태 그대로 검사합니다. (DB/Sync 서버 없이도 시작 화면은 렌더링됩니다.)

local root = eguidev.root
root:expect({ frame_at_least = 1 })

-- ── Row 1 — 워크스페이스 (패널 · 편집 이력 · 파일 · 설정) ──────────────
eguidev.widget("toolbar.settings"):expect({ present = true, enabled = true })
eguidev.widget("toolbar.hide_ui"):expect({ present = true })
eguidev.widget("toolbar.undo"):expect({ present = true })
eguidev.widget("toolbar.redo"):expect({ present = true })
eguidev.widget("toolbar.clear_page"):expect({ present = true })
eguidev.widget("toolbar.panel.library"):expect({ present = true })
eguidev.widget("toolbar.panel.outline"):expect({ present = true })
eguidev.widget("toolbar.panel.palette"):expect({ present = true })

-- ── Row 2 — 페이지 / 종이 ─────────────────────────────────────────────
eguidev.widget("toolbar.insert_page"):expect({ present = true })
eguidev.widget("toolbar.delete_page"):expect({ present = true })

-- ── Row 3 — 필기 도구 선택기 ──────────────────────────────────────────
eguidev.widget("toolbar.tool.pen"):expect({ present = true })
eguidev.widget("toolbar.tool.fountain"):expect({ present = true })
eguidev.widget("toolbar.tool.highlighter"):expect({ present = true })
eguidev.widget("toolbar.tool.eraser"):expect({ present = true })
eguidev.widget("toolbar.tool.pan"):expect({ present = true })

-- ── 탭바 + 캔버스 영역 ────────────────────────────────────────────────
eguidev.widget("tabs.new_note"):expect({ present = true })
eguidev.widget("tabs.open_pdf"):expect({ present = true })

local surface = eguidev.widget("canvas.surface"):expect({ present = true })
assert(surface ~= nil, "canvas.surface 상태를 읽지 못했습니다")
assert(surface.rect.max.x > surface.rect.min.x, "캔버스 영역이 비어 있습니다")

-- 덤프는 실패 번들/로그에서 사람이 읽는 근거가 됩니다.
eguidev.log(eguidev.dump_text({ fields = "core" }))

return {
    viewport = root.id,
    canvas_rect = surface.rect,
    screenshot = root:screenshot(),
}
