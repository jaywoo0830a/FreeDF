#!/usr/bin/env bash
# FreeDF — 로컬 PostgreSQL 서버 시작 (Docker, Mac/Linux/Windows WSL2 공통)
# 사용법: ./scripts/up.sh
# 시작 후 앱은 기본값 postgres://freedf:freedf@localhost:5432/freedf 로 연결됩니다.
set -euo pipefail

# WSL2에서는 Docker Desktop의 docker.exe가 PATH에 없는 경우가 많아 둘 다 확인.
if command -v docker >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "docker를 찾을 수 없습니다. Docker Desktop(WSL2 통합 포함)을 먼저 실행하세요." >&2
    exit 1
fi

# 스크립트 위치와 무관하게 프로젝트 루트에서 compose 실행.
cd "$(dirname "$0")/.."

"$DOCKER" compose up -d db

echo "PostgreSQL 준비 대기 중..."
for _ in $(seq 1 30); do
    if "$DOCKER" compose exec -T db pg_isready -U freedf -d freedf >/dev/null 2>&1; then
        echo "PostgreSQL 준비 완료 → postgres://freedf:freedf@localhost:5432/freedf"
        exit 0
    fi
    sleep 1
done

echo "PostgreSQL 시작 시간 초과. 로그 확인: docker compose logs db" >&2
exit 1
