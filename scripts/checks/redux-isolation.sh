#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "Redux isolation checker requires rg; refusing to run without it" >&2
  exit 1
fi

forbidden=(
  'vala_bifrost::'
  'vala-bifrost[[:space:]]*='
  'vala-bifrost[[:space:]]*\.[[:space:]]*workspace'
  'vala-bifrost[[:space:]]*=[[:space:]]*\{[^}]*workspace[[:space:]]*=[[:space:]]*true'
)
redux_paths=(crates/vala/vala-bifrost-redux/Cargo.toml crates/vala/vala-bifrost-redux/src)

for pattern in "${forbidden[@]}"; do
  if match="$(rg -n -e "$pattern" "${redux_paths[@]}" 2>/dev/null)"; then
    printf 'Redux isolation violation detected: %s\n%s\n' "$pattern" "$match" >&2
    exit 1
  fi
done

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT
for index in "${!forbidden[@]}"; do
  fixture="$fixture_dir/forbidden-${index}.rs"
  case "$index" in
    0) printf '%s\n' 'use vala_bifrost::catalog::WyrdCatalog;' > "$fixture" ;;
    1) printf '%s\n' 'vala-bifrost   = { workspace = true }' > "$fixture" ;;
    2) printf '%s\n' 'vala-bifrost . workspace = true' > "$fixture" ;;
    3) printf '%s\n' 'vala-bifrost={workspace=true}' > "$fixture" ;;
  esac
  if ! rg -n -e "${forbidden[$index]}" "$fixture" >/dev/null; then
    echo "Redux isolation checker negative self-test failed for ${forbidden[$index]}" >&2
    exit 1
  fi
done

echo "Redux isolation passed"
