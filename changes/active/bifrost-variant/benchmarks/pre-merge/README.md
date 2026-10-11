# Pre-merge benchmark reports

These are the latest reports from the `wyrd-bifrost-variant-task-003`
worktree, copied from `target/bifrost-query-capacity/`. Git ignores that
directory and every run overwrites it.

All reports come from the same host: AMD Ryzen 9 9950X, 32 CPUs, 91 GiB,
Linux. **Only the logs write sweep was run at `cfe21dc90`, the pre-merge
head.** The read reports predate the time-column change, the one-fsync group
commit and fixes 1/3/4. They are historical reference, not the parity
baseline.

| File | Run | Date | Code |
|---|---|---|---|
| `read-standard-hot.md` | run 24 | 10-10 15:51 | `1190c539a` plus per-column bytes recorded at append (committed as `05f2ba96b`) |
| `read-standard-parquet.md` | run 26 | 10-10 16:09 | same, but run with the 12 MiB chunk **experiment**. The default stayed 24 MiB (run 25), so its 1m-aggregate, broad-window, table-aggregate and full-scan rows do not reflect the default. |
| `read-telemetry-hot.md` | after run 28 | 10-10 22:27 | between `05f2ba96b` and `179bfdc11`, exact state unrecorded |
| `read-telemetry-parquet.md` | after run 28 | 10-11 07:18 | `179bfdc11` plus uncommitted work that later landed in `cfe21dc90`, exact state unrecorded |
| `write-logs-run-37.md` | run-37 | 10-11 09:28 | `cfe21dc90` |

The per-run history and its analysis are in
`tasks/TASK-003-revision-query-latency.md` on `wyrd/bifrost-variant/TASK-003`
(runs 10–28).

**The parity baseline still has to be produced.** Run
`mise run bench:bifrost:query-capacity` (hot, then Parquet) at `cfe21dc90` in
the TASK-003 worktree, then the write sweeps. Add those reports here before
comparing the merged branch.
