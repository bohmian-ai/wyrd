#!/usr/bin/env bash
# WHY THIS FILE EXISTS: the HTTP error response shape — status code, problem-
# JSON body, error code — is a public contract that clients and operators rely
# on. Multiple IntoResponse impls for WyrdError (or WyrdErrorResponse) cause
# ambiguity: different call sites may select different impls depending on
# import order, producing inconsistent HTTP bodies for the same error. The
# single authorized impl lives in wyrd-server/src/http/error.rs; all server
# error paths must route through it.
#
# WHAT IT CHECKS: exactly one impl IntoResponse for a Wyrd error type exists
# across all crates, and it is the one in wyrd-server/src/http/error.rs.
set -eu

matches=$(rg -n 'impl IntoResponse for (WyrdErrorResponse|.*Wyrd.*Error)' crates --glob '*.rs' || true)
count=$(printf '%s\n' "$matches" | sed '/^$/d' | wc -l | tr -d ' ')

if [ "$count" != "1" ]; then
  echo "expected exactly one Wyrd error IntoResponse mapper, found $count"
  printf '%s\n' "$matches"
  exit 1
fi

case "$matches" in
  crates/wyrd/wyrd-server/src/http/error.rs:*'impl IntoResponse for WyrdErrorResponse'*)
    ;;
  *)
    echo "Wyrd error IntoResponse mapper must stay in wyrd-server/src/http/error.rs"
    printf '%s\n' "$matches"
    exit 1
    ;;
esac
