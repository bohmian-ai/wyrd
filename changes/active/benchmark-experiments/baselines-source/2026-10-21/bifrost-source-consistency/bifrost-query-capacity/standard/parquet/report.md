# Bifrost query capacity: 10000000 rows, parquet rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1513406 rows/s | 43.890/57.342/173.627 | 0 | 1.64 cores | 2.11 GiB | 0.18 cores | PASS | 10000000 rows in 6.6s, 528.9 MB/s, 323.3 MB/s per server core; 1 files, avg 77.8 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | published in 62s: memtable 0 MiB, 0 staged, 0 claims, 7 published files |
| exact answers | 1 | - | 4.486/20.515/76.532 | 0 | 2.27 cores | 0.78 GiB | 0.02 cores | PASS | 76 statements |
| selective sweep | 1 | 512.7 qps | 1.917/2.442/2.745 | 0 | 1.16 cores | 0.67 GiB | 0.06 cores | - | p99 2.745 ms > 1 ms |
| selective sweep | 4 | 1934.7 qps | 2.041/2.553/2.857 | 0 | 4.12 cores | 0.74 GiB | 0.22 cores | - | p99 2.857 ms > 1 ms |
| selective sweep | 8 | 3503.6 qps | 2.254/2.790/3.127 | 0 | 8.02 cores | 0.73 GiB | 0.44 cores | - | p99 3.127 ms > 1 ms |
| selective sweep | 16 | 5895.3 qps | 2.669/3.335/3.813 | 0 | 15.57 cores | 1.21 GiB | 0.86 cores | - | p99 3.813 ms > 1 ms |
| selective sweep | 32 | 7252.3 qps | 4.283/5.980/7.046 | 0 | 23.74 cores | 1.37 GiB | 0.94 cores | - | p99 7.046 ms > 1 ms |
| selective sweep | 64 | 7410.7 qps | 8.318/13.169/15.668 | 0 | 26.00 cores | 1.61 GiB | 0.88 cores | - | p99 15.668 ms > 1 ms |
| selective target | - | - | - | - | - | - | - | FAIL | 1 client: p99 2.745 ms > 1 ms |
| small-aggregate sweep | 1 | 210.0 qps | 4.678/5.657/6.381 | 0 | 1.19 cores | 1.26 GiB | 0.03 cores | - | meets target |
| small-aggregate sweep | 4 | 826.4 qps | 4.788/5.577/6.077 | 0 | 4.12 cores | 0.90 GiB | 0.11 cores | - | meets target |
| small-aggregate sweep | 8 | 1547.1 qps | 5.089/6.049/6.809 | 0 | 8.02 cores | 0.96 GiB | 0.22 cores | - | meets target |
| small-aggregate sweep | 16 | 2683.7 qps | 5.873/7.059/8.086 | 0 | 15.80 cores | 1.06 GiB | 0.44 cores | - | meets target |
| small-aggregate sweep | 32 | 3564.9 qps | 8.702/11.756/13.789 | 0 | 25.51 cores | 1.40 GiB | 0.54 cores | - | meets target |
| small-aggregate sweep | 64 | 3841.5 qps | 16.124/24.526/28.922 | 0 | 28.98 cores | 1.67 GiB | 0.56 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 399.4 qps | 21.174/23.185/24.721 | 0 | 8.04 cores | 1.73 GiB | 0.08 cores | PASS | meets target |
| table-aggregate | 1 | 56.9 qps | 17.422/19.127/20.217 | 0 | 3.03 cores | 1.70 GiB | 0.02 cores | PASS | meets target |
| broad-window | 1 | 26.8 qps | 37.098/39.156/41.686 | 0 | 3.09 cores | 1.68 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 14.4 qps | 69.079/74.288/76.727 | 0 | 9.36 cores | 2.16 GiB | 0.00 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 669.5 qps | 5.814/7.651/8.663 | 0 | 6.27 cores | 3.47 GiB | 0.31 cores | PASS | p95 alone 5.7 ms |
| write during small-aggregate | 4 | 1617773 rows/s | 44.453/59.107/86.151 | 0 | 5.14 cores | 3.47 GiB | 0.35 cores | PASS | 27394048 rows in 16.9s, 566.1 MB/s, 110.0 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 185.4 qps | 22.729/25.668/27.351 | 0 | 6.34 cores | 3.41 GiB | 0.22 cores | PASS | p95 alone 22.8 ms |
| write during 1m-aggregate | 4 | 1436569 rows/s | 44.110/77.718/185.380 | 0 | 5.13 cores | 3.45 GiB | 0.25 cores | PASS | 23052288 rows in 16.0s, 502.4 MB/s, 97.9 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 3.71 cores | 3.69 GiB | 0.16 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.0s; sizing DIVERGENT: 1661 row groups sized, 320 scanned, 0 fallbacks |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.4s |
