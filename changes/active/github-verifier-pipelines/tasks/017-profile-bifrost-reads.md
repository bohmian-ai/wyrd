---
id: TASK-017
status: proposed
approved_spec_revision: 16
depends_on: []
---

# Profile Bifrost memtable and Parquet reads

Locate the added CPU work in hot long-tail reads and the sources of the Parquet
tail. Use the integrated Bifrost server, existing L1 query fixtures, and saved
`bifrost/hot/20261117T200646Z-cfa3bee12.json` and
`bifrost/parquet/20261117T201036Z-cfa3bee12.json` records. Findings support
[revision 16 qualification](../spec-revisions/16-shared-workload-management.md#acceptance).

```sh
mise run bench:l1:bifrost:hot -- --steps span-long-tail-memtable-c1,span-shredded-memtable-c1 --profile cpu
mise run bench:l1:bifrost:parquet -- --steps point-c64,span-trace-c64,span-shredded-c1 --profile cpu
```

1. **Hot memtable.** Long-tail CPU/query rose from 100 to 140 ms while the
   shredded case improved. Use these two queries on the current fixture to
   identify costs of residual Variant decoding, logical row reconstruction,
   filter masks, and workspace reservation. Report time to first batch and
   CPU/query. Preserve the staged scan gains in any proposed fix.
2. **Parquet.** Point c64 reached 12.22 ms server p99 against a 5 ms target,
   and span trace c64 reached 27.59 ms. Scanned rows stayed constant and bytes
   changed little. Inspect Oracle planning, DataFusion task polling and decode,
   and the Gate response path. Report scan work, CPU/query, first batch, server
   p99, and client p99 so the proposed fix addresses the measured cost.

Record the revision, commands, profile paths, query correctness, and the
smallest supported next change or the remaining attribution gap. Keep the
existing OTLP span write-floor shortfall outside this read investigation.
A production fix has its own unprofiled qualification under REQ-024.
