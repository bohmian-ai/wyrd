#!/usr/bin/env bash
# Self-tests for .github/scripts/verify-required-jobs.sh: skipped lanes keep the
# required check green, and any selected lane that fails or is cancelled fails it.
set -euo pipefail

VERIFY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../verify-required-jobs.sh"
PASS=0
FAIL=0

check() {
  local label="$1"
  local want="$2"   # pass | fail
  shift 2

  local got=fail
  if bash "$VERIFY" "$@" > /dev/null; then
    got=pass
  fi
  if [ "$got" = "$want" ]; then
    echo "OK  $label"
    PASS=$((PASS+1))
  else
    echo "FAIL $label: expected $want, got $got"
    FAIL=$((FAIL+1))
  fi
}

check "all selected lanes succeed" pass success success success
check "unaffected lanes skipped" pass success skipped skipped
check "selected lane fails" fail success failure skipped
check "selected lane cancelled" fail success cancelled
check "mixed success, skip, and failure" fail skipped success failure
check "missing result" fail success ""
check "no results" fail

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
