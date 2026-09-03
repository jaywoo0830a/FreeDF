#!/usr/bin/env bash
# FreeDF DB — .env 생성 (PostgreSQL 18.6)
# 사용법:
#   ./init.sh                              # .env 없으면 생성 (비밀번호 자동 생성)
#   FREEDF_DB_HOST=1.2.3.4 ./init.sh       # 연결 문자열에 VPS IP/도메인 사용
#   FREEDF_DB_BIND=0.0.0.0 ./init.sh       # 원격(FreeDF 앱) 접속 허용
#   POSTGRES_PASSWORD=xxx ./init.sh        # 특정 값으로 강제
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
    # 포트 바인딩 — 기본 127.0.0.1(보안).
    # FreeDF 앱이 VPS 밖에서 직접 접속하면 0.0.0.0으로 변경하세요.
    FREEDF_DB_BIND="${FREEDF_DB_BIND:-127.0.0.1}"
    # 연결 문자열 표시용 호스트:
    #   - DB와 같은 호스트(미디어 백엔드, SSH 세션) → localhost
    #   - FreeDF 앱(Windows)에서 직접 연결 → VPS IP 또는 도메인
    FREEDF_DB_HOST="${FREEDF_DB_HOST:-localhost}"
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
# 포트 바인딩 — 원격(FreeDF 앱) 접속 시 0.0.0.0
FREEDF_DB_BIND=${FREEDF_DB_BIND}
FREEDF_DB_PORT=${FREEDF_DB_PORT}
# 연결 문자열 표시용 호스트 (VPS IP/도메인 — 접속 주소에만 쓰임)
FREEDF_DB_HOST=${FREEDF_DB_HOST}
EOF
    echo ".env 생성 완료"
fi

echo "──────────────────────────────────────────────"
echo " DB 바인딩: ${FREEDF_DB_BIND:-127.0.0.1}:${FREEDF_DB_PORT:-5432}"
echo " 앱 연결 문자열 (FREEDF_DATABASE_URL):"
echo "   postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${FREEDF_DB_HOST:-localhost}:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
if [[ "${FREEDF_DB_BIND:-127.0.0.1}" != "127.0.0.1" ]]; then
    echo " ⚠ 포트가 외부에 노출됩니다 — 방화벽으로 앱 IP만 허용 권장:"
    echo "   sudo ufw allow from <앱IP> to any port ${FREEDF_DB_PORT:-5432} proto tcp"
fi
echo "──────────────────────────────────────────────"
