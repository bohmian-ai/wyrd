#!/usr/bin/env bash
# Run cargo tests for a named package family.
# Usage: bash scripts/run-family-tests.sh <wyrd|skald|vala|shared>
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-families.sh"

family="${1:?family name required: wyrd | skald | vala | shared}"

case "$family" in
  wyrd)   packages=("${FAMILY_WYRD[@]}") ;;
  skald)  packages=("${FAMILY_SKALD[@]}") ;;
  vala)   packages=("${FAMILY_VALA[@]}") ;;
  shared) packages=("${FAMILY_SHARED[@]}") ;;
  *) echo "unknown family: $family" >&2; exit 1 ;;
esac

cargo_args=()
for pkg in "${packages[@]}"; do
  cargo_args+=("-p" "$pkg")
done

exec cargo nextest run --locked "${cargo_args[@]}" -E 'not test(pg_tests)'
