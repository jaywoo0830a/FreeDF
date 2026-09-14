-- More 오버레이 시각 검토용 캡처 (디자인 리뷰)
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
return {
    open = item:state() ~= nil,
    screenshot = eguidev.root:screenshot(),
    issues = eguidev.root:layout_issues(),
    rows = (function()
        local out = {}
        for _, w in ipairs(eguidev.widgets({ visible = true })) do
            if string.sub(w.id, 1, 5) == "menu." then
                local s = w:state()
                if s ~= nil then
                    table.insert(out, {
                        id = w.id,
                        x = s.rect.min.x,
                        y = s.rect.min.y,
                        w = s.rect.max.x - s.rect.min.x,
                        h = s.rect.max.y - s.rect.min.y,
                    })
                end
            end
        end
        return out
    end)(),
}
