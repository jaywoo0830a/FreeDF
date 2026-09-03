#!/usr/bin/env bash
# FreeDF DB — 시작 (PostgreSQL 18.6) + 마이그레이션 자동 적용
# 사용법: ./up.sh
set -euo pipefail
cd "$(dirname "$0")"

[[ -f .env ]] || ./init.sh
set -a; source .env; set +a

if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다. Docker Desktop(WSL2 통합 포함)을 먼저 실행하세요." >&2
    exit 1
fi

"$DOCKER" compose up -d db

echo "PostgreSQL 준비 대기 중..."
for _ in $(seq 1 60); do
    if "$DOCKER" compose exec -T db pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done
if ! "$DOCKER" compose exec -T db pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
    echo "PostgreSQL 시작 시간 초과. 로그 확인: docker compose logs db" >&2
    exit 1
fi

# ── 마이그레이션 (schema_migrations 기준, 순서대로 1회씩) ──
PSQL=("$DOCKER" compose exec -T db psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1)
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
echo " PostgreSQL 준비 완료 — 연결 문자열:"
echo "   postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${FREEDF_DB_PORT}/${POSTGRES_DB}"
echo " 앱 실행 시:"
echo "   FREEDF_DATABASE_URL=postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${FREEDF_DB_PORT}/${POSTGRES_DB} freedf"
echo "──────────────────────────────────────────────"
