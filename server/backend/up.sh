#!/usr/bin/env bash
# FreeDF 미디어 백엔드 — 빌드 + 시작
# 사용법: ./up.sh
set -euo pipefail
cd "$(dirname "$0")"

[[ -f .env ]] || ./init.sh

if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다." >&2
    exit 1
fi

mkdir -p "${MEDIA_DIR:-/srv/freedf-server/media}" 2>/dev/null || true

# .env를 compose 변수로 넘기고 backend 서비스만 빌드/기동.
"$DOCKER" compose -f ../docker-compose.yml --env-file .env up -d --build backend

echo "완료 — 백엔드가 기동되었습니다."
echo "로그: $DOCKER compose -f ../docker-compose.yml logs -f backend"
