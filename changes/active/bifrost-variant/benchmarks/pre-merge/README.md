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
| `write-telemetry.md` | `mise run bench:bifrost:write-capacity -- --fixture telemetry` | 2026-10-11 11:15 | `cfe21dc90`, clean tree |
| `write-native.md` | `mise run bench:bifrost:write-capacity` | 2026-10-11 11:16 | `cfe21dc90`, clean tree |
| `client-queue.txt`, `client-queue.json` | `mise run bench:bifrost:client-queue` | 2026-10-11 11:32 | `cfe21dc90`, clean tree |

Notes:
- **Every read row passes.** The telemetry invocation exits 1 only on its
  `export spans` row: 4 OTLP exporters at about 60k spans/s, against the 200k
  floor. That is a known write-side result (runs 27/28), and the write-capacity
  sweep at x64 is where OTLP write throughput is judged.
- **Write results:**
  - **Logs: PASS.** 260,475 rows/s at x64, 42.6 MiB/s OTLP wire per core.
  - **Spans: FAIL on wire bytes per core.**
    - Peak at x64 is 425,901 rows/s, which is over the 200k floor, but only
      16.5 MiB/s OTLP wire per core, under the 30 MiB/s target.
    - Spans are small: about 340 B of wire per row, against 1,253 B for logs.
      Per-row cost is similar, at about 51k rows/s per core for spans and
      36k for logs.
    - The task exits 1 on this row.
  - **Native: PASS.** 3.31M rows/s at x64. p99 is 667 ms at x16 and 717 ms
    at x64.
  - **Client queue: PASS** on every step, with `max_in_flight` 1 as the
    smallest that passes.
- **Older read runs** (runs 10–28, including the run 24 hot and run 25/26
  Parquet numbers) are in `tasks/TASK-003-revision-query-latency.md` on
  `wyrd/bifrost-variant/TASK-003`.
- **Profiles** for these runs (perf, off-CPU, latency Paretos, server logs,
  samples) are preserved in the `wyrd-bifrost-variant-task-003` worktree at
  `target/bifrost-query-capacity-premerge-cfe21dc90/`: reads in `{standard,telemetry}/{hot,parquet}/`, writes in `logs-writes/`, `telemetry-writes/` and `writes/`, and the client queue in `client-queue/`. The logs write profiles from run-37 are in `logs-writes/`.
  Git ignores that folder, and later bench runs do not overwrite it.
