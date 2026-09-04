#!/usr/bin/env bash
# FreeDF API 백엔드 — 중지
# 사용법: ./down.sh
set -euo pipefail
cd "$(dirname "$0")"
source ../_docker.sh

DOCKER="$(require_docker)"
COMPOSE_FILES=( $(freedf_compose_files "$DOCKER") )
"$DOCKER" compose "${COMPOSE_FILES[@]}" rm -sf backend nginx
echo "완료 — backend/nginx 컨테이너를 중지/제거했습니다."
