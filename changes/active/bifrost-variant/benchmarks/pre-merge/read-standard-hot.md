# Bifrost query capacity: 10000000 rows, hot rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1543239 rows/s | 44.6/62.7/86.2 | 0 | 0.98 cores | 3.09 GiB | 0.19 cores | PASS | 10000000 rows in 6.5s, 434.1 MB/s, 442.3 MB/s per server core; 0 files, avg 0.0 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | memtable 2382 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 2.8/12.0/64.3 | 0 | 1.07 cores | 3.15 GiB | 0.03 cores | PASS | 76 statements |
| selective sweep | 1 | 648.5 qps | 1.5/1.9/2.2 | 0 | 1.03 cores | 3.14 GiB | 0.08 cores | - | meets target |
| selective sweep | 4 | 2178.8 qps | 1.8/2.5/2.9 | 0 | 3.87 cores | 3.17 GiB | 0.26 cores | - | meets target |
| selective sweep | 8 | 3514.4 qps | 2.2/2.9/3.2 | 0 | 7.67 cores | 3.20 GiB | 0.44 cores | - | meets target |
| selective sweep | 16 | 5417.1 qps | 2.9/3.6/4.0 | 0 | 15.24 cores | 3.27 GiB | 0.76 cores | - | meets target |
| selective sweep | 32 | 6466.1 qps | 4.7/7.1/8.6 | 0 | 23.70 cores | 3.32 GiB | 0.80 cores | - | meets target |
| selective sweep | 64 | 6851.6 qps | 9.1/13.8/16.4 | 0 | 25.29 cores | 3.36 GiB | 0.79 cores | - | p50 9.1 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6851.6 qps at 64 clients |
| small-aggregate sweep | 1 | 364.1 qps | 2.7/3.3/3.9 | 0 | 1.04 cores | 3.31 GiB | 0.05 cores | - | meets target |
| small-aggregate sweep | 4 | 1259.5 qps | 3.1/4.0/4.7 | 0 | 3.90 cores | 3.32 GiB | 0.18 cores | - | meets target |
| small-aggregate sweep | 8 | 2200.9 qps | 3.6/4.5/5.0 | 0 | 7.79 cores | 3.34 GiB | 0.33 cores | - | meets target |
| small-aggregate sweep | 16 | 3358.6 qps | 4.7/5.8/6.3 | 0 | 15.55 cores | 3.36 GiB | 0.59 cores | - | meets target |
| small-aggregate sweep | 32 | 3819.7 qps | 8.1/11.0/12.9 | 0 | 26.47 cores | 3.42 GiB | 0.67 cores | - | meets target |
| small-aggregate sweep | 64 | 3782.2 qps | 16.6/24.8/29.2 | 0 | 27.01 cores | 3.45 GiB | 0.65 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 923.8 qps | 8.5/10.4/11.4 | 0 | 7.90 cores | 3.40 GiB | 0.16 cores | PASS | meets target |
| table-aggregate | 1 | 25.1 qps | 39.4/42.6/44.0 | 0 | 1.45 cores | 3.41 GiB | 0.01 cores | PASS | meets target |
| broad-window | 1 | 19.6 qps | 50.9/53.5/55.5 | 0 | 1.34 cores | 3.41 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 39.6 qps | 25.1/27.0/28.3 | 0 | 1.35 cores | 3.38 GiB | 0.01 cores | PASS | no target on this fixture |
| rows still hot | - | - | - | - | - | - | - | PASS | memtable 2705 MiB, 0 staged, 0 claims, 0 published files |
| small-aggregate while writing | 4 | 972.1 qps | 3.6/6.2/14.4 | 0 | 4.96 cores | 8.04 GiB | 0.32 cores | PASS | p95 alone 4.0 ms |
| write during small-aggregate | 4 | 1387461 rows/s | 45.0/69.0/95.6 | 0 | 4.01 cores | 9.49 GiB | 0.32 cores | PASS | 22282240 rows in 16.1s, 390.3 MB/s, 97.4 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 414.0 qps | 8.9/13.9/24.2 | 0 | 5.06 cores | 13.94 GiB | 0.24 cores | PASS | p95 alone 9.8 ms |
| write during 1m-aggregate | 4 | 1369189 rows/s | 46.0/72.1/91.0 | 0 | 4.01 cores | 15.49 GiB | 0.25 cores | PASS | 22446080 rows in 16.4s, 385.3 MB/s, 96.1 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 2.39 cores | 15.50 GiB | 0.15 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.0s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 15.8s |
