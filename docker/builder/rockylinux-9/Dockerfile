# Build environment for the x86_64-unknown-linux-gnu-rocky bundle the rocky
# image carries. Its glibc 2.34 is rocky 9's, so the bundle runs on the image
# whatever host or runner built it. `scripts/build.sh bundle-in` builds this
# with mise.toml and rust-toolchain.toml as its only context, then runs
# `scripts/build.sh bundle` in it; tool versions come from mise.toml.
FROM rockylinux:9

RUN dnf install -y --setopt=install_weak_deps=False gcc gcc-c++ make perl \
    git tar gzip xz pkgconf-pkg-config openssl-devel libcurl-devel lld libatomic \
  && dnf clean all

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
