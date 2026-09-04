#!/usr/bin/env bash
# FreeDF API 백엔드 — 빌드 + 시작 (미디어 + Sync v3)
#
# 사용법:
#   ./up.sh              # 이미지 재빌드 + 기동 (DB는 server/db/up.sh 로 먼저)
#   ./up.sh --no-build   # 빌드 없이 기동
#
# 컨테이너는 브리지 네트워크에서 8080을 게시하고, PostgreSQL은
# host.docker.internal(host-gateway)로 접근합니다 — Linux VPS와
# Docker Desktop(WSL) 모두 동일하게 동작합니다.
set -euo pipefail
cd "$(dirname "$0")"
source ../_docker.sh

# DB 설정(server/db/.env)은 필수.
if [[ ! -f ../db/.env ]]; then
    echo "⚠ server/db/.env 가 없습니다 — server/db/init.sh 를 먼저 실행하세요." >&2
    exit 1
fi
set -a
source ../db/.env
[[ -f .env ]] || ./init.sh
source .env
set +a

DOCKER="$(require_docker)"

# 컨테이너 → 호스트 PostgreSQL 주소 (host-gateway: VPS·Docker Desktop 공용).
CONTAINER_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@host.docker.internal:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
export CONTAINER_DB_URL

# 호스트에서 PostgreSQL이 열려 있는지 미리 확인 (경고만 — 기동은 진행).
if ! (timeout 2 bash -c "</dev/tcp/127.0.0.1/${FREEDF_DB_PORT:-5432}" >/dev/null 2>&1); then
    echo "⚠ PostgreSQL(127.0.0.1:${FREEDF_DB_PORT:-5432})이 응답하지 않습니다." >&2
    echo "  server/db/up.sh 로 DB를 먼저 띄우세요 (없으면 백엔드가 재시작 루프에 빠질 수 있습니다)." >&2
fi

# 미디어 폴더 확보 — 컨테이너는 이 경로를 /srv/freedf-server/media 로 마운트합니다.
if ! mkdir -p "$MEDIA_DIR" 2>/dev/null; then
    echo "⚠ '$MEDIA_DIR' 를 만들 수 없습니다 — VPS면 'sudo mkdir -p $MEDIA_DIR' 후," >&2
    echo "  개발 머신이면 .env의 MEDIA_DIR을 \$HOME 아래로 바꾸세요." >&2
fi

BUILD=(--build)
[[ "${1:-}" == "--no-build" ]] && BUILD=()

set +e
out="$(CONTAINER_DB_URL="$CONTAINER_DB_URL" "$DOCKER" compose -f ../docker-compose.yml --env-file .env up -d "${BUILD[@]}" backend 2>&1)"
rc=$?
set -e
if [[ $rc -ne 0 ]]; then
    echo "$out" >&2
    if is_distro_mount_error "$out"; then
        echo >&2
        echo "⚠ Docker Desktop이 이 WSL 배포판의 파일을 마운트할 수 없습니다." >&2
        echo "  Docker Desktop → Settings → Resources → WSL integration 에서" >&2
        echo "  이 배포판을 활성화하거나, MEDIA_DIR을 Windows 경로(/mnt/c/...)로" >&2
        echo "  바꾼 뒤 다시 시도하세요." >&2
    fi
    exit 1
fi

# /health 대기 (게시 포트)
if command -v curl >/dev/null 2>&1; then
    echo "백엔드 기동 — /health 확인 중..."
    ok=0
    for _ in $(seq 1 30); do
        if curl -sf "http://127.0.0.1:${FREEDF_BACKEND_PORT:-8080}/health" >/dev/null 2>&1; then
            ok=1
            break
        fi
        sleep 1
    done
    if [[ $ok == 1 ]]; then
        echo "✓ 백엔드 준비 완료 — http://127.0.0.1:${FREEDF_BACKEND_PORT:-8080}"
    else
        echo "⚠ /health 응답을 받지 못했습니다. 로그 확인:"
        echo "  $DOCKER compose -f ../docker-compose.yml logs -f backend"
    fi
else
    echo "완료 — 백엔드가 기동되었습니다 (curl이 없어 /health 확인 생략)."
fi

echo "──────────────────────────────────────────────"
echo " Sync v3 API:"
echo "   PUT /v3/documents/{id}/snapshot   GET /v3/documents/{id}"
echo "   GET /v3/documents/{id}/changes    /v3/objects/* (CAS)"
echo " 미디어 API: /api/media"
echo " 인증: X-Api-Key: <backend/.env의 FREEDF_API_KEY>"
echo " nginx로만 노출하려면 backend/.env에 FREEDF_BACKEND_BIND=127.0.0.1 추가"
echo "   (8080이 0.0.0.0으로 게시되면 API 키 인증만으로 보호됩니다)"
echo "──────────────────────────────────────────────"
