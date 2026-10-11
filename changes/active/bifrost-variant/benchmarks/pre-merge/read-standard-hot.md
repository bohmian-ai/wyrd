# Bifrost query capacity: 10000000 rows, hot rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1475413 rows/s | 43.0/62.4/65.1 | 0 | 0.52 cores | 3.27 GiB | 0.18 cores | PASS | 10000000 rows in 6.8s, 415.0 MB/s, 792.4 MB/s per server core; 0 files, avg 0.0 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | memtable 2659 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 2.7/12.3/54.4 | 0 | 1.08 cores | 3.32 GiB | 0.06 cores | PASS | 76 statements |
| selective sweep | 1 | 633.1 qps | 1.5/1.9/2.4 | 0 | 1.03 cores | 3.32 GiB | 0.07 cores | - | meets target |
| selective sweep | 4 | 2082.7 qps | 1.9/2.6/2.9 | 0 | 3.86 cores | 3.36 GiB | 0.25 cores | - | meets target |
| selective sweep | 8 | 3314.6 qps | 2.4/3.1/3.5 | 0 | 7.68 cores | 3.37 GiB | 0.43 cores | - | meets target |
| selective sweep | 16 | 5259.8 qps | 3.0/3.7/4.1 | 0 | 15.26 cores | 3.44 GiB | 0.75 cores | - | meets target |
| selective sweep | 32 | 6358.7 qps | 4.8/7.0/8.5 | 0 | 24.28 cores | 3.53 GiB | 0.78 cores | - | meets target |
| selective sweep | 64 | 6196.6 qps | 10.1/15.7/18.7 | 0 | 23.86 cores | 3.53 GiB | 0.71 cores | - | p50 10.1 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 6358.7 qps at 32 clients |
| small-aggregate sweep | 1 | 357.2 qps | 2.7/3.4/4.1 | 0 | 1.04 cores | 3.49 GiB | 0.05 cores | - | meets target |
| small-aggregate sweep | 4 | 1203.8 qps | 3.3/4.3/4.9 | 0 | 3.87 cores | 3.50 GiB | 0.17 cores | - | meets target |
| small-aggregate sweep | 8 | 2079.8 qps | 3.8/4.8/5.3 | 0 | 7.75 cores | 3.53 GiB | 0.32 cores | - | meets target |
| small-aggregate sweep | 16 | 3202.1 qps | 4.9/6.1/6.8 | 0 | 15.50 cores | 3.58 GiB | 0.57 cores | - | meets target |
| small-aggregate sweep | 32 | 3580.6 qps | 8.7/11.8/13.9 | 0 | 26.15 cores | 3.60 GiB | 0.66 cores | - | meets target |
| small-aggregate sweep | 64 | 3673.3 qps | 17.1/25.4/29.6 | 0 | 27.52 cores | 3.65 GiB | 0.66 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 871.5 qps | 9.0/11.0/12.2 | 0 | 7.84 cores | 3.59 GiB | 0.16 cores | PASS | meets target |
| table-aggregate | 1 | 24.9 qps | 39.7/43.4/45.8 | 0 | 1.46 cores | 3.60 GiB | 0.01 cores | PASS | meets target |
| broad-window | 1 | 19.3 qps | 51.5/55.3/57.1 | 0 | 1.34 cores | 3.62 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 40.4 qps | 24.6/26.6/27.1 | 0 | 1.35 cores | 3.59 GiB | 0.01 cores | PASS | no target on this fixture |
| rows still hot | - | - | - | - | - | - | - | PASS | memtable 2708 MiB, 0 staged, 0 claims, 0 published files |
| small-aggregate while writing | 4 | 952.9 qps | 3.8/6.3/10.6 | 0 | 4.60 cores | 7.78 GiB | 0.28 cores | PASS | p95 alone 4.3 ms |
| write during small-aggregate | 4 | 1163978 rows/s | 45.4/90.4/110.4 | 0 | 3.57 cores | 9.01 GiB | 0.28 cores | PASS | 19677184 rows in 16.9s, 327.5 MB/s, 91.6 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 407.2 qps | 8.9/13.2/32.8 | 0 | 4.87 cores | 12.18 GiB | 0.22 cores | PASS | p95 alone 10.0 ms |
| write during 1m-aggregate | 4 | 1144003 rows/s | 45.2/90.3/140.9 | 0 | 3.78 cores | 12.75 GiB | 0.22 cores | PASS | 18825216 rows in 16.5s, 321.9 MB/s, 85.1 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 2.21 cores | 12.85 GiB | 0.14 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.0s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 14.2s |
