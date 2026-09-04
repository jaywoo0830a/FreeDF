#!/usr/bin/env bash
# FreeDF DB — PostgreSQL 18.6 기동 + 마이그레이션 자동 적용
#
# 사용법:
#   ./up.sh              # compose 모드 (postgresql.conf SSD 튜닝 포함)
#   ./up.sh --no-conf    # 단순 모드 — 설정 파일 마운트 없이 docker run
#                        # (Docker Desktop WSL 통합이 꺼져 있을 때 유용)
set -euo pipefail
cd "$(dirname "$0")"
source ../_docker.sh

[[ -f .env ]] || ./init.sh
set -a; source .env; set +a

DOCKER="$(require_docker)"

MODE=compose
[[ "${1:-}" == "--no-conf" ]] && MODE=noconf

if [[ "$MODE" == compose ]]; then
    echo "PostgreSQL 컨테이너 시작 (compose)..."
    set +e
    out="$("$DOCKER" compose up -d db 2>&1)"
    rc=$?
    set -e
    if [[ $rc -ne 0 ]]; then
        echo "$out" >&2
        if is_distro_mount_error "$out"; then
            echo >&2
            echo "⚠ Docker Desktop이 이 WSL 배포판의 파일을 마운트할 수 없습니다." >&2
            echo "  1) Docker Desktop → Settings → Resources → WSL integration 에서" >&2
            echo "     이 배포판을 활성화한 뒤 다시 ./up.sh" >&2
            echo "  2) 또는 설정 파일 마운트 없이 실행: ./up.sh --no-conf" >&2
        fi
        exit 1
    fi
    PSQL=("$DOCKER" compose exec -T db psql -U "$POSTGRES_USER" -d "$POSTGRES_DB")
    READY() { "$DOCKER" compose exec -T db pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; }
else
    if ! "$DOCKER" inspect freedf-db >/dev/null 2>&1; then
        echo "PostgreSQL 컨테이너 생성 (단순 모드 — postgresql.conf 없음)..."
        "$DOCKER" run -d --name freedf-db \
            -e POSTGRES_USER="$POSTGRES_USER" \
            -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
            -e POSTGRES_DB="$POSTGRES_DB" \
            -p "${FREEDF_DB_BIND:-127.0.0.1}:${FREEDF_DB_PORT:-5432}:5432" \
            -v freedf_pgdata:/var/lib/postgresql \
            --restart unless-stopped \
            "${POSTGRES_IMAGE:-postgres:18.6-alpine}"
    else
        echo "기존 컨테이너(freedf-db) 시작..."
        # 이미 실행 중이면 start는 no-op(실패 허용).
        "$DOCKER" start freedf-db >/dev/null 2>&1 || true
    fi
    PSQL=("$DOCKER" exec -i freedf-db psql -U "$POSTGRES_USER" -d "$POSTGRES_DB")
    READY() { "$DOCKER" exec -i freedf-db pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; }
fi

echo "PostgreSQL 준비 대기 중..."
for _ in $(seq 1 60); do
    READY && break
    sleep 1
done
if ! READY; then
    echo "PostgreSQL 시작 시간 초과." >&2
    echo "로그: $DOCKER compose logs db  (단순 모드: $DOCKER logs freedf-db)" >&2
    exit 1
fi

# ── 마이그레이션 (schema_migrations 기준, 순서대로 1회씩) ──
PSQL+=(-v ON_ERROR_STOP=1)
"${PSQL[@]}" -tAc "CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY)" >/dev/null

for f in migrations/*.sql; do
    ver="$(basename "$f" .sql)"
    if [[ "$("${PSQL[@]}" -tAc "SELECT 1 FROM schema_migrations WHERE version='$ver'")" == "1" ]]; then
        echo "  skip $ver (적용됨)"
        continue
    fi
    echo "  apply $ver"
    { echo "BEGIN;"; cat "$f"; echo; echo "INSERT INTO schema_migrations (version) VALUES ('$ver');"; echo "COMMIT;"; } \
        | "${PSQL[@]}" >/dev/null
done

echo "──────────────────────────────────────────────"
echo " PostgreSQL 준비 완료 — DB는 backend(server/backend)만 접속합니다."
echo " 연결 문자열: postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${FREEDF_DB_HOST:-localhost}:${FREEDF_DB_PORT:-5432}/${POSTGRES_DB}"
echo " 다음: cd ../backend && ./up.sh → https://freedf.rlawjddn00.online"
if [[ "${FREEDF_DB_BIND:-127.0.0.1}" != "127.0.0.1" ]]; then
    echo " ⚠ 원격 접속 모드 — 방화벽으로 backend 호스트만 허용 권장:"
    echo "   sudo ufw allow from <backendIP> to any port ${FREEDF_DB_PORT:-5432} proto tcp"
fi
echo "──────────────────────────────────────────────"
