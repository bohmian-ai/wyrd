# Bifrost query capacity: 10000000 rows, hot rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1876891 rows/s | 42.7/46.1/58.8 | 0 | 0.67 cores | 3.30 GiB | 0.22 cores | PASS | 10000000 rows in 5.3s, 528.2 MB/s, 786.1 MB/s per server core; 0 files, avg 0.0 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | memtable 2388 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 2.6/26.5/82.1 | 0 | 0.97 cores | 3.34 GiB | 0.05 cores | PASS | 76 statements |
| selective sweep | 1 | 656.3 qps | 1.5/2.0/2.4 | 0 | 1.12 cores | 3.38 GiB | 0.07 cores | - | meets target |
| selective sweep | 4 | 2342.3 qps | 1.6/2.4/2.7 | 0 | 3.98 cores | 3.49 GiB | 0.23 cores | - | meets target |
| selective sweep | 8 | 3610.0 qps | 2.2/2.9/3.3 | 0 | 7.73 cores | 3.61 GiB | 0.42 cores | - | meets target |
| selective sweep | 16 | 5438.9 qps | 2.9/3.6/4.1 | 0 | 15.27 cores | 3.83 GiB | 0.72 cores | - | meets target |
| selective sweep | 32 | 6283.1 qps | 4.9/7.1/8.5 | 0 | 24.45 cores | 4.13 GiB | 0.79 cores | - | meets target |
| selective sweep | 64 | 6367.6 qps | 9.8/14.9/17.8 | 0 | 25.21 cores | 4.32 GiB | 0.77 cores | - | p50 9.8 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6367.6 qps at 64 clients |
| small-aggregate sweep | 1 | 358.0 qps | 2.7/3.4/3.8 | 0 | 1.15 cores | 4.34 GiB | 0.05 cores | - | meets target |
| small-aggregate sweep | 4 | 1345.9 qps | 2.9/3.9/4.4 | 0 | 4.00 cores | 4.44 GiB | 0.16 cores | - | meets target |
| small-aggregate sweep | 8 | 2226.9 qps | 3.5/4.5/5.0 | 0 | 7.84 cores | 4.52 GiB | 0.32 cores | - | meets target |
| small-aggregate sweep | 16 | 3309.1 qps | 4.8/5.9/6.6 | 0 | 15.55 cores | 4.65 GiB | 0.57 cores | - | meets target |
| small-aggregate sweep | 32 | 3654.3 qps | 8.5/11.7/13.5 | 0 | 26.18 cores | 4.72 GiB | 0.68 cores | - | meets target |
| small-aggregate sweep | 64 | 3726.2 qps | 16.9/24.8/29.0 | 0 | 27.21 cores | 4.83 GiB | 0.68 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 875.3 qps | 9.1/10.4/11.4 | 0 | 7.82 cores | 4.85 GiB | 0.16 cores | PASS | meets target |
| table-aggregate | 1 | 25.4 qps | 39.0/41.9/43.7 | 0 | 1.48 cores | 4.86 GiB | 0.01 cores | PASS | meets target |
| broad-window | 1 | 19.6 qps | 50.8/53.5/54.6 | 0 | 1.35 cores | 4.86 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 40.8 qps | 24.4/25.9/26.2 | 0 | 1.41 cores | 4.86 GiB | 0.01 cores | PASS | no target on this fixture |
| rows still hot | - | - | - | - | - | - | - | PASS | memtable 3251 MiB, 0 staged, 0 claims, 0 published files |
| small-aggregate while writing | 4 | 1148.1 qps | 3.2/5.3/6.7 | 0 | 4.59 cores | 11.18 GiB | 0.33 cores | PASS | p95 alone 3.9 ms |
| write during small-aggregate | 4 | 1637073 rows/s | 44.0/59.0/66.1 | 0 | 3.48 cores | 13.56 GiB | 0.33 cores | PASS | 27770880 rows in 17.0s, 461.7 MB/s, 132.7 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 418.8 qps | 8.5/14.2/28.1 | 0 | 5.12 cores | 18.38 GiB | 0.23 cores | PASS | p95 alone 9.1 ms |
| write during 1m-aggregate | 4 | 1328345 rows/s | 45.0/82.1/104.2 | 0 | 4.16 cores | 20.34 GiB | 0.24 cores | PASS | 22642688 rows in 17.0s, 374.3 MB/s, 89.9 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 4.02 cores | 20.34 GiB | 0.16 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 15.0s |
