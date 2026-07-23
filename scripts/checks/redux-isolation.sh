#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

forbidden='vala_bifrost::|vala-bifrost =|vala-bifrost"'
redux_paths=(crates/vala/vala-bifrost-redux/Cargo.toml crates/vala/vala-bifrost-redux/src)

if rg -n -e "$forbidden" "${redux_paths[@]}" >/dev/null; then
  echo "Redux isolation violation detected" >&2
  exit 1
fi

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT
fixture="$fixture_dir/forbidden.rs"
printf '%s\n' 'use vala_bifrost::catalog::WyrdCatalog;' > "$fixture"
if ! rg -n -e "$forbidden" "$fixture" >/dev/null; then
  echo "Redux isolation checker negative self-test failed" >&2
  exit 1
fi

echo "Redux isolation passed"
