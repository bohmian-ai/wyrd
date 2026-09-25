#!/usr/bin/env bash
# Run cargo tests for a named package family.
# Usage: bash scripts/run-family-tests.sh <wyrd|skald|vala|shared>
# WYRD_TEST_PACKAGES, when non-empty, is the space-separated affected-package
# set CI selected; the family then tests only its members in that set. CI sets
# it empty for the full gate, which tests every member.
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
  if [[ -z "${WYRD_TEST_PACKAGES:-}" || " $WYRD_TEST_PACKAGES " == *" $pkg "* ]]; then
    cargo_args+=("-p" "$pkg")
  fi
done

# A selected family always owns at least one affected package; an empty
# selection is a routing error, never a passing run.
if (( ${#cargo_args[@]} == 0 )); then
  echo "no $family package is in WYRD_TEST_PACKAGES: ${WYRD_TEST_PACKAGES:-}" >&2
  exit 1
fi

echo "testing $family packages:${cargo_args[*]/#-p/}"
exec cargo nextest run --locked "${cargo_args[@]}"
