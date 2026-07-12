# syntax=docker/dockerfile:1
ARG RUST_VERSION=1.87.0

FROM rust:${RUST_VERSION}-slim AS builder
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    set -eux; \
    cargo build --release --locked -p wyrd-server --features cloud; \
    cp target/release/wyrd-server /build/wyrd-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/wyrd-server /usr/local/bin/wyrd-server
ENV WYRD_SERVER_PORT=8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wyrd-server"]
