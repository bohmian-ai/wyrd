#!/usr/bin/env bash
# Aggregates required CI job results into one stable required check: a lane the
# change did not select is skipped and passes; any failed or cancelled lane fails.
set -euo pipefail

if (( $# == 0 )); then
  echo "usage: $0 <job-result>..." >&2
  exit 2
fi

for result in "$@"; do
  case "$result" in
    success|skipped) ;;
    *) echo "CI job failed or was cancelled: ${result:-<empty>}"; exit 1 ;;
  esac
done
echo "all required jobs succeeded or were skipped"
