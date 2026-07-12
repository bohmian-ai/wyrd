#!/usr/bin/env bash
# Assert the four family test lanes (test:wyrd/skald/vala/shared) form an
# exact, non-overlapping partition of the workspace. Every workspace member
# except py-wyrd and wyrd-rust-examples must appear in exactly one lane.
# Adding a crate without updating a lane fails CI.
set -eu

FAMILY_WYRD=(
  wyrd
  wyrd-auth
  wyrd-cards
  wyrd-cli
  wyrd-config
  wyrd-interfaces
  wyrd-mcp
  wyrd-server
  wyrd-sql
  wyrd-storage
  wyrd-testing
  wyrd-tonic
  wyrd-spec
)

FAMILY_SKALD=(
  skald-agent
  skald-cache
  skald-prompt
  skald-providers
  skald-runtime
  skald-spec
  skald-tool
  skald-workflow
)

FAMILY_VALA=(
  vala-bifrost
  vala-core
  vala-drift
  vala-eval
  vala-ingest
  vala-sdk
  vala-sql
)

FAMILY_SHARED=(
  wyrd-auth-check
  wyrd-auth-issue
  wyrd-auth-oidc
  wyrd-auth-verify
  wyrd-client
  wyrd-crypt
  wyrd-dev-fixtures
  wyrd-error-derive
  wyrd-observe
  wyrd-queue
  wyrd-runtime
  wyrd-semver
  wyrd-telemetry
  wyrd-test-contract-macros
  wyrd-utils
  wyrd-version
)

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
" | grep -Ev '^(py-wyrd|wyrd-rust-examples)$')

missing=$(comm -23 <(echo "$workspace_members") <(echo "$combined"))
if [ -n "$missing" ]; then
  echo "FAIL: workspace crates with no family lane — add to test:wyrd/skald/vala/shared in mise.toml:"
  echo "$missing"
  exit 1
fi

phantom=$(comm -23 <(echo "$combined") <(echo "$workspace_members"))
if [ -n "$phantom" ]; then
  echo "FAIL: family lane references crates absent from the workspace (stale entry in mise.toml):"
  echo "$phantom"
  exit 1
fi

echo "OK: all 44 workspace crates are assigned to exactly one family lane."
