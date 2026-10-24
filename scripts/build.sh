#!/usr/bin/env bash
# Builds the Wyrd server bundle and the official images. Release CI, the
# startup lanes, kind, and `mise run docker:build` all build through here; tool
# versions come from mise.toml.
#
#   scripts/build.sh bundle
#       Builds this host's bundle into release/: the wyrd-server binary,
#       which also runs as the Monty worker it spawns for Verifier code, and
#       the production UI in release/ui (the adapter-node BFF build, its
#       package.json, and production node_modules). No Node executable is
#       bundled: native installs use the system Node, images their own.
#       WYRD_PROFILE names the Cargo profile (default release; published
#       bundles use dist).
#
#   scripts/build.sh bundle-in <ubuntu-22.04|rockylinux-9>
#       On Linux: builds the same bundle inside docker/builder/<env>/Dockerfile,
#       whose glibc is the oldest the bundle supports, so the result never
#       depends on the host's glibc. ubuntu-22.04 builds the gnu bundles (the
#       debian images and native installs), rockylinux-9 the rocky one. The
#       build runs as the caller, with its own target/<env> directory and the
#       caller's Cargo download cache.
#
#   scripts/build.sh image <debian|rocky> <tag> [docker build args...]
#       On Linux: builds the image's bundle with bundle-in, then
#       docker/official/<distro>/Dockerfile from it, and prints the image ID.
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

usage() {
  echo "usage: $0 bundle | bundle-in <ubuntu-22.04|rockylinux-9> | image <debian|rocky> <tag> [docker build args...]" >&2
  exit 2
}

bundle() {
  local profile="${WYRD_PROFILE:-release}" target="${CARGO_TARGET_DIR:-$root/target}"
  local profile_dir="$profile" out="$root/release" ui=crates/wyrd/wyrd-server/wyrd-ui
  [[ $profile == dev ]] && profile_dir=debug

  cargo build --locked -p wyrd-server --profile "$profile" --features cloud
  pnpm --dir "$ui" install --frozen-lockfile
  pnpm --dir "$ui" build

  rm -rf "$out"
  mkdir -p "$out/ui"
  cp "$target/$profile_dir/wyrd-server" "$out/"
  cp -R "$ui/build" "$ui/package.json" "$ui/pnpm-lock.yaml" "$out/ui/"
  # adapter-node leaves `dependencies` external; install only those.
  pnpm --dir "$out/ui" install --prod --frozen-lockfile --ignore-scripts
  rm "$out/ui/pnpm-lock.yaml"

  "$out/wyrd-server" --version
  # The BFF's server code and every runtime dependency load without a listener.
  (cd "$out/ui" && node --input-type=module -e 'await import("./build/handler.js")')
}

bundle_in() {
  local env="${1:-}" target="${CARGO_TARGET_DIR:-$root/target}" cargo="${CARGO_HOME:-$HOME/.cargo}"
  [[ -n $env && -f docker/builder/$env/Dockerfile ]] || usage
  if [[ "$(uname -s)" != Linux ]]; then
    # The build installs the UI's Linux node_modules into this checkout.
    echo "a Linux bundle builds on Linux (CI's image and release jobs)" >&2
    exit 1
  fi
  local builder="wyrd-build:$env" mounts=(-v "$root:$root") rustflags="${RUSTFLAGS:-}"
  [[ $target == "$root"/* ]] || mounts+=(-v "$target:$target")
  # wyrd-server's text outgrows aarch64's +/-128 MiB direct-call range, and GNU
  # ld fails with `relocation truncated to fit: R_AARCH64_CALL26` instead of
  # placing veneers; lld (in every builder) inserts range-extension thunks.
  [[ "$(uname -m)" == aarch64 ]] && rustflags+="${rustflags:+ }-C link-arg=-fuse-ld=lld"
  mkdir -p "$target/$env/home" "$cargo/registry" "$cargo/git"

  # The tool pins are the whole context, so the toolchain layers rebuild
  # exactly when mise.toml or rust-toolchain.toml change.
  tar -c mise.toml rust-toolchain.toml "docker/builder/$env/Dockerfile" \
    | docker build -q -f "docker/builder/$env/Dockerfile" \
      --build-arg "MISE_VERSION=$(mise --version | cut -d' ' -f1)" -t "$builder" - >/dev/null
  docker run --rm --user "$(id -u):$(id -g)" "${mounts[@]}" \
    -v "$cargo/registry:/opt/cargo/registry" -v "$cargo/git:/opt/cargo/git" \
    -w "$root" -e HOME="$target/$env/home" -e CARGO_TARGET_DIR="$target/$env" \
    -e MISE_TRUSTED_CONFIG_PATHS="$root" -e WYRD_PROFILE -e RUSTFLAGS="$rustflags" \
    "$builder" scripts/build.sh bundle
}

image() {
  local distro="${1:-}" tag="${2:-}" env
  [[ -n $tag && -f docker/official/$distro/Dockerfile ]] || usage
  shift 2
  case "$distro" in
    debian) env=ubuntu-22.04 ;;
    rocky) env=rockylinux-9 ;;
  esac
  bundle_in "$env" >&2

  local revision
  revision="$(git rev-parse HEAD)"
  [[ -z "$(git status --porcelain --untracked-files=no)" ]] || revision+="+dirty"
  # Only the image ID reaches stdout.
  docker build -q -f "docker/official/$distro/Dockerfile" \
    --label "org.opencontainers.image.revision=$revision" -t "$tag" "$@" .
}

case "${1:-}" in
  bundle) bundle ;;
  bundle-in) bundle_in "${2:-}" ;;
  image) shift; image "$@" ;;
  *) usage ;;
esac
