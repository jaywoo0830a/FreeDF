#!/usr/bin/env bash
# FreeDF API 백엔드 — 빌드 + 시작 (미디어 + Sync v3)
#
# 사용법:
#   ./up.sh              # 이미지 재빌드 + backend/nginx 기동 (DB는 server/db/up.sh 로 먼저)
#   ./up.sh --no-build   # 빌드 없이 기동
#
# backend는 nginx(호스트 네트워크, 8081) 뒤에서만 동작합니다. 네트워크는
# 배포 환경에 따라 자동 선택: Linux Docker Engine(VPS) = 호스트 네트워크로
# DB(127.0.0.1:5432) 직통, Docker Desktop = 브리지 + host-gateway.
# nginx가 /v3·/api 프록시 + /media 서빙을 맡고, 80/443 TLS는 Caddy가 종료합니다.
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

# 배포 환경에 맞는 compose 오버레이 선택 (host / bridge).
COMPOSE_FILES=( $(freedf_compose_files "$DOCKER") )
if [[ " ${COMPOSE_FILES[*]} " == *"docker-compose.host.yml"* ]]; then
    NET_LABEL="host(호스트 네트워크 — DB 직통)"
else
    NET_LABEL="bridge + host-gateway"
fi

# 컨테이너 → PostgreSQL 주소 (모드별로 up.sh가 조립해 넘김).
# 브리지(Desktop): host.docker.internal, 호스트 네트워크(VPS): 127.0.0.1 직통.
CONTAINER_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@host.docker.internal:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
export CONTAINER_DB_URL
HOST_DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${FREEDF_DB_HOST:-127.0.0.1}:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
export HOST_DB_URL

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

# nginx 프록시 전용 노출 (직접 테스트 시 FREEDF_BACKEND_BIND=0.0.0.0).
export FREEDF_BACKEND_BIND="${FREEDF_BACKEND_BIND:-127.0.0.1}"
export FREEDF_BACKEND_PORT="${FREEDF_BACKEND_PORT:-8080}"

set +e
out="$("$DOCKER" compose "${COMPOSE_FILES[@]}" --env-file .env up -d "${BUILD[@]}" backend nginx 2>&1)"
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
        echo "⚠ /health 응답을 받지 못했습니다 — 컨테이너 상태와 최근 로그:"
        "$DOCKER" compose "${COMPOSE_FILES[@]}" ps backend
        "$DOCKER" compose "${COMPOSE_FILES[@]}" logs --tail 30 backend
        echo "(전체 로그: $DOCKER compose ${COMPOSE_FILES[*]} logs -f backend)"
    fi
else
    echo "완료 — 백엔드가 기동되었습니다 (curl이 없어 /health 확인 생략)."
fi

echo "──────────────────────────────────────────────"
echo " 공개 진입점: ${PUBLIC_BASE_URL}"
echo "   Sync v3: PUT /v3/documents/{id}/snapshot · GET /v3/documents/{id}"
echo "   미디어:  /api/media · /media/*"
echo " 흐름: Caddy(80/443, TLS) → nginx(8081) → backend(127.0.0.1:8080)"
echo " backend 네트워크: ${NET_LABEL}"
echo " 인증: X-Api-Key: <backend/.env의 FREEDF_API_KEY>"
echo "──────────────────────────────────────────────"
