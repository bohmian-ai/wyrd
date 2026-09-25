#!/usr/bin/env bash
# Lists the files a change touches and hands them to select-ci.py, which
# classifies them into verification lanes and release packages.
set -euo pipefail

base="${1:-${WYRD_CHANGE_BASE:-}}"
head="${2:-${WYRD_CHANGE_HEAD:-}}"

# WYRD_CHANGED_FILES names a prepared file list so the self-tests can classify
# fixed scenarios without constructing commits.
if [[ -n "${WYRD_CHANGED_FILES:-}" ]]; then
  changed_files="$WYRD_CHANGED_FILES"
else
  changed_files="${RUNNER_TEMP:-/tmp}/wyrd-changed-files"
  if [[ -z "$base" || -z "$head" ]]; then
    echo "base and head SHAs are required" >&2
    echo "usage: $0 <base-sha> <head-sha>" >&2
    echo "or set WYRD_CHANGE_BASE and WYRD_CHANGE_HEAD" >&2
    exit 2
  fi
  if [[ "$base" =~ ^0+$ ]]; then
    git ls-tree -r --name-only "$head" > "$changed_files"
  else
    # --no-renames lists both sides of a rename so the old path's owner is
    # selected too.
    git diff --no-renames --name-only "$base" "$head" > "$changed_files"
  fi
fi

exec python3 "$(dirname "${BASH_SOURCE[0]}")/select-ci.py" "$changed_files"
