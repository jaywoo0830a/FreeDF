FROM rust:1.80-slim AS build
WORKDIR /app
COPY Cargo.toml ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/target/release/freedf-media-server /usr/local/bin/freedf-media-server
CMD ["freedf-media-server"]
