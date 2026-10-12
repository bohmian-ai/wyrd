# Bifrost query capacity: 10000000 rows, parquet otlp logs rows per step, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
write pareto x1: 32.2 MB/s, 93.9 MB/s per server core, 11690 rows/s, client mean 43.87 ms per batch
  OTLP wire: 13.9 MiB/s, 40.6 MiB/s per server core, 1253 B per row
     0.000 ms   0.0%  Gate.write_request, 3941 profiles
     4.000 ms   9.1%  Scribe.group_sync
     2.448 ms   5.6%  Scribe.preprocess
     1.488 ms   3.4%  Scribe.group_fence
     0.427 ms   1.0%  Scribe.decode
     0.314 ms   0.7%  Scribe.group_write
     0.100 ms   0.2%  Scribe.shard_wait
     0.085 ms   0.2%  Scribe.admission
     0.061 ms   0.1%  Scribe.group_insert
     0.032 ms   0.1%  Scribe.group_commit
  1.00 fsyncs and 2.10 pool acquires per batch
cpu pareto, write x1: 14.93 ms server CPU per batch (target/bifrost-query-capacity/logs-writes/parquet/profiles/00-otlp logs-1)
     3.624 ms  24.3%  c:ZSTD
     1.687 ms  11.3%  sha2
     1.333 ms   8.9%  wyrd_server
     1.209 ms   8.1%  kernel
     0.649 ms   4.3%  parquet_variant
     0.461 ms   3.1%  core
     0.382 ms   2.6%  parquet
     0.361 ms   2.4%  alloc
     0.258 ms   1.7%  vala_bifrost_redux
     0.237 ms   1.6%  c:mi
     0.224 ms   1.5%  c:HIST
     0.197 ms   1.3%  arrow_select
     0.170 ms   1.1%  c:
     0.149 ms   1.0%  c:FSE
     0.148 ms   1.0%  c:0x00000000001a17ff
     0.145 ms   1.0%  c:HUF
     0.143 ms   1.0%  c:0x000000000019851e
     0.136 ms   0.9%  wyrd_queue
     0.130 ms   0.9%  indexmap
     0.128 ms   0.9%  c:0x00000000001a1852
| otlp logs x1 | 1 | 11668 rows/s | 44.0/45.5/47.0 | 0 | 0.34 cores | 2.06 GiB | 0.04 cores | - | 2000000 rows in 171.4s, 32.2 MB/s, 93.9 MB/s per server core; 24 files, avg 21.4 MiB; OTLP wire 13.9 MiB/s, 40.6 MiB/s and 33996 rows/s per server core |
write pareto x4: 115.1 MB/s, 105.3 MB/s per server core, 41744 rows/s, client mean 49.11 ms per batch
  OTLP wire: 49.8 MiB/s, 45.5 MiB/s per server core, 1253 B per row
     0.000 ms   0.0%  Gate.write_request, 3915 profiles
     4.288 ms   8.7%  Scribe.group_sync
     2.764 ms   5.6%  Scribe.shard_wait
     2.303 ms   4.7%  Scribe.preprocess
     1.813 ms   3.7%  Scribe.group_fence
     0.432 ms   0.9%  Scribe.group_write
     0.405 ms   0.8%  Scribe.decode
     0.095 ms   0.2%  Scribe.group_insert
     0.078 ms   0.2%  Scribe.admission
     0.061 ms   0.1%  Scribe.group_commit
  0.65 fsyncs and 1.25 pool acquires per batch
cpu pareto, write x4: 13.40 ms server CPU per batch (target/bifrost-query-capacity/logs-writes/parquet/profiles/01-otlp logs-4)
     3.027 ms  22.6%  c:ZSTD
     1.468 ms  10.9%  sha2
     1.436 ms  10.7%  wyrd_server
     1.121 ms   8.4%  kernel
     0.582 ms   4.3%  parquet_variant
     0.440 ms   3.3%  core
     0.398 ms   3.0%  alloc
     0.296 ms   2.2%  parquet
     0.284 ms   2.1%  vala_bifrost_redux
     0.253 ms   1.9%  c:mi
     0.204 ms   1.5%  c:HIST
     0.201 ms   1.5%  c:
     0.188 ms   1.4%  arrow_select
     0.158 ms   1.2%  c:0x00000000001a1852
     0.146 ms   1.1%  indexmap
     0.139 ms   1.0%  wyrd_queue
     0.121 ms   0.9%  c:0x000000000019851e
     0.121 ms   0.9%  c:0x00000000001a17ff
     0.119 ms   0.9%  c:FSE
     0.114 ms   0.9%  std
| otlp logs x4 | 4 | 41673 rows/s | 50.0/59.9/66.9 | 0 | 1.09 cores | 2.44 GiB | 0.14 cores | - | 2000000 rows in 48.0s, 115.1 MB/s, 105.3 MB/s per server core; 40 files, avg 22.3 MiB; OTLP wire 49.8 MiB/s, 45.5 MiB/s and 38110 rows/s per server core |
write pareto x16: 380.3 MB/s, 108.3 MB/s per server core, 137808 rows/s, client mean 59.30 ms per batch
  OTLP wire: 164.5 MiB/s, 46.9 MiB/s per server core, 1253 B per row
     0.000 ms   0.0%  Gate.write_request, 3910 profiles
     8.406 ms  14.2%  Scribe.shard_wait
     7.541 ms  12.7%  Scribe.group_sync
     4.074 ms   6.9%  Scribe.group_fence
     2.384 ms   4.0%  Scribe.preprocess
     1.579 ms   2.7%  Scribe.group_write
     0.438 ms   0.7%  Scribe.decode
     0.343 ms   0.6%  Scribe.group_insert
     0.156 ms   0.3%  Scribe.group_commit
     0.077 ms   0.1%  Scribe.admission
  0.23 fsyncs and 0.59 pool acquires per batch
cpu pareto, write x16: 13.04 ms server CPU per batch (target/bifrost-query-capacity/logs-writes/parquet/profiles/02-otlp logs-16)
     2.686 ms  20.6%  c:ZSTD
     1.573 ms  12.1%  wyrd_server
     1.406 ms  10.8%  sha2
     1.028 ms   7.9%  kernel
     0.552 ms   4.2%  parquet_variant
     0.502 ms   3.8%  alloc
     0.476 ms   3.6%  core
     0.335 ms   2.6%  vala_bifrost_redux
     0.310 ms   2.4%  c:mi
     0.240 ms   1.8%  c:
     0.236 ms   1.8%  parquet
     0.184 ms   1.4%  c:0x00000000001a1852
     0.183 ms   1.4%  c:HIST
     0.172 ms   1.3%  arrow_select
     0.162 ms   1.2%  wyrd_queue
     0.147 ms   1.1%  indexmap
     0.137 ms   1.1%  arrow_array
     0.133 ms   1.0%  c:0x00000000001a17ff
     0.120 ms   0.9%  c:HUF
     0.117 ms   0.9%  hashbrown
| otlp logs x16 | 16 | 137713 rows/s | 59.9/91.4/167.0 | 0 | 3.51 cores | 2.71 GiB | 0.52 cores | - | 2000000 rows in 14.5s, 380.3 MB/s, 108.3 MB/s per server core; 50 files, avg 22.8 MiB; OTLP wire 164.5 MiB/s, 46.9 MiB/s and 39223 rows/s per server core |
write pareto x64: 719.2 MB/s, 98.5 MB/s per server core, 260475 rows/s, client mean 125.64 ms per batch
  OTLP wire: 311.2 MiB/s, 42.6 MiB/s per server core, 1253 B per row
     0.000 ms   0.0%  Gate.write_request, 3907 profiles
    26.155 ms  20.8%  Scribe.shard_wait
    19.575 ms  15.6%  Scribe.group_sync
    14.149 ms  11.3%  Scribe.group_fence
     6.323 ms   5.0%  Scribe.group_write
     2.668 ms   2.1%  Scribe.preprocess
     1.175 ms   0.9%  Scribe.group_insert
     0.587 ms   0.5%  Scribe.group_commit
     0.541 ms   0.4%  Scribe.decode
     0.092 ms   0.1%  Scribe.admission
  0.06 fsyncs and 0.33 pool acquires per batch
cpu pareto, write x64: 14.35 ms server CPU per batch (target/bifrost-query-capacity/logs-writes/parquet/profiles/03-otlp logs-64)
     2.635 ms  18.4%  c:ZSTD
     1.856 ms  12.9%  wyrd_server
     1.418 ms   9.9%  sha2
     1.233 ms   8.6%  kernel
     0.601 ms   4.2%  parquet_variant
     0.587 ms   4.1%  core
     0.551 ms   3.8%  alloc
     0.453 ms   3.2%  vala_bifrost_redux
     0.377 ms   2.6%  c:mi
     0.281 ms   2.0%  c:
     0.215 ms   1.5%  c:0x00000000001a1852
     0.208 ms   1.5%  parquet
     0.189 ms   1.3%  arrow_array
     0.189 ms   1.3%  wyrd_queue
     0.184 ms   1.3%  c:HIST
     0.171 ms   1.2%  std
     0.165 ms   1.1%  indexmap
     0.162 ms   1.1%  arrow_select
     0.146 ms   1.0%  c:0x00000000001a17ff
     0.129 ms   0.9%  arrow_data
| otlp logs x64 | 64 | 260475 rows/s | 112.8/246.6/303.4 | 0 | 7.30 cores | 3.61 GiB | 1.14 cores | - | 2000000 rows in 7.7s, 719.2 MB/s, 98.5 MB/s per server core; 55 files, avg 23.1 MiB; OTLP wire 311.2 MiB/s, 42.6 MiB/s and 35670 rows/s per server core |
| otlp logs target | - | - | - | - | - | - | - | PASS | peak x64: 260475 rows/s, 42.6 MiB/s OTLP wire per server core |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.7s |
