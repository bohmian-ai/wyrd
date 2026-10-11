# Bifrost query capacity: 10000000 rows, parquet otlp spans rows per step, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
write pareto x1: 19.3 MB/s, 42.0 MB/s per server core, 22760 rows/s, client mean 22.53 ms per batch
  OTLP wire: 7.3 MiB/s, 16.0 MiB/s per server core, 338 B per row
     0.000 ms   0.0%  Gate.write_request, 3921 profiles
     3.674 ms  16.3%  Scribe.group_sync
     1.169 ms   5.2%  Scribe.group_fence
     0.992 ms   4.4%  Scribe.preprocess
     0.431 ms   1.9%  Scribe.decode
     0.154 ms   0.7%  Scribe.group_write
     0.091 ms   0.4%  Scribe.shard_wait
     0.086 ms   0.4%  Scribe.admission
     0.060 ms   0.3%  Scribe.group_insert
     0.027 ms   0.1%  Scribe.group_commit
  1.00 fsyncs and 2.05 pool acquires per batch
cpu pareto, write x1: 10.29 ms server CPU per batch (target/bifrost-query-capacity/telemetry-writes/parquet/profiles/00-otlp spans-1)
     1.988 ms  19.3%  wyrd_server
     0.875 ms   8.5%  parquet_variant
     0.705 ms   6.8%  kernel
     0.661 ms   6.4%  c:ZSTD
     0.542 ms   5.3%  core
     0.499 ms   4.8%  parquet
     0.469 ms   4.6%  alloc
     0.419 ms   4.1%  sha2
     0.364 ms   3.5%  vala_bifrost_redux
     0.247 ms   2.4%  indexmap
     0.234 ms   2.3%  c:mi
     0.177 ms   1.7%  c:0x000000000019851e
     0.165 ms   1.6%  wyrd_queue
     0.156 ms   1.5%  arrow_data
     0.140 ms   1.4%  c:
     0.140 ms   1.4%  hashbrown
     0.135 ms   1.3%  parquet_variant_compute
     0.124 ms   1.2%  arrow_select
     0.124 ms   1.2%  std
     0.116 ms   1.1%  c:HUF
| otlp spans x1 | 1 | 22720 rows/s | 13.4/42.8/45.0 | 0 | 0.46 cores | 1.52 GiB | 0.06 cores | - | 2000000 rows in 88.0s, 19.3 MB/s, 42.0 MB/s per server core; 17 files, avg 4.7 MiB; OTLP wire 7.3 MiB/s, 16.0 MiB/s and 49554 rows/s per server core |
write pareto x4: 50.6 MB/s, 44.3 MB/s per server core, 59826 rows/s, client mean 34.26 ms per batch
  OTLP wire: 19.3 MiB/s, 16.9 MiB/s per server core, 338 B per row
     0.000 ms   0.0%  Gate.write_request, 3913 profiles
     3.747 ms  10.9%  Scribe.group_sync
     2.789 ms   8.1%  Scribe.shard_wait
     2.372 ms   6.9%  Scribe.group_fence
     0.952 ms   2.8%  Scribe.preprocess
     0.426 ms   1.2%  Scribe.decode
     0.242 ms   0.7%  Scribe.group_write
     0.135 ms   0.4%  Scribe.group_insert
     0.084 ms   0.2%  Scribe.admission
     0.052 ms   0.2%  Scribe.group_commit
  0.65 fsyncs and 1.21 pool acquires per batch
cpu pareto, write x4: 9.78 ms server CPU per batch (target/bifrost-query-capacity/telemetry-writes/parquet/profiles/01-otlp spans-4)
     2.234 ms  22.8%  wyrd_server
     0.717 ms   7.3%  parquet_variant
     0.592 ms   6.0%  kernel
     0.551 ms   5.6%  c:ZSTD
     0.524 ms   5.4%  core
     0.514 ms   5.2%  alloc
     0.415 ms   4.2%  vala_bifrost_redux
     0.395 ms   4.0%  sha2
     0.350 ms   3.6%  parquet
     0.263 ms   2.7%  c:mi
     0.259 ms   2.6%  indexmap
     0.181 ms   1.9%  wyrd_queue
     0.170 ms   1.7%  hashbrown
     0.158 ms   1.6%  arrow_data
     0.157 ms   1.6%  c:
     0.135 ms   1.4%  c:0x000000000019851e
     0.113 ms   1.2%  c:HUF
     0.111 ms   1.1%  c:FSE
     0.108 ms   1.1%  arrow_select
     0.108 ms   1.1%  std
| otlp spans x4 | 4 | 59741 rows/s | 41.9/56.4/60.9 | 0 | 1.14 cores | 1.69 GiB | 0.19 cores | - | 2000000 rows in 33.5s, 50.6 MB/s, 44.3 MB/s per server core; 25 files, avg 4.7 MiB; OTLP wire 19.3 MiB/s, 16.9 MiB/s and 52247 rows/s per server core |
write pareto x16: 162.2 MB/s, 41.0 MB/s per server core, 191484 rows/s, client mean 42.80 ms per batch
  OTLP wire: 61.8 MiB/s, 15.6 MiB/s per server core, 338 B per row
     0.000 ms   0.0%  Gate.write_request, 3910 profiles
     4.955 ms  11.6%  Scribe.shard_wait
     4.160 ms   9.7%  Scribe.group_fence
     4.007 ms   9.4%  Scribe.group_sync
     1.003 ms   2.3%  Scribe.preprocess
     0.733 ms   1.7%  Scribe.group_write
     0.443 ms   1.0%  Scribe.decode
     0.370 ms   0.9%  Scribe.group_insert
     0.139 ms   0.3%  Scribe.group_commit
     0.090 ms   0.2%  Scribe.admission
  0.21 fsyncs and 0.57 pool acquires per batch
cpu pareto, write x16: 10.58 ms server CPU per batch (target/bifrost-query-capacity/telemetry-writes/parquet/profiles/02-otlp spans-16)
     2.424 ms  22.9%  wyrd_server
     0.824 ms   7.8%  parquet_variant
     0.608 ms   5.7%  kernel
     0.593 ms   5.6%  c:ZSTD
     0.589 ms   5.6%  core
     0.560 ms   5.3%  alloc
     0.480 ms   4.5%  vala_bifrost_redux
     0.420 ms   4.0%  parquet
     0.398 ms   3.8%  sha2
     0.288 ms   2.7%  indexmap
     0.287 ms   2.7%  c:mi
     0.190 ms   1.8%  arrow_data
     0.190 ms   1.8%  hashbrown
     0.190 ms   1.8%  wyrd_queue
     0.180 ms   1.7%  c:
     0.148 ms   1.4%  c:0x000000000019851e
     0.143 ms   1.4%  std
     0.124 ms   1.2%  arrow_array
     0.121 ms   1.1%  arrow_select
     0.114 ms   1.1%  c:HUF
| otlp spans x16 | 16 | 191337 rows/s | 41.5/70.0/80.4 | 0 | 3.96 cores | 2.07 GiB | 0.69 cores | - | 2000000 rows in 10.5s, 162.2 MB/s, 41.0 MB/s per server core; 41 files, avg 4.8 MiB; OTLP wire 61.8 MiB/s, 15.6 MiB/s and 48344 rows/s per server core |
write pareto x64: 360.8 MB/s, 43.4 MB/s per server core, 426010 rows/s, client mean 76.47 ms per batch
  OTLP wire: 137.5 MiB/s, 16.5 MiB/s per server core, 338 B per row
     0.000 ms   0.0%  Gate.write_request, 3908 profiles
    13.151 ms  17.2%  Scribe.shard_wait
    10.630 ms  13.9%  Scribe.group_fence
     6.994 ms   9.1%  Scribe.group_sync
     2.559 ms   3.3%  Scribe.group_write
     1.287 ms   1.7%  Scribe.group_insert
     1.050 ms   1.4%  Scribe.preprocess
     0.466 ms   0.6%  Scribe.decode
     0.431 ms   0.6%  Scribe.group_commit
     0.099 ms   0.1%  Scribe.admission
  0.06 fsyncs and 0.35 pool acquires per batch
cpu pareto, write x64: 10.00 ms server CPU per batch (target/bifrost-query-capacity/telemetry-writes/parquet/profiles/03-otlp spans-64)
     2.476 ms  24.8%  wyrd_server
     0.686 ms   6.9%  parquet_variant
     0.584 ms   5.8%  alloc
     0.570 ms   5.7%  core
     0.552 ms   5.5%  kernel
     0.549 ms   5.5%  c:ZSTD
     0.515 ms   5.1%  vala_bifrost_redux
     0.372 ms   3.7%  sha2
     0.297 ms   3.0%  c:mi
     0.286 ms   2.9%  indexmap
     0.261 ms   2.6%  parquet
     0.220 ms   2.2%  wyrd_queue
     0.208 ms   2.1%  arrow_data
     0.194 ms   1.9%  hashbrown
     0.193 ms   1.9%  c:
     0.122 ms   1.2%  std
     0.114 ms   1.1%  arrow_array
     0.113 ms   1.1%  c:0x000000000019851e
     0.105 ms   1.1%  arrow_select
     0.104 ms   1.0%  c:HUF
| otlp spans x64 | 64 | 425901 rows/s | 69.2/131.4/182.6 | 0 | 8.32 cores | 2.37 GiB | 1.62 cores | - | 2000000 rows in 4.7s, 360.8 MB/s, 43.4 MB/s per server core; 41 files, avg 4.8 MiB; OTLP wire 137.5 MiB/s, 16.5 MiB/s and 51177 rows/s per server core |
| otlp spans target | - | - | - | - | - | - | - | FAIL | peak x64: 425901 rows/s, 16.5 MiB/s OTLP wire per server core; < 30 MiB/s per core |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.7s |
