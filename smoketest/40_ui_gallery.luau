--!strict
-- UI 계약 스캔 — 컴포넌트 갤러리를 스캔해 모든 컴포넌트가 계약을 지키는지 검증합니다.
--
-- **테스트 훅**: 갤러리는 `FREEDF_UI_GALLERY=1`로 시작 시 열립니다.
-- (메뉴 안 항목은 egui 팝업이라 자동화 클릭 대상이 아닙니다 — 실측 확인.
--  그래서 실행 경로를 나눴습니다: `scripts/ui-gallery-check.sh`)
--
-- 검증 항목:
--   1) 갤러리가 열려 있다 (아니면 skip — 스위트에서는 실패로 만들지 않음)
--   2) 모든 컴포넌트가 존재하고 **최소 24pt** 타깃을 갖는다
--   3) 크기 등급 계약: small ≥24 · medium ≥28 · touch ≥32
--   4) 상호작용: 갤러리의 행 토글이 실제로 값을 뒤집는다(왕복 후 원복)

eguidev.root:wait({ frame_at_least = 1 })

local sentinel = eguidev.widget("gallery.button.primary")
if sentinel:state() == nil then
    -- 갤러리 미오픈: `edev smoke` 기본 실행에서는 건너뜁니다.
    return { skipped = true, reason = "FREEDF_UI_GALLERY=1 로 실행해야 갤러리가 열립니다" }
end

-- 2) 모든 컴포넌트: 존재 + 최소 타깃.
local ids = {
    "gallery.button.primary",
    "gallery.button.secondary",
    "gallery.button.ghost",
    "gallery.button.danger",
    "gallery.button.disabled",
    "gallery.button.selected",
    "gallery.button.icon",
    "gallery.button.size.small",
    "gallery.button.size.medium",
    "gallery.button.size.touch",
    "gallery.icon_button.small",
    "gallery.icon_button.medium",
    "gallery.icon_button.touch",
    "gallery.icon_button.disabled",
    "gallery.toggle.inline",
    "gallery.toggle.row",
    "gallery.segment.left",
    "gallery.segment.center",
    "gallery.segment.right",
    "gallery.row.action",
    "gallery.row.radio",
    "gallery.row.trailing",
    "gallery.row.trailing.button",
    "gallery.row.disabled",
}

local rows: { any } = {}
local min_seen = 999.0
for _, id in ipairs(ids) do
    local state = eguidev.widget(id):expect({ present = true })
    assert(state ~= nil, id .. ": 상태를 읽지 못했습니다")
    local w = state.rect.max.x - state.rect.min.x
    local h = state.rect.max.y - state.rect.min.y
    assert(w >= 24.0 and h >= 24.0, id .. ": 최소 타깃 미달 " .. tostring(w) .. "×" .. tostring(h))
    if w < min_seen then
        min_seen = w
    end
    table.insert(rows, { id = id, w = w, h = h })
end

-- 3) 크기 등급 계약.
local function size_of(id: string): (number, number)
    local s = eguidev.widget(id):state()
    assert(s ~= nil, id .. " 없음")
    return s.rect.max.x - s.rect.min.x, s.rect.max.y - s.rect.min.y
end

local _, small_h = size_of("gallery.button.size.small")
local _, medium_h = size_of("gallery.button.size.medium")
local _, touch_h = size_of("gallery.button.size.touch")
assert(small_h >= 24.0, "Small 등급이 24pt 미만: " .. tostring(small_h))
assert(medium_h >= 28.0, "Medium 등급이 28pt 미만: " .. tostring(medium_h))
assert(touch_h >= 32.0, "Touch 등급이 32pt 미만: " .. tostring(touch_h))

-- 4) 상호작용 — 행 토글이 실제로 값을 뒤집는지 (원복 포함).
--    갤러리 창은 스크롤되므로 화면 밖일 수 있습니다 — 먼저 끌어옵니다.
local toggle_row = eguidev.widget("gallery.toggle.row")
toggle_row:scroll_into_view()
eguidev.wait_frames(2)

local before = toggle_row:state()
assert(before ~= nil, "행 토글 상태를 읽지 못했습니다")
local was_on = before.value == true

toggle_row:click()
eguidev.wait_frames(3)
local after = eguidev.widget("gallery.toggle.row"):state()
assert(after ~= nil, "클릭 후 행 토글이 사라졌습니다")
assert((after.value == true) ~= was_on, "행 토글이 값에 반영되지 않았습니다")

eguidev.widget("gallery.toggle.row"):click()
eguidev.wait_frames(3)
local restored = eguidev.widget("gallery.toggle.row"):state()
assert(restored ~= nil, "원복 후 행 토글이 사라졌습니다")
assert((restored.value == true) == was_on, "행 토글 원복 실패")

return {
    ok = true,
    components = #rows,
    min_width = min_seen,
    sizes = { small = small_h, medium = medium_h, touch = touch_h },
    rows = rows,
    screenshot = eguidev.root:screenshot(),
}
