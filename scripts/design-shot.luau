--!strict
-- FreeDF 화면 캡처 — 에이전트가 "실제 프레임"을 눈으로 보게 하는 용도.
--
-- 스크린샷을 화면에서 긁어오는 대신, eguidev가 앱 안에서 프레임을 잡아
-- `ImageRef`로 돌려줍니다. 반환값에서 도달 가능한 모든 `ImageRef`는
--   - CLI(`edev eval`)  → `--out-dir` 아래 파일로 저장되고 JSON에 경로가 실립니다
--   - MCP(`script_eval`) → 이미지 콘텐츠 블록으로 에이전트에게 그대로 전달됩니다
--
-- 사용:
--   edev eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots
--   edev eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots \
--        --arg widget=canvas.surface
--
-- `widget` 인자를 주면 뷰포트 전체 대신 그 위젯만 잘라서 반환합니다
-- (디자인 리뷰에서 가장 저렴하게 "집중된 시각 자료"를 만드는 방법).

local images: { [string]: any } = {}

local widget_id = eguidev.args.widget
if type(widget_id) == "string" and widget_id ~= "" then
    -- `screenshot()`은 새 캡처를 기다렸다가 위젯 사각형으로 잘라냅니다.
    -- 위젯이 없으면 not_found로 실패하므로 조용히 빈 데이터를 돌려주지 않습니다.
    images[widget_id] = eguidev.widget(widget_id):screenshot()
    return images
end

-- 첫 프레임이 완성될 때까지 기다린 뒤 모든 뷰포트를 캡처합니다.
eguidev.root:wait_capture()

for _, viewport in ipairs(eguidev.viewports()) do
    local label: string = viewport.id
    local state = viewport:state()
    if state ~= nil and state.name ~= nil then
        label = state.name
    end
    images[label] = viewport:screenshot()
end

return images
