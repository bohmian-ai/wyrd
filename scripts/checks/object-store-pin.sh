#!/usr/bin/env bash
# Assert single versions of object_store/datafusion/arrow/parquet across the
# iceberg cone (stage1+).
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
