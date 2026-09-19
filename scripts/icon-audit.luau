--!strict
-- FreeDF 아이콘 감사 — **글리프 광학 크기**를 렌더 픽셀에서 재는 스크립트.
--
-- 왜 필요한가: Heroicons는 24px 그리드 안에서 글리프별 잉크 크기가 크게 다르다
-- (실측 `minus` 0.9px ~ `paint-brush` 12.4px). 어휘를 고를 때 "잉크 박스가 비슷한
-- 실루엣"을 기준으로 삼는데, 그 판단을 **눈대중이 아니라 측정**으로 한다
-- (`ui/icons.rs`의 어휘 주석이 이 스크립트의 수치를 근거로 삼는다).
--
-- 읽는 법:
--   * `icons[]`       — 첫 잉크 클러스터(=아이콘)의 bbox: `ink_w`×`ink_h` + 세로 중심
--   * `ink_h`/`ink_w` — 아이콘들의 분포(min/mean/max/표준편차) — 작을수록 일정하다
--   * `single_run[]`  — 아이콘과 텍스트가 **붙어** 분리되지 않은 위젯(판정 불가).
--                       아이콘 없이 텍스트만 있는 버튼도 여기로 오므로, 아이콘 누락
--                       판정은 `tests/icons_tests.rs`(어휘 전량 해석)가 담당한다.
--   * `no_ink[]`      — 잉크가 아예 안 잡힌 위젯(가림/너무 작음)
--
-- 사용:
--   scripts/edev-run.sh --config .edev-gui.toml eval scripts/icon-audit.luau \
--     --out-dir tmp/eguidev-screenshots
--   ... --arg window=40     # 왼쪽 창 폭(px) — **아이콘 + 텍스트 시작**이 함께 들어와야
--                           # 클러스터 분리(아이콘 | 텍스트)가 된다. 24px면 아이콘만
--                           # 들어와 분리가 안 되므로 기본값이 40이다.
--
-- 주의: 이 스크립트는 **읽기 전용**입니다(클릭/입력 없음).
-- 참고: eguidev의 `state.label`은 **글리프를 제거한 ASCII 텍스트**를 준다 —
-- 그래서 아이콘 유무는 라벨이 아니라 **픽셀 클러스터**로 판정한다.

local root = eguidev.root
root:wait_capture()

local window_px = 40
local window_arg = eguidev.args.window
if type(window_arg) == "string" or type(window_arg) == "number" then
    window_px = math.max(8, math.floor(tonumber(window_arg) or window_px))
end

-- 잉크 판정 임계값(상대 휘도차) — 안티에일리어싱 가장자리까지 포함한다.
local ink_threshold = 0.08
-- 아이콘과 텍스트를 가르는 빈 열 개수(공백 폭 ≈ 3~4px @14px).
-- 2로 낮추면 아이콘 **내부** 간격(`arrows-pointing-out` 등)을 쪼개므로 3을 유지한다.
-- 커닝이 좁아 분리되지 않는 버튼은 `single_run`으로 보고한다(측정 불가로 정직하게 남김).
local cluster_gap = 3

local function linear(channel: number): number
    if channel <= 0.03928 then
        return channel / 12.92
    end
    return ((channel + 0.055) / 1.055) ^ 2.4
end

local function luminance(hex: string): number
    local r = (tonumber(string.sub(hex, 2, 3), 16) or 0) / 255
    local g = (tonumber(string.sub(hex, 4, 5), 16) or 0) / 255
    local b = (tonumber(string.sub(hex, 6, 7), 16) or 0) / 255
    return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
end

local function round(value: number, places: number): number
    local factor = 10 ^ places
    return math.floor(value * factor + 0.5) / factor
end

type Point = { x: number, y: number }

-- 잉크 점들을 x열 기준으로 묶어 **클러스터**로 나눈다(빈 열 `cluster_gap`개 이상 = 새 클러스터).
local function clusters(points: { Point }, cols: number): { { Point } }
    local by_column: { [number]: { Point } } = {}
    for _, point in ipairs(points) do
        local column = math.floor(point.x)
        local bucket = by_column[column]
        if bucket == nil then
            bucket = {}
            by_column[column] = bucket
        end
        table.insert(bucket, point)
    end

    local groups: { { Point } } = {}
    local current: { Point } = {}
    local empty = cluster_gap + 1
    for column = 0, cols - 1 do
        local bucket = by_column[column]
        if bucket ~= nil then
            empty = 0
            for _, point in ipairs(bucket) do
                table.insert(current, point)
            end
        else
            empty = empty + 1
            if empty == cluster_gap and #current > 0 then
                table.insert(groups, current)
                current = {}
            end
        end
    end
    if #current > 0 then
        table.insert(groups, current)
    end
    return groups
end

local function bbox(points: { Point }): { w: number, h: number, cy: number, x0: number }
    local min_x, max_x = points[1].x, points[1].x
    local min_y, max_y = points[1].y, points[1].y
    for _, point in ipairs(points) do
        min_x = math.min(min_x, point.x)
        max_x = math.max(max_x, point.x)
        min_y = math.min(min_y, point.y)
        max_y = math.max(max_y, point.y)
    end
    return {
        w = round(max_x - min_x, 1),
        h = round(max_y - min_y, 1),
        cy = (min_y + max_y) / 2,
        x0 = round(min_x, 1),
    }
end

type IconRow = {
    id: string,
    label: string,
    ink_w: number,
    ink_h: number,
    center_dy: number,
    pixels: number,
}

local icons: { IconRow } = {}
local single_run: { { id: string, label: string, runs: number } } = {}
local no_ink: { string } = {}

for _, widget in ipairs(root:widgets({ visible = true })) do
    local state = widget:state()
    if state ~= nil and state.label ~= nil and state.label ~= "" then
        local width = state.rect.max.x - state.rect.min.x
        local height = state.rect.max.y - state.rect.min.y
        local cols = math.min(math.floor(window_px), math.floor(width))
        local lines = math.max(4, math.floor(height))
        if cols >= 8 and lines >= 4 then
            local positions: { { x: number, y: number } } = {}
            for iy = 0, lines - 1 do
                for ix = 0, cols - 1 do
                    table.insert(positions, {
                        x = ix + 0.5,
                        y = (iy + 0.5) * height / lines,
                    })
                end
            end
            local ok, pixels = pcall(function()
                return widget:sample_pixels(positions)
            end)
            if ok and pixels ~= nil and #pixels > 0 then
                local counts: { [string]: number } = {}
                for _, pixel in ipairs(pixels) do
                    local key = string.sub(pixel.hex, 1, 7)
                    counts[key] = (counts[key] or 0) + 1
                end
                local bg: string? = nil
                local bg_count = -1
                for hex, count in pairs(counts) do
                    if count > bg_count then
                        bg, bg_count = hex, count
                    end
                end
                if bg ~= nil then
                    local bg_lum = luminance(bg)
                    local points: { Point } = {}
                    for index, pixel in ipairs(pixels) do
                        local hex = string.sub(pixel.hex, 1, 7)
                        local diff = luminance(hex) - bg_lum
                        if diff < 0 then
                            diff = -diff
                        end
                        if diff > ink_threshold then
                            local ix = (index - 1) % cols
                            local iy = math.floor((index - 1) / cols)
                            table.insert(points, {
                                x = ix + 0.5,
                                y = (iy + 0.5) * height / lines,
                            })
                        end
                    end
                    if #points == 0 then
                        table.insert(no_ink, state.id)
                    else
                        local groups = clusters(points, cols)
                        if #groups >= 2 then
                            local box = bbox(groups[1])
                            table.insert(icons, {
                                id = state.id,
                                label = state.label,
                                ink_w = box.w,
                                ink_h = box.h,
                                center_dy = round(box.cy - height / 2, 1),
                                pixels = #groups[1],
                            })
                        else
                            table.insert(single_run, {
                                id = state.id,
                                label = state.label,
                                runs = #groups,
                            })
                        end
                    end
                end
            end
        end
    end
end

table.sort(icons, function(a: IconRow, b: IconRow): boolean
    return a.ink_h < b.ink_h
end)

local function stats(values: { number }): { min: number, mean: number, max: number, sd: number }
    if #values == 0 then
        return { min = 0, mean = 0, max = 0, sd = 0 }
    end
    local min, max, sum = values[1], values[1], 0
    for _, value in ipairs(values) do
        min = math.min(min, value)
        max = math.max(max, value)
        sum = sum + value
    end
    local mean = sum / #values
    local variance = 0
    for _, value in ipairs(values) do
        variance = variance + (value - mean) ^ 2
    end
    return {
        min = round(min, 1),
        mean = round(mean, 1),
        max = round(max, 1),
        sd = round(math.sqrt(variance / #values), 2),
    }
end

local ink_h_values: { number } = {}
local ink_w_values: { number } = {}
local center_values: { number } = {}
for _, row in ipairs(icons) do
    table.insert(ink_h_values, row.ink_h)
    table.insert(ink_w_values, row.ink_w)
    table.insert(center_values, row.center_dy)
end

return {
    count = #icons,
    icons = icons,
    ink_h = stats(ink_h_values),
    ink_w = stats(ink_w_values),
    center_dy = stats(center_values),
    single_run = single_run,
    no_ink = no_ink,
    window_px = window_px,
}
