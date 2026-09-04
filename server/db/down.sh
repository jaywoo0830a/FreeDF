#!/usr/bin/env bash
# FreeDF DB — 중지
#
# 사용법:
#   ./down.sh              # 컨테이너만 중지/제거 (데이터 볼륨 유지)
#   ./down.sh --wipe       # 데이터 볼륨까지 완전 삭제
#   ./down.sh --no-conf    # 단순 모드(./up.sh --no-conf)로 띄운 컨테이너 정리
set -euo pipefail
cd "$(dirname "$0")"
source ../_docker.sh

DOCKER="$(require_docker)"

MODE=compose
WIPE=0
for a in "$@"; do
    [[ "$a" == "--no-conf" ]] && MODE=noconf
    [[ "$a" == "--wipe" ]] && WIPE=1
done

if [[ "$MODE" == compose ]]; then
    if [[ "$WIPE" == 1 ]]; then
        echo "PostgreSQL 중지 + 데이터 볼륨 삭제..."
        "$DOCKER" compose down -v
        echo "완료 — 데이터가 완전히 삭제되었습니다."
    else
        "$DOCKER" compose down
        echo "완료 — 컨테이너만 중지했습니다 (데이터 볼륨 유지)."
        echo "데이터까지 지우려면: ./down.sh --wipe"
    fi
else
    echo "PostgreSQL 컨테이너(freedf-db) 정리..."
    "$DOCKER" stop freedf-db >/dev/null 2>&1 || true
    "$DOCKER" rm freedf-db >/dev/null 2>&1 || true
    if [[ "$WIPE" == 1 ]]; then
        "$DOCKER" volume rm freedf_pgdata >/dev/null 2>&1 || true
        echo "완료 — 컨테이너 + 데이터 볼륨이 삭제되었습니다."
    else
        echo "완료 — 컨테이너를 중지/제거했습니다 (데이터 볼륨 유지)."
    fi
fi
