#!/usr/bin/env bash
# FreeDF DB — 중지
# 사용법:
#   ./down.sh            # 컨테이너만 중지 (데이터 볼륨 유지)
#   ./down.sh --wipe     # 중지 + 데이터 볼륨 완전 삭제
set -euo pipefail
cd "$(dirname "$0")"

if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다." >&2
    exit 1
fi

if [[ "${1:-}" == "--wipe" ]]; then
    echo "PostgreSQL 중지 + 데이터 볼륨 삭제..."
    "$DOCKER" compose down -v
    echo "완료 — 데이터가 완전히 삭제되었습니다."
else
    "$DOCKER" compose down
    echo "완료 — 컨테이너만 중지했습니다 (데이터 볼륨 유지)."
    echo "데이터까지 지우려면: ./down.sh --wipe"
fi
