# Bifrost query capacity: 10000000 rows, parquet rows per step, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
write pareto x1: 156.0 MB/s, 227.8 MB/s per server core, 554669 rows/s, client mean 29.51 ms per batch
    11.543 ms  39.1%  Gate.write_request, 614 profiles
     5.102 ms  17.3%  Scribe.group_sync
     3.885 ms  13.2%  Scribe.preprocess
     1.509 ms   5.1%  Scribe.group_fence
     0.573 ms   1.9%  Scribe.group_write
     0.181 ms   0.6%  Scribe.group_insert
     0.082 ms   0.3%  Scribe.shard_wait
     0.072 ms   0.2%  Scribe.admission
     0.026 ms   0.1%  Scribe.group_commit
     0.002 ms   0.0%  Scribe.decode
  1.01 fsyncs and 2.07 pool acquires per batch
cpu pareto, write x1: 20.11 ms server CPU per batch (target/bifrost-query-capacity/writes/parquet/profiles/00-write-1)
     5.614 ms  27.9%  c:ZSTD
     2.359 ms  11.7%  kernel
     2.333 ms  11.6%  vala_bifrost_redux
     1.784 ms   8.9%  parquet
     1.740 ms   8.7%  sha2
     0.702 ms   3.5%  arrow_row
     0.640 ms   3.2%  chrono
     0.577 ms   2.9%  c:HUF
     0.507 ms   2.5%  c:0x00000000001a17ff
     0.491 ms   2.4%  c:HIST
     0.449 ms   2.2%  arrow_select
     0.284 ms   1.4%  alloc
     0.217 ms   1.1%  core
     0.183 ms   0.9%  ahash
     0.161 ms   0.8%  arrow_array
     0.137 ms   0.7%  wyrd_spec
     0.119 ms   0.6%  hashbrown
     0.117 ms   0.6%  c:0x00000000001a1852
     0.109 ms   0.5%  c:mi
     0.105 ms   0.5%  tokio
| write x1 | 1 | 554650 rows/s | 41.9/52.4/57.0 | 0 | 0.68 cores | 1.16 GiB | 0.07 cores | PASS | 10000000 rows in 18.0s, 156.0 MB/s, 227.8 MB/s per server core; 2 files, avg 98.6 MiB |
write pareto x4: 321.9 MB/s, 224.7 MB/s per server core, 1144361 rows/s, client mean 57.21 ms per batch
    36.165 ms  63.2%  Gate.write_request, 612 profiles
    14.729 ms  25.7%  Scribe.group_sync
    12.954 ms  22.6%  Scribe.shard_wait
     4.381 ms   7.7%  Scribe.preprocess
     2.146 ms   3.8%  Scribe.group_fence
     1.346 ms   2.4%  Scribe.group_write
     0.406 ms   0.7%  Scribe.group_insert
     0.056 ms   0.1%  Scribe.group_commit
     0.013 ms   0.0%  Scribe.admission
     0.001 ms   0.0%  Scribe.decode
  0.54 fsyncs and 1.14 pool acquires per batch
cpu pareto, write x4: 20.46 ms server CPU per batch (target/bifrost-query-capacity/writes/parquet/profiles/01-write-4)
     5.908 ms  28.9%  c:ZSTD
     2.377 ms  11.6%  vala_bifrost_redux
     2.156 ms  10.5%  kernel
     2.003 ms   9.8%  parquet
     1.528 ms   7.5%  sha2
     0.710 ms   3.5%  arrow_row
     0.691 ms   3.4%  c:0x00000000001a17ff
     0.608 ms   3.0%  chrono
     0.530 ms   2.6%  c:HUF
     0.489 ms   2.4%  c:HIST
     0.471 ms   2.3%  arrow_select
     0.333 ms   1.6%  alloc
     0.245 ms   1.2%  core
     0.180 ms   0.9%  ahash
     0.145 ms   0.7%  c:mi
     0.145 ms   0.7%  arrow_array
     0.129 ms   0.6%  c:0x00000000001a1852
     0.127 ms   0.6%  c:
     0.123 ms   0.6%  wyrd_spec
     0.098 ms   0.5%  c:0x00000000001a1d7a
| write x4 | 4 | 1144329 rows/s | 45.0/89.2/268.0 | 0 | 1.43 cores | 1.94 GiB | 0.15 cores | PASS | 10000000 rows in 8.7s, 321.9 MB/s, 224.7 MB/s per server core; 4 files, avg 98.4 MiB |
write pareto x16: 685.6 MB/s, 380.6 MB/s per server core, 2437315 rows/s, client mean 106.73 ms per batch
    89.036 ms  83.4%  Gate.write_request, 612 profiles
    40.046 ms  37.5%  Scribe.group_sync
    32.503 ms  30.5%  Scribe.shard_wait
     5.186 ms   4.9%  Scribe.preprocess
     4.876 ms   4.6%  Scribe.group_fence
     4.487 ms   4.2%  Scribe.group_write
     1.428 ms   1.3%  Scribe.group_insert
     0.211 ms   0.2%  Scribe.group_commit
     0.014 ms   0.0%  Scribe.admission
     0.002 ms   0.0%  Scribe.decode
  0.16 fsyncs and 0.45 pool acquires per batch
cpu pareto, write x16: 12.08 ms server CPU per batch (target/bifrost-query-capacity/writes/parquet/profiles/02-write-16)
     3.049 ms  25.2%  c:ZSTD
     1.864 ms  15.4%  kernel
     1.433 ms  11.9%  sha2
     0.746 ms   6.2%  parquet
     0.554 ms   4.6%  chrono
     0.519 ms   4.3%  c:0x00000000001a17ff
     0.487 ms   4.0%  vala_bifrost_redux
     0.362 ms   3.0%  c:HUF
     0.342 ms   2.8%  c:0x00000000001a1852
     0.332 ms   2.7%  alloc
     0.325 ms   2.7%  c:HIST
     0.221 ms   1.8%  core
     0.153 ms   1.3%  wyrd_spec
     0.147 ms   1.2%  arrow_array
     0.134 ms   1.1%  arrow_select
     0.120 ms   1.0%  c:mi
     0.106 ms   0.9%  arrow_arith
     0.077 ms   0.6%  ahash
     0.075 ms   0.6%  c:0x00000000001a1d7a
     0.074 ms   0.6%  crc32c
| write x16 | 16 | 2437191 rows/s | 76.7/344.1/667.1 | 0 | 1.80 cores | 2.96 GiB | 0.39 cores | PASS | 10000000 rows in 4.1s, 685.6 MB/s, 380.6 MB/s per server core; 8 files, avg 73.7 MiB |
write pareto x64: 931.3 MB/s, 311.5 MB/s per server core, 3311126 rows/s, client mean 290.80 ms per batch
   259.639 ms  89.3%  Gate.write_request, 611 profiles
   113.638 ms  39.1%  Scribe.group_sync
    73.290 ms  25.2%  Scribe.shard_wait
    43.124 ms  14.8%  Scribe.group_write
    15.095 ms   5.2%  Scribe.group_fence
     6.331 ms   2.2%  Scribe.preprocess
     5.869 ms   2.0%  Scribe.group_insert
     0.971 ms   0.3%  Scribe.group_commit
     0.014 ms   0.0%  Scribe.admission
     0.002 ms   0.0%  Scribe.decode
  0.05 fsyncs and 0.24 pool acquires per batch
cpu pareto, write x64: 14.78 ms server CPU per batch (target/bifrost-query-capacity/writes/parquet/profiles/03-write-64)
     3.587 ms  24.3%  c:ZSTD
     2.150 ms  14.5%  kernel
     1.268 ms   8.6%  vala_bifrost_redux
     1.052 ms   7.1%  sha2
     1.008 ms   6.8%  parquet
     0.735 ms   5.0%  c:0x00000000001a17ff
     0.581 ms   3.9%  chrono
     0.406 ms   2.7%  c:HUF
     0.401 ms   2.7%  c:0x00000000001a1852
     0.356 ms   2.4%  c:HIST
     0.356 ms   2.4%  alloc
     0.327 ms   2.2%  arrow_row
     0.275 ms   1.9%  core
     0.242 ms   1.6%  arrow_select
     0.197 ms   1.3%  arrow_array
     0.182 ms   1.2%  c:mi
     0.143 ms   1.0%  wyrd_spec
     0.136 ms   0.9%  arrow_arith
     0.098 ms   0.7%  c:0x00000000001a1d7a
     0.086 ms   0.6%  ahash
| write x64 | 64 | 3311126 rows/s | 252.8/481.8/717.0 | 0 | 2.99 cores | 3.88 GiB | 0.87 cores | PASS | 10000000 rows in 3.0s, 931.3 MB/s, 311.5 MB/s per server core; 10 files, avg 77.9 MiB |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.6s |
