#!/usr/bin/env bash
# Run cargo tests for a named package family.
# Usage: bash scripts/run-family-tests.sh <wyrd|skald|vala|shared>
# A family is every workspace package under its directories, so a new crate
# joins its family's lane without any list to update. .github/scripts/select-ci.py
# uses the same prefixes.
# WYRD_TEST_PACKAGES, when non-empty, is the space-separated affected-package
# set CI selected; the family then tests only its members in that set. CI sets
# it empty for the full gate, which tests every member.
set -euo pipefail

family="${1:?family name required: wyrd | skald | vala | shared}"

case "$family" in
  wyrd)   dirs='crates/wyrd/|crates/wyrd-spec/' ;;
  skald)  dirs='crates/skald/' ;;
  vala)   dirs='crates/vala/' ;;
  shared) dirs='crates/shared/|sdks/wyrd-sdk-rust/' ;;
  *) echo "unknown family: $family" >&2; exit 1 ;;
esac

# A read loop rather than mapfile, which macOS's bash 3.2 lacks.
packages=()
while IFS= read -r pkg; do packages+=("$pkg"); done < <(cargo metadata --locked --no-deps --format-version 1 |
  jq -r --arg re "^($dirs)" \
    '.workspace_root as $root | .packages[]
     | select(.manifest_path | ltrimstr($root + "/") | test($re)) | .name')

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
