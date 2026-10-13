# Bifrost query capacity: 10000000 rows, parquet rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1316453 rows/s | 44.1/59.8/432.7 | 0 | 1.26 cores | 1.73 GiB | 0.16 cores | PASS | 10000000 rows in 7.6s, 370.5 MB/s, 293.2 MB/s per server core; 1 files, avg 98.2 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | published in 60s: memtable 0 MiB, 0 staged, 0 claims, 6 published files |
| exact answers | 1 | - | 5.1/21.4/79.3 | 0 | 2.40 cores | 0.79 GiB | 0.00 cores | PASS | 76 statements |
| selective sweep | 1 | 509.3 qps | 1.9/2.4/2.7 | 0 | 1.15 cores | 0.71 GiB | 0.06 cores | - | meets target |
| selective sweep | 4 | 1934.3 qps | 2.0/2.5/2.8 | 0 | 4.15 cores | 0.80 GiB | 0.21 cores | - | meets target |
| selective sweep | 8 | 3409.8 qps | 2.3/2.8/3.2 | 0 | 8.03 cores | 0.79 GiB | 0.41 cores | - | meets target |
| selective sweep | 16 | 5567.5 qps | 2.8/3.5/3.9 | 0 | 15.54 cores | 1.18 GiB | 0.79 cores | - | meets target |
| selective sweep | 32 | 6562.5 qps | 4.7/6.6/7.6 | 0 | 23.49 cores | 1.25 GiB | 0.92 cores | - | meets target |
| selective sweep | 64 | 6585.6 qps | 9.4/14.6/17.3 | 0 | 23.87 cores | 1.34 GiB | 0.88 cores | - | p50 9.4 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6585.6 qps at 64 clients |
| small-aggregate sweep | 1 | 213.7 qps | 4.6/5.5/6.3 | 0 | 1.17 cores | 1.17 GiB | 0.03 cores | - | meets target |
| small-aggregate sweep | 4 | 823.1 qps | 4.8/5.6/6.1 | 0 | 4.10 cores | 1.06 GiB | 0.11 cores | - | meets target |
| small-aggregate sweep | 8 | 1523.2 qps | 5.2/6.2/7.1 | 0 | 7.96 cores | 1.05 GiB | 0.21 cores | - | meets target |
| small-aggregate sweep | 16 | 2643.5 qps | 6.0/7.2/8.3 | 0 | 15.69 cores | 1.13 GiB | 0.43 cores | - | meets target |
| small-aggregate sweep | 32 | 3497.2 qps | 8.9/11.9/13.8 | 0 | 25.39 cores | 1.36 GiB | 0.54 cores | - | meets target |
| small-aggregate sweep | 64 | 3732.2 qps | 16.6/25.3/30.0 | 0 | 27.88 cores | 1.70 GiB | 0.54 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 430.4 qps | 20.6/22.8/24.0 | 0 | 8.21 cores | 1.09 GiB | 0.08 cores | PASS | meets target |
| table-aggregate | 1 | 71.4 qps | 13.7/16.9/18.0 | 0 | 3.73 cores | 0.93 GiB | 0.02 cores | PASS | meets target |
| broad-window | 1 | 29.4 qps | 33.7/36.1/38.6 | 0 | 3.37 cores | 1.01 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 14.3 qps | 69.2/75.9/79.2 | 0 | 9.52 cores | 1.64 GiB | 0.01 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 683.2 qps | 5.6/7.4/8.9 | 0 | 5.89 cores | 2.74 GiB | 0.29 cores | PASS | p95 alone 5.6 ms |
| write during small-aggregate | 4 | 1514348 rows/s | 44.5/64.7/152.2 | 0 | 4.63 cores | 2.84 GiB | 0.31 cores | PASS | 26853376 rows in 17.7s, 426.9 MB/s, 92.2 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 193.4 qps | 21.3/24.9/74.3 | 0 | 5.68 cores | 2.95 GiB | 0.20 cores | PASS | p95 alone 22.2 ms |
| write during 1m-aggregate | 4 | 1302055 rows/s | 44.0/111.5/355.8 | 0 | 4.43 cores | 2.95 GiB | 0.22 cores | PASS | 22069248 rows in 16.9s, 366.7 MB/s, 82.7 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 3.83 cores | 2.97 GiB | 0.16 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.3s |
