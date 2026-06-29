#!/usr/bin/env bash
set -euo pipefail

base="${1:-${WYRD_CHANGE_BASE:-}}"
head="${2:-${WYRD_CHANGE_HEAD:-}}"
output_file="${GITHUB_OUTPUT:-/dev/stdout}"
changed_files="${RUNNER_TEMP:-/tmp}/wyrd-changed-files"

if [[ -z "$base" || -z "$head" ]]; then
  echo "base and head SHAs are required" >&2
  echo "usage: $0 <base-sha> <head-sha>" >&2
  echo "or set WYRD_CHANGE_BASE and WYRD_CHANGE_HEAD" >&2
  exit 2
fi

git diff --name-only "$base" "$head" > "$changed_files"

set_output() {
  local name="$1"
  local pattern="$2"

  if grep -Eq "$pattern" "$changed_files"; then
    echo "$name=true" >> "$output_file"
  else
    echo "$name=false" >> "$output_file"
  fi
}

set_output rust '^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|deny\.toml|mise\.toml|crates/|python/py-wyrd/Cargo\.toml|python/py-wyrd/src/)'
set_output python '^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/shared/wyrd-utils/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|python/py-wyrd/|examples/python/)'
set_output docs '^(mise\.toml|docs/|openapi\.yaml|crates/wyrd-spec/schemas/|examples/)'
# ui also covers the cross-package generated token targets so the check:tokens
# drift lock fires on a hand-edit to any of them, not just on brand/ source edits.
set_output ui '^(mise\.toml|crates/wyrd/wyrd-server/wyrd-ui/|docs/src/styles/wyrd-tokens\.css|\.claude/skills/wyrd-ui/references/wyrd-theme\.css|\.codex/skills/wyrd-ui/references/wyrd-theme\.css)'
set_output generated '^(mise\.toml|Cargo\.toml|Cargo\.lock|crates/wyrd-spec/|crates/wyrd/wyrd-cards/|crates/wyrd/wyrd-interfaces/|python/py-wyrd/)'
set_output workflow '^(\.github/workflows/|\.github/scripts/)'

if [[ -s "$changed_files" ]]; then
  echo "any=true" >> "$output_file"
else
  echo "any=false" >> "$output_file"
fi

echo "Changed files:"
sed 's/^/- /' "$changed_files"
