# Bifrost query capacity: 10000000 rows, parquet rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1191734 rows/s | 44.0/86.3/194.0 | 0 | 1.20 cores | 1.62 GiB | 0.14 cores | PASS | 10000000 rows in 8.4s, 335.2 MB/s, 279.9 MB/s per server core; 1 files, avg 98.6 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | published in 63s: memtable 0 MiB, 0 staged, 0 claims, 7 published files |
| exact answers | 1 | - | 4.9/19.1/78.2 | 0 | 2.63 cores | 0.84 GiB | 0.02 cores | PASS | 76 statements |
| selective sweep | 1 | 498.5 qps | 2.0/2.5/2.8 | 0 | 1.07 cores | 0.69 GiB | 0.06 cores | - | meets target |
| selective sweep | 4 | 1780.6 qps | 2.2/2.8/3.1 | 0 | 4.04 cores | 0.70 GiB | 0.23 cores | - | meets target |
| selective sweep | 8 | 3312.5 qps | 2.4/2.9/3.3 | 0 | 7.94 cores | 0.69 GiB | 0.44 cores | - | meets target |
| selective sweep | 16 | 5507.3 qps | 2.9/3.6/4.1 | 0 | 15.20 cores | 0.76 GiB | 0.82 cores | - | meets target |
| selective sweep | 32 | 6735.7 qps | 4.6/6.7/8.0 | 0 | 22.20 cores | 1.03 GiB | 0.93 cores | - | meets target |
| selective sweep | 64 | 6468.5 qps | 9.5/15.2/18.2 | 0 | 21.28 cores | 1.19 GiB | 0.82 cores | - | p50 9.5 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6735.7 qps at 32 clients |
| small-aggregate sweep | 1 | 213.3 qps | 4.6/5.6/6.2 | 0 | 1.06 cores | 0.77 GiB | 0.03 cores | - | meets target |
| small-aggregate sweep | 4 | 794.4 qps | 5.0/5.9/6.4 | 0 | 4.11 cores | 0.76 GiB | 0.12 cores | - | meets target |
| small-aggregate sweep | 8 | 1466.2 qps | 5.4/6.5/7.1 | 0 | 8.14 cores | 0.79 GiB | 0.23 cores | - | meets target |
| small-aggregate sweep | 16 | 2472.5 qps | 6.4/7.7/8.5 | 0 | 15.83 cores | 0.97 GiB | 0.44 cores | - | meets target |
| small-aggregate sweep | 32 | 3497.9 qps | 8.9/11.9/13.9 | 0 | 25.61 cores | 1.25 GiB | 0.55 cores | - | meets target |
| small-aggregate sweep | 64 | 3363.3 qps | 18.3/29.1/34.4 | 0 | 25.40 cores | 1.46 GiB | 0.51 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 500.3 qps | 14.8/24.4/26.3 | 0 | 10.19 cores | 1.69 GiB | 0.10 cores | PASS | meets target |
| table-aggregate | 1 | 70.8 qps | 13.9/15.9/16.8 | 0 | 4.26 cores | 1.58 GiB | 0.02 cores | PASS | meets target |
| broad-window | 1 | 34.1 qps | 28.9/32.2/38.2 | 0 | 4.28 cores | 1.67 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 13.6 qps | 73.3/79.5/81.3 | 0 | 9.56 cores | 2.20 GiB | 0.00 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 653.3 qps | 5.9/7.6/8.9 | 0 | 5.77 cores | 3.32 GiB | 0.27 cores | PASS | p95 alone 6.1 ms |
| write during small-aggregate | 4 | 1199128 rows/s | 44.3/87.9/206.3 | 0 | 4.38 cores | 3.32 GiB | 0.29 cores | PASS | 21118976 rows in 17.6s, 337.4 MB/s, 76.9 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 225.8 qps | 15.8/26.1/58.4 | 0 | 6.66 cores | 3.28 GiB | 0.19 cores | PASS | p95 alone 23.3 ms |
| write during 1m-aggregate | 4 | 1073465 rows/s | 44.4/119.7/305.6 | 0 | 5.02 cores | 3.28 GiB | 0.21 cores | PASS | 18333696 rows in 17.1s, 302.1 MB/s, 60.2 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 3.75 cores | 3.32 GiB | 0.16 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 1.1s |
