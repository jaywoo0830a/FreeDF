--!strict
-- More 오버레이 — 열림/닫힘 + 항목 존재 + **계층 리듬**(같은 높이 · 같은 좌측 레일).
--
-- 이 회귀 테스트가 필요한 이유:
--   * 예전에는 오버레이 항목이 전혀 계측되지 않아 측정/검증이 불가능했습니다.
--   * 예전에는 토스트가 우측 상단 입력을 삼켜 메뉴가 **열리지조차** 않았습니다.
--
-- 범위: **구조**를 검증합니다(항목·역할·값·기하). 팝업 안 항목을 클릭해 값을
-- 바꾸는 검증은 이 eguidev 리비전에서 불가능합니다 — 클릭이 팝업을 먼저 닫아
-- 항목이 클릭을 받지 못합니다(docs/eguidev-automation.md의 "메뉴 항목" 항목 참고).

eguidev.root:wait({ frame_at_least = 1 })

local more = eguidev.widget("toolbar.more")
local first_item = eguidev.widget("menu.page.align.left")

-- 메뉴 열기. **앱 세션 첫 클릭은 창 활성화에 소비**되므로 열릴 때까지 클릭합니다
-- (이미 열려 있으면 즉시 true — 불필요한 클릭 없음).
local function open_menu(): boolean
    for _ = 1, 3 do
        if first_item:state() ~= nil then
            return true
        end
        more:hover()
        eguidev.wait_frames(2)
        more:click()
        eguidev.wait_frames(3)
    end
    return first_item:state() ~= nil
end

assert(open_menu(), "More 메뉴를 열지 못했습니다")

-- 1) 모든 항목이 존재하고(활성 상태) 24pt 이상 터치 타깃을 갖습니다.
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

-- 4) 행끼리 **부분적으로 겹치면 안 됩니다**. 겹치면 아래 행이 위 행의 하단 클릭
--    영역을 훔칩니다(실측 결함: Row가 트레일링 자식 ui 때문에 커서를 되돌려
--    stride 17 < 높이 28). 완전 포함(트레일링 ⚙가 행 안에 있는 것)은 정상이라
--    "부분 겹침"만 결함으로 봅니다.
local function contains(outer: any, inner: any): boolean
    return outer.min.x <= inner.min.x
        and outer.min.y <= inner.min.y
        and outer.max.x >= inner.max.x
        and outer.max.y >= inner.max.y
end

local partial: { string } = {}
for _, issue in ipairs(eguidev.root:layout_issues()) do
    if issue.kind == "overlap" and #issue.widgets == 2 then
        local a_id, b_id = issue.widgets[1], issue.widgets[2]
        local both_menu = string.find(a_id, "menu.", 1, true) == 1
            and string.find(b_id, "menu.", 1, true) == 1
        if both_menu then
            local a = eguidev.widget(a_id):state()
            local b = eguidev.widget(b_id):state()
            if a ~= nil and b ~= nil and not (contains(a.rect, b.rect) or contains(b.rect, a.rect)) then
                table.insert(partial, a_id .. " ↔ " .. b_id)
            end
        end
    end
end
assert(#partial == 0, "메뉴 행이 부분적으로 겹칩니다: " .. table.concat(partial, ", "))

-- 5) 다시 눌러 닫히는지 (팝업 토글). 앱 상태를 바꾸지 않는 유일한 닫기 방법입니다.
local closed = false
for _ = 1, 3 do
    more:click()
    eguidev.wait_frames(3)
    if first_item:state() == nil then
        closed = true
        break
    end
end
assert(closed, "More 메뉴가 닫히지 않았습니다")

return {
    ok = true,
    rows = rows,
    partial_overlaps = partial,
    screenshot = eguidev.root:screenshot(),
}
