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
COPY --from=build /app/server/backend/target/release/freedf-media-server /usr/local/bin/freedf-media-server
CMD ["freedf-media-server"]
