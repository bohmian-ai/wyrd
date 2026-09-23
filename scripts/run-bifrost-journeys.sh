#!/usr/bin/env bash
# Run every Bifrost tier-1 capability binary against the Postgres the caller
# already stood up, and report all of them.
#
# The point of this script is that it does NOT stop at the first failing
# capability. mise aborts a multiline `run` on its first non-zero line, so
# composing the capability lanes that way meant a failure in the first binary
# hid the seven behind it — you learned about one broken capability per full
# database lifecycle. Here every capability runs, its exit code is recorded,
# and the summary at the end names all of them at once.
#
# Usage: bash scripts/run-bifrost-journeys.sh
# Exit:  0 when every capability passed, 1 when any failed.
set -uo pipefail

capabilities=(
  sdk
  observe
  drift
  forge
  scribe
  oracle
  otlp
  server
  mcp
)

declare -a failed=()

for capability in "${capabilities[@]}"; do
  printf '\n=== bifrost journey: %s ===\n' "$capability"
  if mise run "test:bifrost:journey:${capability}:inner"; then
    printf '=== bifrost journey: %s PASSED ===\n' "$capability"
  else
    status=$?
    printf '=== bifrost journey: %s FAILED (exit %d) ===\n' "$capability" "$status"
    failed+=("$capability")
  fi
done

printf '\n=== bifrost journey summary: %d/%d capabilities passed ===\n' \
  "$(( ${#capabilities[@]} - ${#failed[@]} ))" "${#capabilities[@]}"

if [[ ${#failed[@]} -gt 0 ]]; then
  printf 'failed capabilities: %s\n' "${failed[*]}"
  printf 'rerun one with: mise run test:bifrost:journey:<capability>\n'
  exit 1
fi
