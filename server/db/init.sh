#!/usr/bin/env bash
# FreeDF DB — .env 생성 (PostgreSQL 18.6)
# 사용법: ./init.sh               # .env가 없으면 생성 (비밀번호는 자동 생성)
#         POSTGRES_PASSWORD=xxx ./init.sh   # 특정 값으로 강제
# 생성 후 up.sh가 이 값을 읽습니다.
set -euo pipefail
cd "$(dirname "$0")"

if [[ -f .env ]]; then
    echo ".env 이미 존재합니다 — 수정하려면 직접 편집 후 ./up.sh"
    source .env
else
    # 기본값 (환경 변수로 재정의 가능)
    POSTGRES_USER="${POSTGRES_USER:-freedf}"
    POSTGRES_DB="${POSTGRES_DB:-freedf}"
    FREEDF_DB_PORT="${FREEDF_DB_PORT:-5432}"
    # 비밀번호: 지정 없으면 랜덤 생성 (외부 노출 VPS에서는 랜덤 권장)
    if [[ -z "${POSTGRES_PASSWORD:-}" ]]; then
        POSTGRES_PASSWORD="$(openssl rand -hex 16 2>/dev/null \
            || od -An -N16 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n' \
            || echo "freedf-dev-$(date +%s)")"
    fi
    cat > .env <<EOF
# server/db 설정 — ./init.sh가 생성 (자유롭게 수정 가능)
POSTGRES_USER=${POSTGRES_USER}
POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
POSTGRES_DB=${POSTGRES_DB}
FREEDF_DB_PORT=${FREEDF_DB_PORT}
EOF
    echo ".env 생성 완료"
fi

echo "──────────────────────────────────────────────"
echo " DB 연결 문자열 (앱에서 FREEDF_DATABASE_URL로 사용):"
echo "   postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${FREEDF_DB_PORT}/${POSTGRES_DB}"
echo "──────────────────────────────────────────────"
