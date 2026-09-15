#!/usr/bin/env bash
# EDEV를 헤드리스 환경(Linux)에서 돌리기 위한 얇은 래퍼.
#
# eframe/glow는 실제 디스플레이(또는 Xvfb)가 필요합니다. 이 스크립트는
#   - Xvfb 가상 디스플레이를 띄우고 (이미 DISPLAY가 있으면 그대로 사용)
#   - 소프트웨어 GL(llvmpipe)로 렌더링한 뒤
#   - 넘겨받은 edev 명령을 실행하고 Xvfb를 정리합니다.
#
# 사용:
#   scripts/edev-run.sh dump
#   scripts/edev-run.sh eval tmp/probe.luau --out-dir tmp/probe-out
#   scripts/edev-run.sh smoke
#
# 사전 준비:
#   sudo apt-get install -y xvfb libxcb1 libxkbcommon-x11-0 libxcursor1 \
#                           libxrandr2 libxi6 libgl1-mesa-dri
#   cargo install --git https://github.com/jaywoo0830a/eguidev \
#                 --rev 84ab2da60b36fa5f4235c0792b78e5535b95700a edev

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ── 전역 직렬화 ─────────────────────────────────────────────────────────────
# edev는 **실제 창/디스플레이**를 씁니다. 두 인스턴스를 동시에 돌리면 서로의
# 프레임/스크린샷을 깨뜨립니다(실측: 동시 실행 시 스크린샷 타임아웃).
# 호출 순서를 사람이 지키게 하지 말고, 여기서 flock으로 강제합니다.
LOCK_FILE="${TMPDIR:-/tmp}/freedf-edev.lock"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "edev: 다른 실행이 끝나기를 기다립니다 (직렬 실행)…" >&2
  if ! flock -w 900 9; then
    echo "오류: 다른 edev 실행이 15분 안에 끝나지 않았습니다." >&2
    exit 1
  fi
fi

# 소프트웨어 GL — GPU 없는 CI/VPS에서도 glow 백엔드가 뜹니다.
export LIBGL_ALWAYS_SOFTWARE=1
# 대화형 창 관리자가 없으므로 winit이 X11을 그대로 쓰게 둡니다.
export FREEDF_RENDERER="${FREEDF_RENDERER:-glow}"

XVFB_PID=""
cleanup() {
  if [[ -n "$XVFB_PID" ]]; then
    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# Xvfb가 필요한 경우는 **헤드리스 Linux뿐**입니다.
# macOS는 DISPLAY가 비어 있어도 실제 디스플레이가 있고, Windows도 마찬가지입니다.
# (Wayland 세션도 WAYLAND_DISPLAY가 있으므로 Xvfb가 필요 없습니다.)
needs_xvfb() {
  [[ "$(uname -s)" == "Linux" && -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]
}

if needs_xvfb; then
  if ! command -v Xvfb >/dev/null 2>&1; then
    echo "오류: 헤드리스 Linux인데 Xvfb도 설치되어 있지 않습니다." >&2
    echo "      sudo apt-get install -y xvfb" >&2
    exit 1
  fi
  XVFB_DISPLAY=":99"
  # 락을 잡고도 :99에 서버가 남아 있으면(이전 실행이 비정상 종료) 재사용합니다.
  if ! xdpyinfo -display "$XVFB_DISPLAY" >/dev/null 2>&1; then
    Xvfb "$XVFB_DISPLAY" -screen 0 1600x1000x24 -nolisten tcp 9>&- >/tmp/freedf-xvfb.log 2>&1 &
    XVFB_PID=$!
  fi
  export DISPLAY="$XVFB_DISPLAY"
  # X 서버가 소켓을 열 때까지 대기.
  for _ in $(seq 1 50); do
    if xdpyinfo -display "$XVFB_DISPLAY" >/dev/null 2>&1; then break; fi
    sleep 0.1
  done
  # stdout은 `edev eval`의 JSON 출력이 지나가는 통로이므로 진단은 stderr로 보냅니다.
  echo "Xvfb: $XVFB_DISPLAY (pid $XVFB_PID)" >&2
fi

if ! command -v edev >/dev/null 2>&1; then
  echo "오류: edev CLI를 찾을 수 없습니다." >&2
  echo "  cargo install --git https://github.com/jaywoo0830a/eguidev \\" >&2
  echo "                --rev 84ab2da60b36fa5f4235c0792b78e5535b95700a edev" >&2
  exit 1
fi

exec edev "$@"
