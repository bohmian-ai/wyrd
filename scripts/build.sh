#!/usr/bin/env bash
# Builds the Wyrd server bundle and the official images. Release CI, the
# startup lanes, kind, and `mise run docker:build` all build through here; tool
# versions come from mise.toml.
#
#   scripts/build.sh bundle
#       Builds this host's bundle into release/: the wyrd-server binary,
#       which also runs as the Monty worker it spawns for Verifier code.
#       WYRD_PROFILE names the Cargo profile (default release; published
#       bundles use dist).
#
#   scripts/build.sh image <ubuntu|debian|rocky> <tag> [docker build args...]
#       On Linux: builds this host's bundle, then
#       docker/official/<distro>/Dockerfile from it, and prints the image ID.
#       The bundle needs a glibc no newer than the image's; release builds the
#       ubuntu and debian images from an ubuntu-22.04 bundle and rocky from a
#       rockylinux:9 one.
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

usage() {
  echo "usage: $0 bundle | image <ubuntu|debian|rocky> <tag> [docker build args...]" >&2
  exit 2
}

bundle() {
  local profile="${WYRD_PROFILE:-release}" target="${CARGO_TARGET_DIR:-$root/target}"
  local profile_dir="$profile" out="$root/release"
  [[ $profile == dev ]] && profile_dir=debug

  cargo build --locked -p wyrd-server --profile "$profile" --features cloud

  rm -rf "$out"
  mkdir -p "$out"
  cp "$target/$profile_dir/wyrd-server" "$out/"
  "$out/wyrd-server" --version
}

image() {
  local distro="${1:-}" tag="${2:-}"
  [[ -n $tag && -f docker/official/$distro/Dockerfile ]] || usage
  shift 2
  if [[ "$(uname -s)" != Linux ]]; then
    echo "an image copies a Linux bundle; build it on Linux (CI's image job)" >&2
    exit 1
  fi
  bundle >&2

  local revision
  revision="$(git rev-parse HEAD)"
  [[ -z "$(git status --porcelain --untracked-files=no)" ]] || revision+="+dirty"
  # Only the image ID reaches stdout.
  docker build -q -f "docker/official/$distro/Dockerfile" \
    --label "org.opencontainers.image.revision=$revision" -t "$tag" "$@" .
}

case "${1:-}" in
  bundle) bundle ;;
  image) shift; image "$@" ;;
  *) usage ;;
esac
