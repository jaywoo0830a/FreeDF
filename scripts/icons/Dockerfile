# FreeDF 아이콘 생성기 — 격리된 파이썬 컨테이너.
#
# 빌드:  docker build -t freedf-icongen -f scripts/icons/Dockerfile scripts/icons
# 실행:  docker run --rm -v "$(pwd)":/workspace freedf-icongen
#        (저장소 루트를 /workspace 로 마운트 → design_icon.py 가
#         crates/freedf/assets/icon/ 아래에 PNG/*.ico 를 생성)
#
# 로컬 파이썬/Pillow 환경을 오염시키지 않고, 의존성을 이미지 안에 고정해
# 어느 머신에서나 동일한 아티팩트를 뽑기 위한 목적.
FROM python:3.12-slim

# Pillow만 설치 — 아이콘 생성에 필요한 유일한 런타임 의존성.
RUN python -m pip install --no-cache-dir --disable-pip-version-check Pillow==11.2.* \
    && rm -rf /root/.cache

WORKDIR /workspace

# 호스트 저장소 루트가 /workspace 로 마운트됨을 가정.
CMD ["python", "/workspace/scripts/design_icon.py"]