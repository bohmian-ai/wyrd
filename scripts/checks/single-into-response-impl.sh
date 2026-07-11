#!/usr/bin/env bash
# Assert Wyrd HTTP errors flow through one server IntoResponse mapper.
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
