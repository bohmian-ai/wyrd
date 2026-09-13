#!/usr/bin/env bash
# WHY THIS FILE EXISTS: the four family test lanes (test:wyrd/skald/vala/shared)
# are the unit of CI parallelism — each runs independently and their combined
# pass is the Rust test gate. If a new crate is added to the workspace but not
# assigned to a lane, its tests are silently skipped. If a crate is listed in
# a lane but removed from the workspace, the lane invocation fails with an
# unknown-package error that's hard to diagnose. Both are caught here before merge.
# Package lists live in scripts/test-families.sh as the single source of truth.
#
# WHAT IT CHECKS:
#   - no crate appears in more than one family lane (dedup)
#   - every workspace crate except language bindings with dedicated gates is in
#     a Rust family lane
#   - no lane references a crate absent from the workspace (stale entries)
set -eu

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$REPO_ROOT/scripts/test-families.sh"

combined=$(printf '%s\n' \
  "${FAMILY_WYRD[@]}" \
  "${FAMILY_SKALD[@]}" \
  "${FAMILY_VALA[@]}" \
  "${FAMILY_SHARED[@]}" | sort)

dupes=$(echo "$combined" | uniq -d)
if [ -n "$dupes" ]; then
  echo "FAIL: crates appear in more than one family lane:"
  echo "$dupes"
  exit 1
fi

workspace_members=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
  | python3 -c "
import json, sys
data = json.load(sys.stdin)
print('\n'.join(sorted(p['name'] for p in data['packages'])))
" | grep -Ev '^(wyrd-sdk-python|wyrd-sdk-ts|wyrd-sdk-ts-testing|wyrd-rust-examples)$')

missing=$(comm -23 <(echo "$workspace_members") <(echo "$combined"))
if [ -n "$missing" ]; then
  echo "FAIL: workspace crates with no family lane — add to scripts/test-families.sh:"
  echo "$missing"
  exit 1
fi

phantom=$(comm -23 <(echo "$combined") <(echo "$workspace_members"))
if [ -n "$phantom" ]; then
  echo "FAIL: family lane references crates absent from the workspace (stale entry in scripts/test-families.sh):"
  echo "$phantom"
  exit 1
fi

# N-API cdylibs link against symbols supplied by Node and therefore cannot be
# executed as ordinary Rust lib-test binaries. Their compile/type/unit coverage
# must remain explicit in the aggregate gate instead of being silently exempt.
gate_body=$(sed -n '/^\[tasks\."gate"\]/,/^\[tasks\./p' "$REPO_ROOT/mise.toml")
for task in ts:napi:check ts:typecheck ts:test:unit; do
  if ! printf '%s\n' "$gate_body" | grep -F "\"$task\"" >/dev/null; then
    echo "FAIL: gate is missing required N-API/TypeScript task: $task"
    exit 1
  fi
done

total=$(echo "$combined" | wc -l | tr -d ' ')
echo "OK: all $total workspace crates are assigned to exactly one family lane."
