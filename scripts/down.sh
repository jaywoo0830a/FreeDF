#!/usr/bin/env bash
# FreeDF — 로컬 PostgreSQL 서버 중지 (Docker, Mac/Linux/Windows WSL2 공통)
# 사용법:
#   ./scripts/down.sh          # 중지 (데이터 볼륨 유지)
#   ./scripts/down.sh --wipe   # 중지 + 데이터 완전 삭제
set -euo pipefail

if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다." >&2
    exit 1
fi

cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--wipe" ]]; then
    echo "PostgreSQL 중지 + 데이터 볼륨 삭제..."
    "$DOCKER" compose down -v
    echo "완료 — 데이터가 완전히 삭제되었습니다."
else
    "$DOCKER" compose down
    echo "완료 — 컨테이너만 중지했습니다 (데이터 볼륨 freedf_pgdata 유지)."
    echo "데이터까지 지우려면: ./scripts/down.sh --wipe"
fi
