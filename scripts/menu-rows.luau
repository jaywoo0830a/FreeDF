-- 행 우측 영역을 **위젯 크롭**으로 떠서 실제로 무엇이 그려지는지 봅니다.
eguidev.root:wait({ frame_at_least = 1 })
local more = eguidev.widget("toolbar.more")
local item = eguidev.widget("menu.page.align.left")
for _ = 1, 3 do
    if item:state() ~= nil then break end
    more:hover()
    eguidev.wait_frames(2)
    more:click()
    eguidev.wait_frames(3)
end

local function info(id: string)
    local s = eguidev.widget(id):state()
    if s == nil then
        return { id = id, missing = true, x = 0, y = 0, w = 0, h = 0, value_text = "" }
    end
    return {
        id = id,
        missing = false,
        x = s.rect.min.x,
        y = s.rect.min.y,
        w = s.rect.max.x - s.rect.min.x,
        h = s.rect.max.y - s.rect.min.y,
        value_text = s.value_text,
    }
end

return {
    media = info("menu.view.media_panel"),
    dict = info("menu.lookup.dictionary"),
    dwell = info("menu.input.focus_dwell"),
    gear = info("menu.input.focus_settings"),
    hud = info("menu.diag.debug_hud"),
    media_img = eguidev.widget("menu.view.media_panel"):screenshot(),
    dwell_img = eguidev.widget("menu.input.focus_dwell"):screenshot(),
    issues = eguidev.root:layout_issues(),
}
