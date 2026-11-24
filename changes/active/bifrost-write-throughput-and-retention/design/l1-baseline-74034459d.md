L1 verification sweep

Coverage: routine sweep, 5 of 5 routine cases
Mode: standard
Sweep: target/benchmarks/l1/verification/20261123t035605936z/sweep.json

| Case | Status | Throughput | Server CPU per unit | Server overhead p99 | Client round trip p99 | Target | Record |
|---|---|---|---|---|---|---|---|
| direct-verify-c1 | VALID | 977.5 verifications/s | 1.524 ms/verification | (0.184, 0.185] ms, n=88103 | 1.519 ms, n=88103 | none | 01-direct-verify-direct-verify-c1.json |
| direct-verify-c64 | VALID | 8253.1 verifications/s | 2.561 ms/verification | (1.026, 1.027] ms, n=744196 | (12.423, 12.431] ms, n=744196 | none | 02-direct-verify-direct-verify-c64.json |
| direct-verify-r3000 | VALID | 2995.1 verifications/s | 1.825 ms/verification | (0.242, 0.243] ms, n=270000 | (2.109, 2.111] ms, n=270000 | none | 03-direct-verify-direct-verify-r3000.json |
| direct-verify-python-r3000 | VALID | 2995.0 verifications/s | 1.718 ms/verification | (0.236, 0.236] ms, n=270000 | (2.089, 2.091] ms, n=270000 | none | 04-direct-verify-direct-verify-python-r3000.json |
| direct-verify-typescript-r3000 | VALID | 2995.3 verifications/s | 1.826 ms/verification | (0.249, 0.249] ms, n=270000 | (2.313, 2.315] ms, n=270000 | none | 05-direct-verify-direct-verify-typescript-r3000.json |

Definitions:

- **Case**: the exact selector. Its suffix is the load: `c<N>` is N closed-loop clients, each sending its next request only once the previous one is answered; `r<N>` is an open-loop schedule of N requests per second at 64 clients.
- **Status**: `VALID` only when every measured window met the validity rules, every phase fit its allowance and cleanup was confirmed. Do not interpret the numbers of an `INVALID` or `NOT RUN` row.
- **Throughput**: successful work units, in the unit shown, divided by the summed seconds of the case's three measured windows; warmup and drain are excluded. Higher is better.
- **Server CPU per unit**: CPU time (user and system) of the server process over the measured windows, divided by the same work units, in milliseconds per unit (per 1,000 rows for writes). It includes background work the server did in those windows, so it is the server's whole cost per unit, not one code path. Lower is better.
- **Latency cells**: one percentile of the case's latency, pooled over its three windows. `(a, b] ms` is the histogram bucket that holds it: above a and at most b, so read b as the value; a single value is exact. `n` is the number of samples pooled; a p99 needs at least 1,000 and a p95 at least 200, or the reading is unavailable.
- **Server overhead p99**: per direct verification, the server's time from the request's HTTP arrival until the execution finished (`verification.direct`), less the time spent inside its Verifier blocks: what Wyrd adds around the Verifier's own work. Network and client time are excluded.
- **Client round trip p99**: per direct verification, the caller's time from immediately before the SDK's public `verify` call until its decoded result returns: SDK serialization, transport, the whole server execution including the Verifier, and decoding. Compare the `rust`, `python` and `typescript` `r3000` cases here: they run at one shared rate, so this is where each SDK's cost shows.
- **Target**: `none`: earlier performance targets do not apply to L1 cases.
- **Record**: the case record beside the sweep record; `capacity report <record>` shows its full evidence.

L1 gateway sweep

Coverage: routine sweep, 5 of 5 routine cases
Mode: standard
Sweep: target/benchmarks/l1/gateway/20261123t040929621z/sweep.json

| Case | Status | Throughput | Server CPU per unit | Headline latency | Target | Record |
|---|---|---|---|---|---|---|
| buffered-1kib-capture-metadata.c1 | VALID | 7681.0 requests/s | 0.185 ms/request | added overhead p99: (0.151, 0.151] ms, n=692371 | none | 01-overhead-buffered-1kib-capture-metadata.c1.json |
| buffered-1kib-capture-metadata.c64 | VALID | 65539.8 requests/s | 0.305 ms/request | added overhead p99: (0.492, 0.492] ms, n=5908372 | none | 02-overhead-buffered-1kib-capture-metadata.c64.json |
| streamed-1kib-capture-metadata.c64 | VALID | 1542.9 requests/s | 0.974 ms/request | added overhead p99: (0.200, 0.200] ms, n=139285 | none | 03-overhead-streamed-1kib-capture-metadata.c64.json |
| buffered-64kib-capture-metadata.c64 | VALID | 30003.6 requests/s | 0.672 ms/request | added overhead p99: (1.371, 1.372] ms, n=2705308 | none | 04-overhead-buffered-64kib-capture-metadata.c64.json |
| buffered-64kib-capture-payload.c64 | VALID | 8930.3 requests/s | 1.522 ms/request | added overhead p99: (65.012, 65.044] ms, n=805384 | none | 05-overhead-buffered-64kib-capture-payload.c64.json |

Definitions:

- **Case**: the exact selector. Its suffix is the load: `c<N>` is N closed-loop clients, each sending its next request only once the previous one is answered; `r<N>` is an open-loop schedule of N requests per second at 64 clients.
- **Status**: `VALID` only when every measured window met the validity rules, every phase fit its allowance and cleanup was confirmed. Do not interpret the numbers of an `INVALID` or `NOT RUN` row.
- **Throughput**: successful work units, in the unit shown, divided by the summed seconds of the case's three measured windows; warmup and drain are excluded. Higher is better.
- **Server CPU per unit**: CPU time (user and system) of the server process over the measured windows, divided by the same work units, in milliseconds per unit (per 1,000 rows for writes). It includes background work the server did in those windows, so it is the server's whole cost per unit, not one code path. Lower is better.
- **Latency cells**: one percentile of the case's latency, pooled over its three windows. `(a, b] ms` is the histogram bucket that holds it: above a and at most b, so read b as the value; a single value is exact. `n` is the number of samples pooled; a p99 needs at least 1,000 and a p95 at least 200, or the reading is unavailable.
- **Headline latency**, *added overhead p99*: per model call, the server's time from handler entry until the answer is handed off (a buffered response, or a stream's first bytes), less the time the call waited on the model provider: what the gateway adds on top of the provider. Network and client time are excluded.
- **Target**: `none`: earlier performance targets do not apply to L1 cases.
- **Record**: the case record beside the sweep record; `capacity report <record>` shows its full evidence.

L1 bifrost sweep

Coverage: routine sweep, 18 of 18 routine cases
Mode: standard
Sweep: target/benchmarks/l1/bifrost/20261123t042226269z/sweep.json

| Case | Status | Throughput | Server CPU per unit | Headline latency | Input | Target | Record |
|---|---|---|---|---|---|---|---|
| otlp-spans-c1 | VALID | 47727.2 rows/s | 20.445 ms/1k rows | durable ACK p99: (23.953, 23.970] ms, n=8404 | 15.40 MiB/s; 15.79 MiB/CPU-s | none | 01-writes-parquet-otlp-spans-c1.json |
| otlp-spans-c64 | VALID | 266148.5 rows/s | 29.206 ms/1k rows | durable ACK p99: (422.052, 422.314] ms, n=47055 | 85.93 MiB/s; 11.05 MiB/CPU-s | none | 02-writes-parquet-otlp-spans-c64.json |
| otlp-spans-1k-c1 | VALID | 27664.8 rows/s | 37.863 ms/1k rows | durable ACK p99: (31.654, 31.670] ms, n=4873 | 27.81 MiB/s; 26.55 MiB/CPU-s | none | 03-writes-parquet-otlp-spans-1k-c1.json |
| otlp-spans-1k-c64 | VALID | 153196.4 rows/s | 59.750 ms/1k rows | durable ACK p99: (624.427, 624.951] ms, n=27162 | 154.05 MiB/s; 16.83 MiB/CPU-s | none | 04-writes-parquet-otlp-spans-1k-c64.json |
| otlp-logs-c1 | VALID | 49032.1 rows/s | 18.787 ms/1k rows | durable ACK p99: (29.213, 29.229] ms, n=8634 | 58.57 MiB/s; 63.58 MiB/CPU-s | none | 05-writes-parquet-otlp-logs-c1.json |
| otlp-logs-c64 | VALID | 220540.4 rows/s | 29.928 ms/1k rows | durable ACK p99: (831.521, 832.045] ms, n=39026 | 263.45 MiB/s; 39.91 MiB/CPU-s | none | 06-writes-parquet-otlp-logs-c64.json |
| native-write-c1 | VALID | 1021615.6 rows/s | 1.403 ms/1k rows | durable ACK p99: (67.633, 67.699] ms, n=5623 | unavailable: native input bytes are not measured | none | 07-writes-parquet-native-write-c1.json |
| native-write-c64 | VALID | 2627937.2 rows/s | 2.002 ms/1k rows | durable ACK p99: (1234.174, 1235.223] ms, n=14659 | unavailable: native input bytes are not measured | none | 08-writes-parquet-native-write-c64.json |
| point-memtable-c1 | VALID | 1045.6 queries/s | 1.066 ms/query | server query p99: (1.327, 1.328] ms, n=94234 | n/a | none | 09-hot-point-memtable-c1.json |
| point-memtable-c64 | VALID | 18777.7 queries/s | 1.428 ms/query | server query p99: (4.180, 4.182] ms, n=1692657 | n/a | none | 10-hot-point-memtable-c64.json |
| point-staged-c1 | VALID | 485.6 queries/s | 2.356 ms/query | server query p99: (3.000, 3.002] ms, n=43766 | n/a | none | 11-hot-point-staged-c1.json |
| point-c1 | VALID | 555.4 queries/s | 1.983 ms/query | server query p99: (2.572, 2.574] ms, n=50054 | n/a | none | 12-parquet-point-c1.json |
| point-c64 | VALID | 8462.9 queries/s | 3.357 ms/query | server query p99: (10.912, 10.920] ms, n=763055 | n/a | none | 13-parquet-point-c64.json |
| full-scan-c1 | VALID | 20.6 queries/s | 459.495 ms/query | server query p95: (51.610, 51.642] ms, n=1863 | n/a | none | 14-parquet-full-scan-c1.json |
| span-trace-c64 | VALID | 4044.0 queries/s | 7.085 ms/query | server query p99: (25.739, 25.756] ms, n=364742 | n/a | none | 15-parquet-span-trace-c64.json |
| ui-sql/memtable-ui-c8 | VALID | 399.9 pages/s | 68.468 ms/page | client page p99: (47.423, 47.455] ms, n=36073 | n/a | none | 16-ui-sql-memtable-ui-c8.json |
| ui-sql/staged-ui-c8 | VALID | 388.5 pages/s | 70.395 ms/page | client page p99: (40.607, 40.639] ms, n=35043 | n/a | none | 17-ui-sql-staged-ui-c8.json |
| ui-sql/published-ui-c8 | VALID | 479.6 pages/s | 54.604 ms/page | client page p99: (35.903, 35.935] ms, n=43251 | n/a | none | 18-ui-sql-published-ui-c8.json |

Derived readings:

- otlp-spans-peak: 266148.5 rows/s from `otlp-spans-c64`
- otlp-spans-1k-peak: 153196.4 rows/s from `otlp-spans-1k-c64`
- otlp-logs-peak: 220540.4 rows/s from `otlp-logs-c64`
- native-write-peak: 2627937.2 rows/s from `native-write-c64`
- point-memtable-scaling: 0.281 (Q64 / (64 × Q1)) from `point-memtable-c64` (18777.7 queries/s) and `point-memtable-c1` (1045.6 queries/s)
- point-scaling: 0.238 (Q64 / (64 × Q1)) from `point-c64` (8462.9 queries/s) and `point-c1` (555.4 queries/s)

Definitions:

- **Case**: the exact selector. Its suffix is the load: `c<N>` is N closed-loop clients, each sending its next request only once the previous one is answered; `r<N>` is an open-loop schedule of N requests per second at 64 clients.
- **Status**: `VALID` only when every measured window met the validity rules, every phase fit its allowance and cleanup was confirmed. Do not interpret the numbers of an `INVALID` or `NOT RUN` row.
- **Throughput**: successful work units, in the unit shown, divided by the summed seconds of the case's three measured windows; warmup and drain are excluded. Higher is better.
- **Server CPU per unit**: CPU time (user and system) of the server process over the measured windows, divided by the same work units, in milliseconds per unit (per 1,000 rows for writes). It includes background work the server did in those windows, so it is the server's whole cost per unit, not one code path. Lower is better.
- **Headline latency**: one percentile of the case's latency, pooled over its three windows, at the boundary named below. `(a, b] ms` is the histogram bucket that holds it: above a and at most b, so read b as the value. `n` is the number of samples pooled; a p99 needs at least 1,000 and a p95 at least 200, or the reading is unavailable.
  - *durable ACK p99* (writes): per write request, the server's time from its gRPC handler entry until the rows are durably acknowledged (Gate's acknowledgement for native writes, the export response for OTLP): when the rows are safe to call stored. Network and client time are excluded.
  - *server query p99* (reads): per query, the server's time from the request's arrival at its outermost HTTP middleware until the query's final result frame is handed to the response body: authentication, planning, execution and encoding. Network transfer and client decoding are excluded.
  - *server query p95*: the same boundary at p95, for `full-scan`, whose windows hold too few queries for a p99.
  - *client page p99* (UI SQL): per UI page (traces, verification, logs or metrics), the driver's time from sending the page's first statement until its last answer arrived; independent statements run concurrently, then those that use their results. A page is several queries, and the network and client are included, so it is larger than one query's latency.
- **Input**: OTLP writes only: encoded OTLP bytes accepted per second, and per server CPU-second. Native writes do not measure input bytes; reads have none.
- **Target**: `none`: earlier performance targets do not apply to L1 cases.
- **Record**: the case record beside the sweep record; `capacity report <record>` shows its full evidence.
- **`<path>-peak`**: the higher throughput of a write path's `c1` and `c64` cases.
- **`<lookup>-scaling`**: `Q64 / (64 × Q1)`, the `c64` case's throughput over 64 times the `c1` case's: 1.0 means 64 clients answer 64 times what one client does; lower means contention. A derived reading is shown only when the sweep ran both of its cases.
