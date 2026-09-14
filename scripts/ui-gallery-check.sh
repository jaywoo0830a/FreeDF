#!/usr/bin/env bash
# UI 컴포넌트 계약 검증 — 갤러리를 연 상태로 스모크 40을 돌립니다.
#
# 왜 별도 경로인가: 갤러리 입구가 egui **팝업(More 메뉴)** 안에 있으면 자동화가
# 클릭할 수 없습니다(실측). 그래서 `FREEDF_UI_GALLERY=1` 환경변수로 시작 시 열고,
# 같은 스크립트(`smoketest/40_ui_gallery.luau`)를 돌립니다.
#
# 사용법:
#   ./scripts/ui-gallery-check.sh              # 계약 스캔 + 스크린샷
#   ./scripts/ui-gallery-check.sh --shot-only  # 스크린샷만
set -euo pipefail

cd "$(dirname "$0")/.."

export FREEDF_UI_GALLERY=1

if [[ "${1:-}" == "--shot-only" ]]; then
    exec ./scripts/edev-run.sh eval smoketest/40_ui_gallery.luau --out-dir tmp/eguidev-screenshots
fi

./scripts/edev-run.sh eval smoketest/40_ui_gallery.luau --out-dir tmp/eguidev-screenshots
