#!/usr/bin/env bash
#
# ideation 테스트 샌드박스 진입점.
# Node 24.21.0 + Vitest 5.0.0 컨테이너 안에서 ideation/tests 의 테스트를 실행한다.
#
# 사용 예:
#   ./test.sh                        # 전체 테스트
#   ./test.sh tests/sum.test.js      # 특정 파일만
#   ./test.sh --watch                # vitest 에 인자 그대로 전달 (watch 등)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

IMAGE_NAME="freedf-ideation"

# 이미지 빌드 (레이어 캐시 때문에 이미 빌드된 경우 거의 즉시 끝남)
# node_modules 는 이미지 레이어 안에만 존재하므로 호스트는 전혀 오염되지 않는다.
docker build -t "$IMAGE_NAME" .

# TTY 가 붙어 있으면 대화형 플래그 사용 (watch 모드 등에서 필요)
TTY_FLAGS=()
if [ -t 0 ]; then TTY_FLAGS+=(-it); elif [ -t 1 ]; then TTY_FLAGS+=(-t); fi

# 테스트 코드와 설정만 이미지의 /app 으로 bind mount 하므로
# 호스트에서 파일을 수정하면 즉시 컨테이너에 반영된다.
# (호스트에 없는 경로를 마운트하지 않으므로 Docker 부산물 디렉터리도 생기지 않음)
exec docker run --rm "${TTY_FLAGS[@]}" \
  --user "$(id -u):$(id -g)" \
  --env HOME=/tmp \
  --volume "$SCRIPT_DIR/tests":/app/tests \
  --volume "$SCRIPT_DIR/vitest.config.js":/app/vitest.config.js \
  --workdir /app \
  "$IMAGE_NAME" \
  npx vitest run "$@"
