#!/usr/bin/env bash
# FreeDF 미디어 백엔드 — .env 생성
# 사용법: ./init.sh
set -euo pipefail
cd "$(dirname "$0")"

if [[ -f .env ]]; then
    echo ".env 이미 존재합니다 — 수정하려면 직접 편집 후 ./up.sh"
    source .env
else
    # API 키: 지정 없으면 랜덤 생성 (클라이언트 server.json에 같은 값 입력)
    if [[ -z "${FREEDF_API_KEY:-}" ]]; then
        FREEDF_API_KEY="$(openssl rand -hex 16 2>/dev/null \
            || od -An -N16 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n' \
            || echo "dev-key-$(date +%s)")"
    fi
    cat > .env <<EOF
# server/backend 설정 — ./init.sh가 생성 (자유롭게 수정 가능)
# FreeDF 메인 DB 연결 (server/db/.env 와 일치)
DATABASE_URL=${DATABASE_URL:-postgres://freedf:CHANGEME@localhost:5432/freedf}
# 파일 저장 경로 (nginx가 같은 경로를 서빙)
MEDIA_DIR=${MEDIA_DIR:-/srv/freedf-server/media}
# 클라이언트에 알려줄 공개 URL (응답의 url 필드)
PUBLIC_BASE_URL=${PUBLIC_BASE_URL:-https://media.example.com}
# API 키 — FreeDF 앱의 server.json에 같은 값
FREEDF_API_KEY=${FREEDF_API_KEY}
# 백엔드 바인딩 (nginx가 127.0.0.1:8080으로 프록시)
BIND=${BIND:-127.0.0.1:8080}
EOF
    echo ".env 생성 완료"
fi

echo "──────────────────────────────────────────────"
echo " DATABASE_URL = ${DATABASE_URL}"
echo " MEDIA_DIR    = ${MEDIA_DIR}"
echo " PUBLIC_BASE_URL = ${PUBLIC_BASE_URL}"
echo " FREEDF_API_KEY  = ${FREEDF_API_KEY}"
echo "──────────────────────────────────────────────"
echo " FreeDF 앱의 Server 설정(server.json)에 PUBLIC_BASE_URL과 API 키를 입력하세요."
