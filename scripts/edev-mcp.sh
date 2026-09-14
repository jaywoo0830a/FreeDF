#!/usr/bin/env bash
# `edev mcp`를 VS Code / Cline에 물릴 때 쓰는 stdio 래퍼.
#
# MCP stdio 서버는 **stdout이 곧 JSON-RPC 프로토콜**입니다. 그래서 이 스크립트는
# stdout에 아무것도 쓰지 않고(진단 메시지는 전부 stderr), 대신 두 가지를
# 보장합니다:
#
#   1) DISPLAY가 없으면 Xvfb 가상 디스플레이를 띄웁니다. VS Code Server(SSH)의
#      확장 호스트에는 DISPLAY가 없어서 eframe/glow가 뜨지 못합니다.
#   2) `--cwd`로 워크스페이스 루트를 고정합니다. MCP 클라이언트가 서버를 어떤
#      cwd로 띄우더라도 EDEV가 `.edev.toml`을 찾을 수 있습니다.
#
# 사용(MCP 클라이언트 설정에 넣는 값):
#   command = "<repo>/scripts/edev-mcp.sh"
#   args    = ["mcp"]
#
# 터미널에서 직접 진단할 때:
#   scripts/edev-mcp.sh mcp -- --help   # (인자 그대로 edev에 전달)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 소프트웨어 GL — GPU 없는 서버/CI에서도 glow 백엔드가 뜹니다.
export LIBGL_ALWAYS_SOFTWARE=1
# wgpu는 자동화 중 유휴 프레임이 멈추는 조합이 있어 glow를 강제합니다.
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
    echo "edev-mcp: 헤드리스 Linux인데 Xvfb가 없습니다. 'sudo apt-get install -y xvfb' 후 다시 시도하세요." >&2
    exit 1
  fi
  XVFB_DISPLAY="${EGUIDEV_XVFB_DISPLAY:-:99}"
  Xvfb "$XVFB_DISPLAY" -screen 0 1600x1000x24 -nolisten tcp \
    >/tmp/freedf-xvfb-mcp.log 2>&1 &
  XVFB_PID=$!
  export DISPLAY="$XVFB_DISPLAY"
  for _ in $(seq 1 50); do
    if xdpyinfo -display "$XVFB_DISPLAY" >/dev/null 2>&1; then break; fi
    sleep 0.1
  done
  # stdout은 프로토콜 전용이므로 진단은 stderr로만 보냅니다.
  echo "edev-mcp: Xvfb $XVFB_DISPLAY (pid $XVFB_PID, log /tmp/freedf-xvfb-mcp.log)" >&2
fi

if ! command -v edev >/dev/null 2>&1; then
  echo "edev-mcp: edev CLI가 없습니다. 'scripts/edev-run.sh --install' 또는 문서의 cargo install 명령을 실행하세요." >&2
  exit 1
fi

SUBCOMMAND="${1:-mcp}"
shift || true

# stdin/stdout은 프로토콜이므로 절대 만지지 않고 그대로 물려줍니다.
exec edev "$SUBCOMMAND" --cwd "$ROOT" "$@"
