#!/usr/bin/env bash
# WHY THIS FILE EXISTS: DataFusion, Arrow, and object_store are tightly
# version-coupled. If multiple versions resolve in the dep graph, a
# TableProvider registered against one Arrow schema type cannot be handed to
# a RecordBatch consumer expecting a type from a different version — the
# compiler accepts it but the runtime panics or silently drops batches.
# object_store is pinned to 0.13.* because the iceberg-rust crate bakes in
# that version's API; a minor bump changes the trait surface and breaks
# object-store integration silently at runtime, not at compile time.
#
# WHAT IT CHECKS: object_store, datafusion, arrow, and parquet each resolve
# to exactly one version across --all-features; object_store is on 0.13.*.
set -e
fail=0
for dep in object_store datafusion arrow parquet; do
  n=$(cargo tree --workspace -e=no-dev -d --all-features 2>/dev/null \
        | awk -v d="$dep" '$1==d {print $2}' | sort -u | wc -l | tr -d ' ')
  if [ "$n" -gt "1" ]; then
    echo "FAIL: $dep has multiple versions:"
    cargo tree --workspace -e=no-dev -d --all-features | awk -v d="$dep" '$1==d {print "  "$2}'
    fail=1
  fi
done
os=$(cargo tree --workspace -e=no-dev -i object_store --all-features 2>/dev/null | awk 'NR==1{print $2}')
case "$os" in v0.13.*) : ;; *) echo "FAIL: object_store $os is not 0.13.*"; fail=1 ;; esac
[ "$fail" = 0 ] && echo "OK: single object_store ($os) + single datafusion/arrow/parquet across the iceberg cone"
exit $fail
