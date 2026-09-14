--!strict
-- More 오버레이 — 항목 존재 + **계층 리듬**(같은 높이·같은 좌측 레일) + 상태 왕복을 검증합니다.
--
-- 이 회귀 테스트가 필요한 이유:
--   * 예전에는 오버레이 항목이 전혀 계측되지 않아 측정/검증이 불가능했습니다.
--   * 예전에는 토스트가 우측 상단 입력을 삼켜 메뉴가 **열리지조차** 않았습니다.

eguidev.root:wait({ frame_at_least = 1 })

-- 메뉴를 엽니다. (토스트가 떠 있어도 열려야 합니다 — 입력 통과 회귀 방지.)
-- 주의: **앱 세션당 첫 클릭은 창 활성화에 소비**되어 사라집니다(실측: 1번째 클릭
-- 무시, 2번째부터 정상). 그래서 "열릴 때까지" 클릭합니다 — 이미 열려 있으면
-- 대기가 즉시 성공하므로 추가 클릭이 나가지 않습니다.
local more = eguidev.widget("toolbar.more")
local first_item = eguidev.widget("menu.page.align.left")

-- 팝업은 클릭 **다음 프레임**에 그려지므로, 대기는 위젯 수준으로 합니다.
-- (전역 `eguidev.wait(fn)`은 이 앱에서 조기 종료됩니다.)
local opened = false
for _ = 1, 3 do
    more:hover()
    eguidev.wait_frames(2)
    more:click()
    eguidev.wait_frames(3)
    if first_item:state() ~= nil then
        opened = true
        break
    end
end
assert(opened, "More 메뉴를 열지 못했습니다")

-- 1) 모든 항목이 존재하고 24pt 이상 터치 타깃을 갖습니다.
local ids = {
    "menu.page.align.left",
    "menu.page.align.center",
    "menu.page.align.right",
    "menu.view.media_panel",
    "menu.lookup.dictionary",
    "menu.input.focus_dwell",
    "menu.input.focus_settings",
    "menu.input.macros",
    "menu.input.gamepad",
    "menu.server.media",
    "menu.maintenance.clear.download_cache",
    "menu.maintenance.clear.canvas_cache",
    "menu.diag.debug_hud",
}

local heights: { [number]: boolean } = {}
local left_rail: { [number]: boolean } = {}
local rows: { any } = {}

for _, id in ipairs(ids) do
    local state = eguidev.widget(id):expect({ present = true, enabled = true })
    assert(state ~= nil, id .. ": 상태를 읽지 못했습니다")
    local h = state.rect.max.y - state.rect.min.y
    local w = state.rect.max.x - state.rect.min.x
    assert(h >= 24 or w >= 24, id .. ": 터치 타깃이 24pt 미만입니다")
    heights[h] = true
    -- 정렬 3-세그먼트와 트레일링 ⚙는 우측이라 좌측 레일 비교에서 제외합니다.
    if not string.find(id, "align", 1, true) and not string.find(id, "focus_settings", 1, true) then
        left_rail[state.rect.min.x] = true
    end
    table.insert(rows, { id = id, x = state.rect.min.x, y = state.rect.min.y, w = w, h = h })
end

-- 2) 행 높이가 한 가지여야 합니다 (행마다 키가 달라 보이던 문제의 회귀 방지).
local height_count = 0
for _ in pairs(heights) do
    height_count += 1
end
assert(height_count == 1, "메뉴 행 높이가 " .. tostring(height_count) .. "가지로 갈렸습니다")

-- 3) 좌측 정렬 레일이 하나여야 합니다 (들여쓰기 흔들림 방지).
local rail_count = 0
for _ in pairs(left_rail) do
    rail_count += 1
end
assert(rail_count == 1, "메뉴 항목의 좌측 정렬이 " .. tostring(rail_count) .. "가지입니다")

-- 4) 토글 상태 왕복 — 스모크 스위트는 서로 독립적이어야 하므로 원래 값으로 되돌립니다.
local hud = eguidev.widget("menu.diag.debug_hud")
local before = hud:state()
assert(before ~= nil, "debug_hud 상태를 읽지 못했습니다")
local was_on = before.value == true

hud:click()
hud:wait({ value = not was_on })
hud:click()
hud:wait({ value = was_on })

return {
    ok = true,
    rows = rows,
    screenshot = eguidev.root:screenshot(),
}
