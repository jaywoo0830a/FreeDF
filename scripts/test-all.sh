#!/usr/bin/env bash
# FreeDF 전체 검증 — **모든 단계를 순차로** 돌립니다.
#
# 왜 순차인가: `edev`는 실제 창/디스플레이를 쓰므로 동시 실행이 서로를 깨뜨립니다.
# (`scripts/edev-run.sh`가 flock으로 직렬화하지만, 여기서도 명시적으로 순서를 갖습니다.)
#
#   1) 헤드리스 계약 테스트 (`cargo test --features dev-automation`)
#      — ui::kit / ui::a11y / tokens 계약을 창 없이 검증 (76개)
#   2) eguidev 스모크 스위트 (10_launch · 20_ink_tool_picker · 30_more_menu · 40_ui_gallery)
#   3) UI 갤러리 계약 스캔 (FREEDF_UI_GALLERY=1 — 팝업 클릭이 불가능해서 별도 경로)
#   4) 기본 프로필 컴파일 확인 (`cargo check -p freedf`, dev-automation 꺼짐)
#
# 사용: ./scripts/test-all.sh [--verbose]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERBOSE="${1:-}"
LOG_DIR="${TMPDIR:-/tmp}/freedf-test-all"
mkdir -p "$LOG_DIR"

FAILED=0
step() { printf '\n=== %s ===\n' "$1"; }
report() { # $1=이름 $2=종료코드 $3=로그
  if [[ "$2" -eq 0 ]]; then
    printf '[PASS] %s\n' "$1"
  else
    printf '[FAIL] %s (exit %s) — 로그: %s\n' "$1" "$2" "$3"
    FAILED=1
  fi
}

# 1) 헤드리스 계약 테스트 -----------------------------------------------------
step "1/4 헤드리스 계약 테스트: cargo test -p freedf --features dev-automation --bin freedf"
LOG="$LOG_DIR/headless.log"
cargo test -p freedf --features dev-automation --bin freedf >"$LOG" 2>&1
report "headless contract tests" $? "$LOG"
grep -E '^test result' "$LOG" || true
[[ "$VERBOSE" == "--verbose" ]] && tail -30 "$LOG"

# 2) eguidev 스모크 스위트 ----------------------------------------------------
step "2/4 eguidev 스모크 스위트: edev smoke"
LOG="$LOG_DIR/smoke.log"
./scripts/edev-run.sh smoke >"$LOG" 2>&1
report "edev smoke" $? "$LOG"
grep -E '^\[(PASS|FAIL)\]' "$LOG" || true

# 3) UI 갤러리 계약 스캔 ------------------------------------------------------
step "3/4 UI 갤러리 계약 스캔: FREEDF_UI_GALLERY=1"
LOG="$LOG_DIR/gallery.log"
./scripts/ui-gallery-check.sh >"$LOG" 2>&1
report "ui gallery contract scan" $? "$LOG"
[[ "$VERBOSE" == "--verbose" ]] && tail -30 "$LOG"

# 4) 기본 프로필 컴파일 확인 ---------------------------------------------------
step "4/4 기본 프로필 컴파일: cargo check -p freedf (dev-automation 없이)"
LOG="$LOG_DIR/check-default.log"
cargo check -p freedf >"$LOG" 2>&1
report "cargo check (default)" $? "$LOG"
[[ "$VERBOSE" == "--verbose" ]] && tail -20 "$LOG"

printf '\n'
if [[ "$FAILED" -eq 0 ]]; then
  echo "전체 통과 ✅ (로그: $LOG_DIR)"
else
  echo "실패한 단계가 있습니다 ❌ (로그: $LOG_DIR)"
fi
exit "$FAILED"
