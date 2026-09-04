#!/usr/bin/env bash
# FreeDF API 백엔드 — .env 생성 (미디어 + Sync v3)
#
# 사용법:
#   ./init.sh                                          # .env 생성 (API 키 자동 생성)
#   PUBLIC_BASE_URL=https://media.mydomain.com ./init.sh  # VPS 도메인 지정
#   BIND=0.0.0.0:8080 ./init.sh                        # nginx 없이 직접 테스트 시
#
# DATABASE_URL은 같은 호스트의 server/db/.env 에서 자동 조립됩니다.
# (호스트에서 직접 실행할 때 사용 — 컨테이너는 up.sh가 host-gateway 주소로 교체)
set -euo pipefail
cd "$(dirname "$0")"

if [[ -f .env ]]; then
    echo ".env 이미 존재합니다 — 수정하려면 직접 편집 후 ./up.sh"
    source .env
else
    DB_URL=""
    if [[ -f ../db/.env ]]; then
        source ../db/.env
        DB_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
    fi
    DATABASE_URL="${DATABASE_URL:-${DB_URL:-postgres://freedf:CHANGEME@localhost:5432/freedf}}"
    # 공개 URL — 반드시 VPS 도메인/IP로. FreeDF 앱이 미디어 재생에 사용합니다.
    PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-https://media.example.com}"
    # 파일 저장 경로 (nginx가 같은 경로를 서빙)
    MEDIA_DIR="${MEDIA_DIR:-/srv/freedf-server/media}"
    # 바인딩 — nginx 프록시 구성이면 127.0.0.1:8080 그대로.
    # nginx 없이 직접 접속해 테스트하려면 0.0.0.0:8080.
    BIND="${BIND:-127.0.0.1:8080}"
    # API 키: 지정 없으면 랜덤 생성 (FreeDF 앱 server.json에 같은 값 입력)
    if [[ -z "${FREEDF_API_KEY:-}" ]]; then
        FREEDF_API_KEY="$(openssl rand -hex 16 2>/dev/null \
            || od -An -N16 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n' \
            || echo "dev-key-$(date +%s)")"
    fi
    cat > .env <<EOF
# server/backend 설정 — ./init.sh가 생성 (자유롭게 수정 가능)
# FreeDF 메인 DB 연결 (호스트에서 직접 실행할 때 사용 — server/db/.env 와 일치)
DATABASE_URL=${DATABASE_URL}
# 파일 저장 경로 (nginx가 같은 경로를 서빙)
MEDIA_DIR=${MEDIA_DIR}
# 클라이언트에 알려줄 공개 URL — 반드시 VPS 도메인/IP로 변경
PUBLIC_BASE_URL=${PUBLIC_BASE_URL}
# API 키 — FreeDF 앱의 server.json에 같은 값 (X-Api-Key)
FREEDF_API_KEY=${FREEDF_API_KEY}
# 백엔드 바인딩 (nginx가 127.0.0.1:8080으로 프록시)
BIND=${BIND}
EOF
    echo ".env 생성 완료"
fi

echo "──────────────────────────────────────────────"
echo " DATABASE_URL     = ${DATABASE_URL}"
echo " MEDIA_DIR        = ${MEDIA_DIR}"
echo " PUBLIC_BASE_URL  = ${PUBLIC_BASE_URL}"
echo " FREEDF_API_KEY   = ${FREEDF_API_KEY}"
echo " BIND             = ${BIND}"
echo "──────────────────────────────────────────────"
if [[ "${DATABASE_URL}" == *"CHANGEME"* ]]; then
    echo " ⚠ DATABASE_URL에 CHANGEME가 남아 있습니다 — server/db/up.sh 를 먼저"
    echo "   실행하거나(같은 호스트) .env를 직접 수정하세요."
elif [[ "${PUBLIC_BASE_URL}" == *"media.example.com"* ]]; then
    echo " ⚠ PUBLIC_BASE_URL이 예시 도메인입니다 — VPS 도메인/IP로 변경하세요."
fi
echo " FreeDF 앱의 Server 설정(server.json)에 PUBLIC_BASE_URL과 API 키를 입력하세요."
echo " (Sync v3 동기화도 같은 서버/키를 사용 — /v3/* 엔드포인트)"
