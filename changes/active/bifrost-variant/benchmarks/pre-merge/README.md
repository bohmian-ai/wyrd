# Pre-merge benchmark reports

Pre-merge parity baseline for `wyrd/bifrost-variant/TASK-003` at `cfe21dc90`.
Host: AMD Ryzen 9 9950X, 32 CPUs, 91 GiB, Linux. Compare only against runs on
the same host.

| File | Command | Date | Code |
|---|---|---|---|
| `read-standard-hot.md` | `mise run bench:bifrost:query-capacity` (hot tier) | 2026-10-11 10:37 | `cfe21dc90`, clean tree |
| `read-standard-parquet.md` | same invocation (Parquet tier) | 2026-10-11 10:45 | `cfe21dc90`, clean tree |
| `read-telemetry-hot.md` | `mise run bench:bifrost:query-capacity -- --fixture telemetry` (hot tier) | 2026-10-11 10:48 | `cfe21dc90`, clean tree |
| `read-telemetry-parquet.md` | same invocation (Parquet tier) | 2026-10-11 10:52 | `cfe21dc90`, clean tree |
| `write-logs-run-37.md` | `mise run bench:bifrost:write-capacity -- --fixture logs` | 2026-10-11 09:28 | `cfe21dc90` |

Notes:
- **Every read row passes.** The telemetry invocation exits 1 only on its
  `export spans` row: 4 OTLP exporters at about 60k spans/s, against the 200k
  floor. That is a known write-side result (runs 27/28), and the write-capacity
  sweep at x64 is where OTLP write throughput is judged.
- **Write sweeps without a baseline yet:** telemetry and native. Produce
  baselines for them at `cfe21dc90` before comparing writes.
- **Older read runs** (runs 10–28, including the run 24 hot and run 25/26
  Parquet numbers) are in `tasks/TASK-003-revision-query-latency.md` on
  `wyrd/bifrost-variant/TASK-003`.
- **Profiles** for these runs (perf, off-CPU, latency Paretos, server logs,
  samples) are preserved in the `wyrd-bifrost-variant-task-003` worktree at
  `target/bifrost-query-capacity-premerge-cfe21dc90/{standard,telemetry}/{hot,parquet}/`.
  Git ignores that folder, and later bench runs do not overwrite it.
