--!strict
-- 필기 도구 선택기가 **실제 좌표 기반 클릭**으로 바뀌는지 확인합니다.
-- pen → eraser → pen 왕복으로 선택 상태가 상호 배타적인지 검증합니다.

eguidev.root:wait({ frame_at_least = 1 })

local pen = eguidev.widget("toolbar.tool.pen")
local eraser = eguidev.widget("toolbar.tool.eraser")

-- 세션에 저장된 시작 도구가 무엇이든, 펜을 눌러 기준 상태를 만듭니다.
pen:click()
local pen_on = pen:expect({ selected = true })
eraser:wait({ selected = false })

eraser:click()
eraser:wait({ selected = true })
pen:wait({ selected = false })

-- 원래 도구로 되돌립니다 (스모크 스위트는 서로 독립적이어야 합니다).
pen:click()
local back = pen:expect({ selected = true })
eraser:wait({ selected = false })

assert(pen_on ~= nil and back ~= nil, "도구 선택 상태를 읽지 못했습니다")

return {
    ok = true,
    screenshot = eguidev.root:screenshot(),
}
