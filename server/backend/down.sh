#!/usr/bin/env bash
# FreeDF 미디어 백엔드 — 중지
# 사용법: ./down.sh
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

"$DOCKER" compose -f ../docker-compose.yml rm -sf backend
echo "완료 — backend 컨테이너를 중지/제거했습니다."
