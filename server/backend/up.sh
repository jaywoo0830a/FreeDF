#!/usr/bin/env bash
# FreeDF 미디어 백엔드 — 빌드 + 시작
# 사용법: ./up.sh
set -euo pipefail
cd "$(dirname "$0")"

[[ -f .env ]] || ./init.sh
set -a; source .env; set +a   # MEDIA_DIR/DATABASE_URL 등을 실제 값으로

if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다." >&2
    exit 1
fi

# 미디어 폴더 확보 — VPS에서 /srv 경로면 sudo가 필요할 수 있고,
# 개발 머신이면 $HOME 아래로 MEDIA_DIR을 바꾸는 게 편합니다.
# 실패해도 중단하지 않음: compose가 바인드 마운트 디렉터리를 자동 생성합니다.
if ! mkdir -p "$MEDIA_DIR" 2>/dev/null; then
    echo "⚠ '$MEDIA_DIR' 를 만들 수 없습니다 — VPS면 'sudo mkdir -p $MEDIA_DIR' 후," >&2
    echo "  개발 머신이면 .env의 MEDIA_DIR을 \$HOME 아래로 바꾸세요." >&2
fi

# .env를 compose 변수로 넘기고 backend 서비스만 빌드/기동.
"$DOCKER" compose -f ../docker-compose.yml --env-file .env up -d --build backend

echo "완료 — 백엔드가 기동되었습니다."
echo "로그: $DOCKER compose -f ../docker-compose.yml logs -f backend"
