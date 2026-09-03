# 로컬 개발 툴체인과 같은 1.98 — 의존성(edition2024) 요구 충족.
# Cargo.lock을 함께 복사해 빌드 결과를 재현 가능하게 고정합니다.
FROM rust:1.98-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/freedf-media-server /usr/local/bin/freedf-media-server
CMD ["freedf-media-server"]
