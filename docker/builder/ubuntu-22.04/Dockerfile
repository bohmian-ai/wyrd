# Build environment for the gnu Linux bundles: the debian images and native
# `wyrd server install` on x86_64 and aarch64. Its glibc 2.35 is the oldest
# those bundles support, so a bundle built here runs on Ubuntu 22.04, Debian 12,
# and newer whatever host or runner built it. `scripts/build.sh bundle-in`
# builds this with mise.toml and rust-toolchain.toml as its only context, then
# runs `scripts/build.sh bundle` in it; tool versions come from mise.toml.
FROM ubuntu:22.04

RUN apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends --yes \
    build-essential pkg-config libssl-dev libcurl4-openssl-dev lld \
    git curl ca-certificates perl xz-utils \
  && rm -rf /var/lib/apt/lists/*

# Toolchains live outside any home directory and are world-writable, because
# the build runs as the invoking host user so its outputs stay theirs.
ARG MISE_VERSION
ENV MISE_DATA_DIR=/opt/mise MISE_CACHE_DIR=/opt/mise/cache \
    RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo \
    PATH=/opt/mise/shims:/opt/cargo/bin:$PATH
RUN curl -fsSL https://mise.run | MISE_VERSION="v${MISE_VERSION}" MISE_INSTALL_PATH=/usr/local/bin/mise sh
COPY mise.toml rust-toolchain.toml /opt/wyrd/
RUN cd /opt/wyrd && mise trust && mise install rust node npm:pnpm cmake protoc \
  && chmod -R a+rwX /opt/mise /opt/rustup /opt/cargo
