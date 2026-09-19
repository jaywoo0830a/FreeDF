--!strict
-- FreeDF 디자인 감사 — 한 번의 호출로 "이미지 + 측정값"을 함께 돌려줍니다.
--
-- 에이전트가 디자인의 장단점을 말하려면 픽셀만으로는 부족합니다. 이 스크립트는
-- 같은 프레임에서 다음을 수집합니다.
--   * `image`          — 실제 프레임 캡처(눈으로 보는 자료)
--   * `widgets`        — 모든 보이는 위젯의 id/역할/라벨/사각형/활성 상태
--   * `layout_issues`  — eguidev가 판정한 overlap/clipping/overflow/text_truncation…
--   * `small_targets`  — 클릭 대상인데 24pt 미만인 컨트롤(터치/접근성 지적)
--   * `disabled`       — 비활성 상태로 보이는 어포던스(사용자가 헷갈리는 지점)
--   * `palette`        — 화면을 그리드 샘플링한 색 사용 빈도(테마 일관성)
--   * `contrast`       — 위젯 내부 격자의 WCAG 대비비(최빈색=배경, 최대 휘도차=전경).
--                        격자 밀도는 위젯 크기에 맞춰 늘어난다(최대 48×24) — 고정
--                        9×5는 넓은 위젯에서 얇은 획을 놓쳐 대비를 과소측정한다.
--
-- 사용:
--   edev eval scripts/design-audit.luau --out-dir tmp/eguidev-screenshots
--   edev eval scripts/design-audit.luau --out-dir tmp/eguidev-screenshots \
--        --arg grid=64x40 --arg contrast_max=30
--
-- 주의: 이 스크립트는 **읽기 전용**입니다(클릭/입력 없음). 상태를 바꾸는
-- 시나리오는 smoketest/의 스크립트를 쓰세요.

local root = eguidev.root
local state = root:wait_capture()

-- ── 색 계산 (WCAG 2.x 상대 휘도 / 대비비) ─────────────────────────────
local function rgb(hex: string): (number, number, number)
    local r = tonumber(string.sub(hex, 2, 3), 16) or 0
    local g = tonumber(string.sub(hex, 4, 5), 16) or 0
    local b = tonumber(string.sub(hex, 6, 7), 16) or 0
    return r, g, b
end

local function linear(channel: number): number
    local c = channel / 255
    if c <= 0.03928 then
        return c / 12.92
    end
    return ((c + 0.055) / 1.055) ^ 2.4
end

local function luminance(hex: string): number
    local r, g, b = rgb(hex)
    return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
end

local function contrast_ratio(a: string, b: string): number
    local la, lb = luminance(a), luminance(b)
    if la < lb then
        la, lb = lb, la
    end
    return (la + 0.05) / (lb + 0.05)
end

local function round(value: number, places: number): number
    local factor = 10 ^ places
    return math.floor(value * factor + 0.5) / factor
end

-- ── 그리드 인자(팔레트 해상도) ────────────────────────────────────────
local grid_x, grid_y = 48, 30
local grid_arg = eguidev.args.grid
if type(grid_arg) == "string" then
    local nx, ny = string.match(grid_arg, "^(%d+)x(%d+)$")
    if nx ~= nil and ny ~= nil then
        grid_x = math.min(tonumber(nx) or grid_x, 128)
        grid_y = math.min(tonumber(ny) or grid_y, 128)
    end
end

local contrast_max = 25
local contrast_arg = eguidev.args.contrast_max
if type(contrast_arg) == "string" or type(contrast_arg) == "number" then
    contrast_max = math.max(0, math.floor(tonumber(contrast_arg) or contrast_max))
end

-- ── 위젯 기하 + 접근성 신호 ──────────────────────────────────────────
local widgets = root:widgets({ visible = true })
local rows = {}
local small_targets = {}
local disabled = {}
local interactive_roles = {
    button = true,
    toggle = true,
    checkbox = true,
    radio = true,
    selectable = true,
    slider = true,
    drag_value = true,
    text_edit = true,
    combo_box = true,
    menu_button = true,
}

local samples: { { id: string, role: string, label: string?, width: number, height: number, area: number } } =
    {}

for _, widget in ipairs(widgets) do
    local s = widget:state()
    if s ~= nil then
        local width = s.rect.max.x - s.rect.min.x
        local height = s.rect.max.y - s.rect.min.y
        table.insert(rows, {
            id = s.id,
            role = s.role,
            label = s.label,
            x = round(s.rect.min.x, 1),
            y = round(s.rect.min.y, 1),
            w = round(width, 1),
            h = round(height, 1),
            enabled = s.enabled,
            selected = s.selected,
        })

        if s.enabled and interactive_roles[s.role] == true and (width < 24 or height < 24) then
            table.insert(small_targets, {
                id = s.id,
                role = s.role,
                label = s.label,
                w = round(width, 1),
                h = round(height, 1),
            })
        end

        if not s.enabled then
            table.insert(disabled, s.id)
        end

        -- 대비 측정 대상: 면적이 큰 순서로 훑습니다.
        table.insert(samples, {
            id = s.id,
            role = s.role,
            label = s.label,
            width = width,
            height = height,
            area = width * height,
        })
    end
end

table.sort(samples, function(a, b)
    return a.area > b.area
end)

-- ── WCAG 대비: 위젯 내부를 여러 점 샘플링합니다 ─────────────────────
-- 한 점만 찍으면 아이콘과 글자 사이 빈 공간을 집어 대비를 1.0으로 잘못
-- 보고합니다. 그래서 내부를 그리드로 훑어
--   배경 = 가장 흔한 색(최빈색), 전경 = 그 배경과 휘도차가 가장 큰 샘플
-- 로 잡습니다. `distinct`가 false면 그 위젯 안에서 뚜렷한 글자/아이콘 픽셀이
-- 잡히지 않았다는 뜻이므로, 대비 수치 대신 이미지를 보고 판단해야 합니다.
local contrast: { any } = {}
for index, sample in ipairs(samples) do
    if index > contrast_max then
        break
    end

    local ok, result = pcall(function()
        local widget = eguidev.widget(sample.id)
        local positions: { { x: number, y: number } } = {}
        -- 그리드 밀도는 위젯 크기에 맞춘다 — 9×5 고정이면 **넓은 위젯에서 얇은 획을
        -- 놓쳐** 대비를 과소측정한다(실측: 넓은 "Bookmarks" 버튼이 2.51로 오측정,
        -- 같은 행 같은 스타일의 "Outline"은 11.68). 넓으면 촘촘히 훑는다.
        local cols = math.max(9, math.min(48, math.floor(sample.width / 6)))
        local lines = math.max(5, math.min(24, math.floor(sample.height / 4)))
        for iy = 0, lines - 1 do
            for ix = 0, cols - 1 do
                table.insert(positions, {
                    x = sample.width * (ix + 0.5) / cols,
                    y = sample.height * (iy + 0.5) / lines,
                })
            end
        end
        return widget:sample_pixels(positions)
    end)

    if ok and result ~= nil and #result > 0 then
        local counts: { [string]: number } = {}
        for _, pixel in ipairs(result) do
            local key = string.sub(pixel.hex, 1, 7)
            counts[key] = (counts[key] or 0) + 1
        end

        local bg, bg_count = nil, -1
        for hex, count in pairs(counts) do
            if count > bg_count then
                bg, bg_count = hex, count
            end
        end

        local fg, ratio = bg, 1
        if bg ~= nil then
            for _, pixel in ipairs(result) do
                local hex = string.sub(pixel.hex, 1, 7)
                local candidate = contrast_ratio(hex, bg)
                if candidate > ratio then
                    fg, ratio = hex, candidate
                end
            end
        end

        table.insert(contrast, {
            id = sample.id,
            role = sample.role,
            label = sample.label,
            fg = fg,
            bg = bg,
            ratio = round(ratio, 2),
            -- 전경이 배경과 같다 = 샘플에 글자/아이콘이 안 걸렸다.
            distinct = bg_count < #result,
            -- AA: 본문 4.5, 큰 글자/UI 구성요소 3.0
            aa_body = ratio >= 4.5,
            aa_ui = ratio >= 3.0,
        })
    end
end

-- ── 팔레트: 화면 전체를 그리드 샘플링해 색 사용 빈도를 셉니다 ─────────
-- (이 리비전의 eguidev는 `sample_grid`를 위젯에만 제공합니다. 뷰포트는
--  좌표 목록을 넘기는 `sample_pixels`를 씁니다.)
local palette: { { hex: string, count: number, share: number } } = {}
local seen: { [string]: number } = {}

local positions: { { x: number, y: number } } = {}
for iy = 0, grid_y - 1 do
    for ix = 0, grid_x - 1 do
        table.insert(positions, {
            x = state.inner_size.x * (ix + 0.5) / grid_x,
            y = state.inner_size.y * (iy + 0.5) / grid_y,
        })
    end
end

local grid_ok, grid = pcall(function()
    return root:sample_pixels(positions)
end)

if grid_ok and grid ~= nil then
    local counts: { { hex: string, count: number, share: number } } = {}
    local total = 0
    for _, pixel in ipairs(grid) do
        local key = string.sub(pixel.hex, 1, 7) -- alpha는 합칩니다(RGB만 집계)
        seen[key] = (seen[key] or 0) + 1
        total = total + 1
    end
    for hex, count in pairs(seen) do
        table.insert(counts, {
            hex = hex,
            count = count,
            share = round(count / math.max(total, 1), 4),
        })
    end
    table.sort(counts, function(a, b)
        return a.count > b.count
    end)
    for index, entry in ipairs(counts) do
        if index > 12 then
            break
        end
        table.insert(palette, entry)
    end
end

-- ── 레이아웃 판정 ────────────────────────────────────────────────────
local layout_ok, issues = pcall(function()
    return root:layout_issues()
end)

local layout_issues = {}
if layout_ok and issues ~= nil then
    for _, issue in ipairs(issues) do
        table.insert(layout_issues, {
            kind = issue.kind,
            widgets = issue.widgets,
            message = issue.message,
        })
    end
end

return {
    viewport = {
        w = round(state.inner_size.x, 1),
        h = round(state.inner_size.y, 1),
        frame = state.frame_count,
        scale = state.pixels_per_point,
    },
    image = root:screenshot(),
    widget_count = #rows,
    widgets = rows,
    layout_issues = layout_issues,
    small_targets = small_targets,
    disabled = disabled,
    palette = palette,
    contrast = contrast,
}
