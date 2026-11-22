# writes-parquet benchmark ledger

Every `bench:l1:bifrost:writes` record produced during this change, oldest
first, read with `capacity report` (medians of each step's repetitions). Records
sit under `target/benchmarks/bifrost/writes-parquet/` in the worktree that ran
them, or `benchmarks/history/` for full unselected runs. "steps" counts measured
steps; 12 is a full run. "in MiB/cpu-s" is `input_mib_per_cpu_s`; the throughput
mandate is 80 on otlp-logs-c64.

Reading rules learned from this ledger:

- One commit's runs vary more than the changes measured so far (`193b5ea52`:
  992–1,514 spans-c64 requests/s, 18.7–21.3 input MiB/cpu-s). A single run per
  side cannot decide a 10% change; `capacity compare` intervals cover the
  repetitions inside each run, not variation between runs.
- otlp-logs-c64 reads 55–62 input MiB/cpu-s alone or in a full run but 27–40 in
  the three-step `otlp-spans-c64,otlp-logs-c64,native-write-c64` subset, where it
  follows spans-c64. Judge the mandate only alone or in a full run.
- A change counts only when throughput and input MiB/cpu-s both improve.

| record (time-commit) | verdict | steps | spans-c64 req/s | spans-c64 in MiB/cpu-s | logs-c64 req/s | logs-c64 in MiB/cpu-s | native-c64 req/s | native-c64 µs/row | errors |
|---|---|---|---|---|---|---|---|---|---|
| 20261121T180126Z-31af0fe55 | VALID | 1 |  |  | 1136.861 | 58.21 |  |  | 0 |
| 20261121T235858Z-27ec04c40 | VALID | 12 | 614.793 | 13.192 | 611.427 | 55.644 | 105.763 | 2.619 | 0 |
| 20261122T001549Z-27ec04c40 | VALID | 1 |  |  | 631.175 | 62.002 |  |  | 0 |
| 20261122T003824Z-5a057524f | VALID | 1 |  |  | 1009.736 | 59.909 |  |  | 0 |
| 20261122T003915Z-27ec04c40 | VALID | 1 |  |  | 1000.378 | 59.714 |  |  | 0 |
| 20261122T004708Z-27ec04c40 | VALID | 2 |  |  | 654.765 | 54.509 |  |  | 0 |
| 20261122T010737Z-27ec04c40 | VALID | 1 |  |  | 1006.026 | 60.743 |  |  | 0 |
| 20261122T010854Z-6db7d71af | VALID | 2 |  |  | 774.044 | 51.88 |  |  | 0 |
| 20261122T011912Z-6db7d71af | INVALID | 2 |  |  | 622.283 | 76.972 |  |  | 0 |
| 20261122T012501Z-6db7d71af | VALID | 2 |  |  | 753.198 | 60.06 |  |  | 0 |
| 20261122T024344Z-193b5ea52 | INVALID | 12 | 696.255 | 16.058 | 798.515 | 61.922 | 129.781 | — | native-write-c64= 64 total (median 0) ; |
| 20261122T030957Z-193b5ea52 | VALID | 3 | 992.272 | 20.906 | 615.29 | 29.903 | 102.98 | 4.403 | 0 |
| 20261122T032347Z-fc32c2b99 | VALID | 3 | 1226.215 | 19.976 | 604.872 | 28.677 | 101.375 | 4.485 | 0 |
| 20261122T033722Z-27ec04c40 | VALID | 3 | 986.07 | 19.224 | 488.607 | 26.542 | 71.341 | 5.043 | 0 |
| 20261122T034552Z-193b5ea52 | VALID | 3 | 1184.79 | 18.736 | 533.83 | 29.267 | 83.937 | 4.849 | 0 |
| 20261122T034956Z-193b5ea52 | VALID | 3 | 1222.655 | 21.261 | 564.574 | 30.792 | 84.26 | 4.204 | 0 |
| 20261122T043916Z-193b5ea52 | INVALID | 3 |  |  |  |  |  |  | 0 |
| 20261122T043950Z-193b5ea52 | VALID | 3 | 1513.985 | 19.097 | 838.314 | 32.38 | 105.509 | 4.513 | 0 |
| 20261122T044326Z-16047deec | VALID | 3 | 1085.478 | 21.186 | 585.505 | 28.311 | 73.97 | 5.026 | 0 |
| 20261122T044549Z-16047deec | VALID | 3 | 1184.437 | 20.162 | 599.847 | 29.887 | 77.274 | 4.577 | 0 |
| 20261122T051556Z-a08a0feb3 | INVALID | 3 | 1248.637 | 18.816 | 760.386 | 40.077 | 88.978 | 4.626 | 0 |
| 20261122T115951Z-cb464d5ba | VALID | 3 | 1481.428 | 18.273 | 882.955 | 36.069 | 132.941 | 5.088 | 0 |

## A-B-B-A check of the WAL commits (9d1451691, cb464d5ba)

Three-step runs, A = `16047deec`, B = `68521c22d` (code identical to
`cb464d5ba`), run in order A1 B1 B2 A2 on a quiet host, all VALID.

| run | record | spans-c64 req/s | spans-c64 in MiB/cpu-s | spans-c64 µs/row | logs-c64 req/s | logs-c64 in MiB/cpu-s |
|---|---|---|---|---|---|---|
| A1 | 20261122T123302Z-16047deec | 1594.699 | 20.474 | 15.767 | 756.353 | 29.992 |
| B1 | 20261122T123453Z-68521c22d | 1320.159 | 21.368 | 15.108 | 712.492 | 33.357 |
| B2 | 20261122T123647Z-68521c22d | 1442.245 | 20.965 | 15.400 | 738.417 | 28.333 |
| A2 | 20261122T123839Z-16047deec | 1143.082 | 21.012 | 15.361 | 623.445 | 28.678 |

Result: no CPU-efficiency regression. spans-c64 input MiB/cpu-s moved +4.4%
(A1→B1) and −0.2% (A2→B2), against 2.6% (A) and 1.9% (B) between runs of the same
build. The earlier −9% to −14% came from comparing single noisy runs.
Throughput is not decided: the same build varied 28% (A1→A2).
