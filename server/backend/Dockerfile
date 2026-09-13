# 컨텍스트 = 저장소 루트 (server/docker-compose.yml 의 build.context 참조).
# rust:1.98 — 로컬 개발 툴체인과 동일 (edition2024 의존성 요구 충족).
# Cargo.lock 포함 복사 → 재현 가능한 빌드.
FROM rust:1.98-slim AS build
WORKDIR /app
# freedf-sync(경로 의존) + 백엔드 소스. **/target 은 .dockerignore로 제외.
COPY crates/freedf-sync crates/freedf-sync
COPY server/backend server/backend
RUN cargo build --release --manifest-path server/backend/Cargo.toml

FROM debian:bookworm-slim
# curl: compose healthcheck(/health) 및 간단한 디버깅용 (런타임에 필수 아님).
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/server/backend/target/release/freedf-media-server /usr/local/bin/freedf-media-server
# 1Gbps 대용량(multipart/스냅샷) 전송 공간 예약 + 바인딩은 .env(BIND)로 제어.
EXPOSE 8080
CMD ["freedf-media-server"]
