#!/usr/bin/env bash
# Run one L1 benchmark category through its `benchmark:l1:<category>` task.
#
# `--list-cases` lists the category's exact case selectors and their routine
# or targeted status from the driver's registry file, building nothing and
# starting no case. Anything else prepares the release server, driver
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
    case $category in
      verification | gateway | bifrost) ;;
      *)
        echo "capacity l1: `$category` is not a category: verification, gateway or bifrost" >&2
        exit 2
        ;;
    esac
    if [[ $# -ne 1 ]]; then
      echo "capacity l1 $category: --list-cases runs nothing and takes no other flag" >&2
      exit 2
    fi
    # The driver's `Category::list` format, from the registry it compiles in:
    # an id that recurs within the category is qualified by its family.
    exec awk -F '\t' -v category="$category" '
      /^#/ || $1 != category { next }
      { family[++n] = $2; id[n] = $3; status[n] = $4; seen[$3]++ }
      $4 == "routine" { routine++ }
      END {
        printf "%s L1 cases (%d routine, %d targeted)\n", category, routine, n - routine
        for (i = 1; i <= n; i++) {
          selector = seen[id[i]] > 1 ? family[i] "/" id[i] : id[i]
          printf "%s\t%s\t%s\n", selector, family[i], status[i]
        }
      }
    ' crates/wyrd/wyrd-testing/src/bin/capacity/cases.tsv
  fi
done

mise run bench:build
if [[ $category == verification ]]; then
  mise run bench:build:sdks
fi
exec target/profiling/release/capacity l1 "$category" "$@"
