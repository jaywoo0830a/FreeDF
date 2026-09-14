#!/usr/bin/env bash
# 빠른 디자인 스크린샷 — 고정 대기 없이 완료를 폴링합니다.
#
# 사용법:
#   scripts/shot.sh                          # 뷰포트 전체
#   scripts/shot.sh settings.window draw     # 위젯 크롭 + 설정 탭
#   FREEDF_UI_GALLERY=1 scripts/shot.sh ui_gallery.window
#
# 왜 별도 스크립트인가: 디자인 리뷰는 "찍고 보기"를 빠르게 반복해야 합니다.
# 빌드는 캐시를 쓰고, 앱이 뜨는 즉시 캡처합니다(90초 고정 대기 금지).
set -euo pipefail
cd "$(dirname "$0")/.."

widget="${1:-}"
tab="${2:-}"
out="tmp/eguidev-screenshots"
mkdir -p "$out"

# 1) 미리 빌드 — 이후 실행은 캐시로 즉시 끝납니다.
cargo build -q -p freedf --features dev-automation 2>/dev/null || \
    cargo build -p freedf --features dev-automation

# 2) 평가 실행 (백그라운드) + JSON 파일이 완성될 때까지 폴링
json="$(mktemp)"
args=()
[[ -n "$widget" ]] && args+=(--arg "widget=$widget")
# 탭 지정이 없으면 기존 값을 존중 (사용자가 미리 내보낸 경우 덮어쓰지 않음)
if [[ -n "$tab" ]]; then export FREEDF_SETTINGS_TAB="$tab"; fi
./scripts/edev-run.sh eval scripts/design-shot.luau \
    "${args[@]}" --out-dir "$out" >"$json" 2>&1 &
pid=$!
for _ in $(seq 1 600); do
    # JSON은 `success` 키가 찍히면 완성입니다.
    if grep -q '"success"' "$json" 2>/dev/null; then break; fi
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 0.25
done
wait "$pid" 2>/dev/null || true

grep -o '"file": "[^"]*"' "$json" | sed 's/"file": //' || tail -c 400 "$json"
