#!/usr/bin/env bash
# 공용 Docker CLI 선택 — server/* 스크립트가 공유.
#
# 1) `docker`가 데몬에 **실제로 연결**되면 우선 사용
#    (Linux Docker Engine, 또는 WSL integration이 켜진 Docker Desktop).
# 2) 아니면 `docker.exe`(Windows Docker Desktop)로 폴백.
# 3) 둘 다 실패하면 명확한 안내와 함께 종료.
#
# 사용:
#   source "$(dirname "$0")/../_docker.sh"
#   DOCKER="$(require_docker)"

detect_docker() {
    local d
    for d in docker docker.exe; do
        command -v "$d" >/dev/null 2>&1 || continue
        if command -v timeout >/dev/null 2>&1; then
            timeout 5 "$d" version --format '{{.Server.Version}}' >/dev/null 2>&1 \
                && { echo "$d"; return 0; }
        else
            "$d" version --format '{{.Server.Version}}' >/dev/null 2>&1 \
                && { echo "$d"; return 0; }
        fi
    done
    return 1
}

require_docker() {
    local d
    d="$(detect_docker || true)"
    if [[ -z "$d" ]]; then
        echo "Docker 데몬에 연결할 수 없습니다." >&2
        echo " - Linux: Docker Engine 실행 확인 → systemctl status docker" >&2
        echo " - Windows(WSL): Docker Desktop을 시작하고, 필요하면" >&2
        echo "   Settings → Resources → WSL integration 에서 이 배포판을 활성화하세요." >&2
        exit 1
    fi
    echo "$d"
}

# WSL 배포판 마운트 서비스 오류(Docker Desktop 통합 꺼짐) 여부.
# compose 출력이 이 패턴을 포함하면 바인드 마운트가 불가능한 상태입니다.
is_distro_mount_error() {
    local out="$1"
    [[ "$out" == *"distro mount service"* || "$out" == *"guest-services"* ]]
}
