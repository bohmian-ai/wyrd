#!/usr/bin/env bash
# Self-tests for scripts/checks/test-coverage.sh logic.
# Tests the array-validation logic (dedup, missing, phantom) without requiring
# a real cargo metadata invocation.
set -euo pipefail

PASS=0
FAIL=0

check_exit() {
  local label="$1"
  local want="$2"   # 0 | nonzero
  shift 2
  local got=0
  "$@" >/dev/null 2>&1 || got=$?
  if [ "$want" = "0" ] && [ "$got" -eq 0 ]; then
    echo "OK  $label"
    PASS=$((PASS+1))
  elif [ "$want" != "0" ] && [ "$got" -ne 0 ]; then
    echo "OK  $label (expected failure, got exit $got)"
    PASS=$((PASS+1))
  else
    echo "FAIL $label: want exit $want, got $got"
    FAIL=$((FAIL+1))
  fi
}

_dedup_check() {
  local combined
  combined=$(printf '%s\n' "$@" | sort)
  local dupes
  dupes=$(echo "$combined" | uniq -d)
  if [ -n "$dupes" ]; then
    echo "duplicate found: $dupes" >&2
    return 1
  fi
  return 0
}

_missing_check() {
  # $1 = newline-separated workspace members (sorted)
  # $2 = newline-separated combined family list (sorted)
  local missing
  missing=$(comm -23 <(printf '%s\n' "$1") <(printf '%s\n' "$2"))
  if [ -n "$missing" ]; then
    echo "missing: $missing" >&2
    return 1
  fi
  return 0
}

_phantom_check() {
  # $1 = combined list, $2 = workspace members
  local phantom
  phantom=$(comm -23 <(printf '%s\n' "$1") <(printf '%s\n' "$2"))
  if [ -n "$phantom" ]; then
    echo "phantom: $phantom" >&2
    return 1
  fi
  return 0
}

# --- dedup ---
check_exit "no duplicates: passes"  0  _dedup_check alpha beta gamma
check_exit "duplicate entry: fails" nonzero  _dedup_check alpha beta alpha

# --- missing ---
check_exit "full coverage: passes" 0 \
  _missing_check $'alpha\nbeta' $'alpha\nbeta'
check_exit "crate missing from family: fails" nonzero \
  _missing_check $'alpha\nbeta\ngamma' $'alpha\nbeta'

# --- phantom ---
check_exit "no phantom: passes" 0 \
  _phantom_check $'alpha\nbeta' $'alpha\nbeta'
check_exit "phantom crate in family: fails" nonzero \
  _phantom_check $'alpha\nbeta\nphantom-crate' $'alpha\nbeta'

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
