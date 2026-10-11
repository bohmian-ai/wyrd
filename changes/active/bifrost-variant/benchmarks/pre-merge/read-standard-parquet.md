# Bifrost query capacity: 10000000 rows, parquet rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1559856 rows/s | 44.1/58.4/66.2 | 0 | 1.46 cores | 1.64 GiB | 0.20 cores | PASS | 10000000 rows in 6.4s, 438.8 MB/s, 299.6 MB/s per server core; 1 files, avg 97.3 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | published in 65s: memtable 0 MiB, 0 staged, 0 claims, 6 published files |
| exact answers | 1 | - | 5.1/19.1/81.0 | 0 | 2.33 cores | 0.73 GiB | 0.03 cores | PASS | 76 statements |
| selective sweep | 1 | 493.2 qps | 2.0/2.5/2.8 | 0 | 1.07 cores | 0.60 GiB | 0.06 cores | - | meets target |
| selective sweep | 4 | 1775.0 qps | 2.2/2.8/3.0 | 0 | 4.03 cores | 0.64 GiB | 0.23 cores | - | meets target |
| selective sweep | 8 | 3321.7 qps | 2.4/2.9/3.3 | 0 | 7.91 cores | 0.69 GiB | 0.44 cores | - | meets target |
| selective sweep | 16 | 5449.2 qps | 2.9/3.6/4.2 | 0 | 15.13 cores | 0.86 GiB | 0.83 cores | - | meets target |
| selective sweep | 32 | 6660.2 qps | 4.7/6.6/7.9 | 0 | 22.49 cores | 1.13 GiB | 0.95 cores | - | meets target |
| selective sweep | 64 | 6170.6 qps | 10.0/16.1/19.5 | 0 | 21.08 cores | 1.18 GiB | 0.82 cores | - | p50 10.0 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6660.2 qps at 32 clients |
| small-aggregate sweep | 1 | 213.7 qps | 4.6/5.6/6.2 | 0 | 1.04 cores | 0.73 GiB | 0.03 cores | - | meets target |
| small-aggregate sweep | 4 | 788.9 qps | 5.0/6.0/6.4 | 0 | 4.03 cores | 0.76 GiB | 0.12 cores | - | meets target |
| small-aggregate sweep | 8 | 1474.0 qps | 5.4/6.4/6.9 | 0 | 7.97 cores | 0.83 GiB | 0.23 cores | - | meets target |
| small-aggregate sweep | 16 | 2534.7 qps | 6.3/7.4/8.3 | 0 | 15.57 cores | 0.89 GiB | 0.44 cores | - | meets target |
| small-aggregate sweep | 32 | 3550.5 qps | 8.8/11.6/13.3 | 0 | 25.54 cores | 1.21 GiB | 0.56 cores | - | meets target |
| small-aggregate sweep | 64 | 3492.8 qps | 17.7/27.2/32.1 | 0 | 26.01 cores | 1.34 GiB | 0.53 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 407.6 qps | 20.7/23.4/25.2 | 0 | 8.13 cores | 1.52 GiB | 0.07 cores | PASS | meets target |
| table-aggregate | 1 | 60.7 qps | 16.2/18.7/20.0 | 0 | 3.24 cores | 1.43 GiB | 0.02 cores | PASS | meets target |
| broad-window | 1 | 26.2 qps | 37.7/40.8/43.6 | 0 | 3.01 cores | 1.49 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 13.8 qps | 71.4/79.2/81.5 | 0 | 9.48 cores | 2.08 GiB | 0.01 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 665.1 qps | 5.8/7.5/8.8 | 0 | 5.95 cores | 3.15 GiB | 0.31 cores | PASS | p95 alone 6.2 ms |
| write during small-aggregate | 4 | 1331081 rows/s | 45.0/75.8/190.0 | 0 | 4.45 cores | 3.15 GiB | 0.30 cores | PASS | 23363584 rows in 17.6s, 374.5 MB/s, 84.2 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 186.3 qps | 22.2/25.9/35.1 | 0 | 6.02 cores | 3.32 GiB | 0.22 cores | PASS | p95 alone 23.3 ms |
| write during 1m-aggregate | 4 | 1386678 rows/s | 45.0/69.0/137.3 | 0 | 4.69 cores | 3.32 GiB | 0.23 cores | PASS | 23281664 rows in 16.8s, 390.2 MB/s, 83.2 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 14.11 cores | 3.12 GiB | 0.83 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.8s |
