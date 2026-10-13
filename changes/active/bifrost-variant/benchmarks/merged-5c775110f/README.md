# Merged-branch query-capacity reports

`wyrd/bifrost-variant/integrate-main` at `5c775110f`, same host as
`../pre-merge/` (AMD Ryzen 9 9950X, 32 CPUs, 91 GiB, Linux), one run each,
host otherwise idle.

| File | Command | Date | Code |
|---|---|---|---|
| `read-standard-hot.md` | `mise run bench:bifrost:query-capacity` (hot tier) | 2026-10-12 23:08 | `5c775110f`, clean tree |
| `read-standard-parquet.md` | same invocation (Parquet tier) | 2026-10-12 23:15 | `5c775110f`, clean tree |
| `read-telemetry-hot.md` | `mise run bench:bifrost:query-capacity -- --fixture telemetry` (hot tier) | 2026-10-12 23:34 | `5c775110f` + uncommitted DataFusion fork `variant_get_text` fast path, local path patch |
| `read-telemetry-parquet.md` | same invocation (Parquet tier) | 2026-10-12 23:37 | same as above |

The comparison against `../pre-merge/` is in `../../BENCHMARK-HANDOFF.md`,
"Merged-branch read results (2026-10-12)".
