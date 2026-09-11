#!/usr/bin/env bash
# Run every Bifrost test tier against the Postgres lifecycle owned by the caller.
set -uo pipefail

lanes=(
  unit:rust
  unit:python
  unit:typescript
  integration:redux
  integration:sql
  integration:server
  journey
  journey:python
  journey:typescript
)

declare -a failed=()

for lane in "${lanes[@]}"; do
  printf '\n=== bifrost: %s ===\n' "$lane"
  task="test:bifrost:${lane}:inner"
  if mise run "$task"; then
    printf '=== bifrost: %s PASSED ===\n' "$lane"
  else
    status=$?
    printf '=== bifrost: %s FAILED (exit %d) ===\n' "$lane" "$status"
    failed+=("$lane")
  fi
done

printf '\n=== bifrost summary: %d/%d lanes passed ===\n' \
  "$(( ${#lanes[@]} - ${#failed[@]} ))" "${#lanes[@]}"

if [[ ${#failed[@]} -gt 0 ]]; then
  printf 'failed lanes: %s\n' "${failed[*]}"
  exit 1
fi
