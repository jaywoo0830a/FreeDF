#!/usr/bin/env bash
# FreeDF 아이콘 재생성 — 격리된 파이썬 도커 컨테이너에서 실행.
#
#   bash scripts/generate_icons.sh
#
# - Pillow를 담은 전용 이미지(freedf-icongen)를 빌드한 뒤,
#   저장소 루트를 /workspace 로 마운트해 scripts/design_icon.py 를 실행합니다.
# - 결과물: crates/freedf/assets/icon/ 아래 PNG(16~1024) + app_icon.ico
#   (빌드 시 icon.rs/include_bytes! 가 app_icon.png 를, win/app.rc 가
#    app_icon.ico 를 임베드합니다.)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# 로컬에서 Docker 데몬에 연결 가능한 CLI 선택 (server/_docker.sh 와 동일 논리).
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    DOCKER=docker
elif command -v docker.exe >/dev/null 2>&1; then
    DOCKER=docker.exe
else
    echo "Docker 데몬에 연결할 수 없습니다 — Docker를 시작한 뒤 다시 실행하세요." >&2
    exit 1
fi

IMAGE="freedf-icongen"

echo "▸ 빌드: $IMAGE (python:3.12-slim + Pillow)"
"$DOCKER" build -t "$IMAGE" -f "$ROOT/scripts/icons/Dockerfile" "$ROOT/scripts/icons"

echo "▸ 생성 중..."
"$DOCKER" run --rm -v "$ROOT":/workspace "$IMAGE"

echo "▸ 완료 — crates/freedf/assets/icon/ 를 확인하세요."