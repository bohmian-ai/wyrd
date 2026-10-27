# Bifrost query capacity: 10000000 rows, hot rows, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1821051 rows/s | 43.725/54.453/68.549 | 0 | 0.87 cores | 4.01 GiB | 0.23 cores | PASS | 10000000 rows in 5.5s, 636.4 MB/s, 734.2 MB/s per server core; 0 files, avg 0.0 MiB |
| rows in tier | - | - | - | - | - | - | - | PASS | memtable 2905 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 2.641/13.679/52.914 | 0 | 1.22 cores | 4.07 GiB | 0.06 cores | PASS | 76 statements |
| selective sweep | 1 | 681.1 qps | 1.394/1.940/2.352 | 0 | 1.17 cores | 4.13 GiB | 0.07 cores | - | p99 2.352 ms > 1 ms |
| selective sweep | 4 | 2349.1 qps | 1.622/2.324/2.623 | 0 | 4.20 cores | 4.26 GiB | 0.25 cores | - | p99 2.623 ms > 1 ms |
| selective sweep | 8 | 3923.2 qps | 2.007/2.624/2.999 | 0 | 8.22 cores | 4.43 GiB | 0.48 cores | - | p99 2.999 ms > 1 ms |
| selective sweep | 16 | 6084.1 qps | 2.590/3.289/3.789 | 0 | 16.29 cores | 4.61 GiB | 0.83 cores | - | p99 3.789 ms > 1 ms |
| selective sweep | 32 | 7746.0 qps | 3.965/5.769/7.134 | 0 | 26.14 cores | 4.78 GiB | 0.85 cores | - | p99 7.134 ms > 1 ms |
| selective sweep | 64 | 8178.2 qps | 7.546/11.983/14.619 | 0 | 29.12 cores | 4.91 GiB | 0.86 cores | - | p99 14.619 ms > 1 ms |
| selective target | - | - | - | - | - | - | - | FAIL | 1 client: p99 2.352 ms > 1 ms |
| small-aggregate sweep | 1 | 359.7 qps | 2.681/3.454/3.861 | 0 | 1.19 cores | 4.94 GiB | 0.05 cores | - | meets target |
| small-aggregate sweep | 4 | 1309.9 qps | 2.963/3.848/4.402 | 0 | 4.17 cores | 5.02 GiB | 0.17 cores | - | meets target |
| small-aggregate sweep | 8 | 2247.5 qps | 3.505/4.362/4.906 | 0 | 8.20 cores | 5.12 GiB | 0.34 cores | - | meets target |
| small-aggregate sweep | 16 | 3348.0 qps | 4.704/5.969/6.911 | 0 | 16.35 cores | 5.27 GiB | 0.61 cores | - | meets target |
| small-aggregate sweep | 32 | 3795.6 qps | 8.162/11.192/13.305 | 0 | 27.51 cores | 5.37 GiB | 0.69 cores | - | meets target |
| small-aggregate sweep | 64 | 3874.3 qps | 16.244/24.631/28.966 | 0 | 29.11 cores | 5.49 GiB | 0.66 cores | - | meets target |
| small-aggregate target | - | - | - | - | - | - | - | PASS | met at 1, 4, 8, 16, 32, 64 clients |
| 1m-aggregate | 8 | 892.2 qps | 8.870/10.351/11.452 | 0 | 8.07 cores | 5.47 GiB | 0.18 cores | PASS | meets target |
| table-aggregate | 1 | 24.9 qps | 39.985/42.749/44.736 | 0 | 1.52 cores | 5.48 GiB | 0.01 cores | PASS | meets target |
| broad-window | 1 | 19.4 qps | 51.346/54.205/55.712 | 0 | 1.38 cores | 5.48 GiB | 0.01 cores | PASS | no target on this fixture |
| full-scan | 1 | 39.6 qps | 25.104/26.944/27.888 | 0 | 1.45 cores | 5.45 GiB | 0.01 cores | PASS | no target on this fixture |
| rows still hot | - | - | - | - | - | - | - | PASS | memtable 3983 MiB, 0 staged, 0 claims, 0 published files |
| small-aggregate while writing | 4 | 930.6 qps | 3.642/7.813/16.284 | 0 | 5.44 cores | 10.97 GiB | 0.30 cores | PASS | p95 alone 4.0 ms |
| write during small-aggregate | 4 | 1373438 rows/s | 45.055/80.452/97.954 | 0 | 4.32 cores | 12.86 GiB | 0.32 cores | PASS | 22364160 rows in 16.3s, 481.0 MB/s, 111.3 MB/s per server core; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 403.0 qps | 8.800/16.745/29.329 | 0 | 5.71 cores | 18.15 GiB | 0.24 cores | PASS | p95 alone 8.9 ms |
| write during 1m-aggregate | 4 | 1376305 rows/s | 45.022/82.014/103.834 | 0 | 5.06 cores | 18.87 GiB | 0.27 cores | PASS | 21774336 rows in 15.8s, 481.6 MB/s, 95.1 MB/s per server core; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 3.24 cores | 18.74 GiB | 0.13 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.0s |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 9.6s |
