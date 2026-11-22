#!/usr/bin/env bash
# Run one L1 benchmark category through its `benchmark:l1:<category>` task.
#
# `--list-cases` lists the category's exact case selectors and their routine
# or targeted status from the driver's registry, preparing no benchmark build
# and starting no case. Anything else prepares the release server, driver
# and, for Verification, the SDK modules once, before any case clock starts,
# then runs the sweep: the routine cases, or exactly `--cases a,b` in that
# order, each with its own fresh PostgreSQL, server, storage and clients.
set -Eeuo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <verification|gateway|bifrost> [--list-cases | --cases a,b] [--profile cpu|heap|both] [--allow-dirty]" >&2
  exit 64
fi
readonly category=$1
shift
readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

for argument in "$@"; do
  if [[ $argument == --list-cases ]]; then
    if [[ -x target/profiling/release/capacity ]]; then
      exec target/profiling/release/capacity l1 "$category" "$@"
    fi
    exec cargo run --locked --quiet -p wyrd-testing --bin capacity -- l1 "$category" "$@"
  fi
done

mise run bench:build
if [[ $category == verification ]]; then
  mise run bench:build:sdks
fi
exec target/profiling/release/capacity l1 "$category" "$@"
