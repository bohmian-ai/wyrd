# Bifrost query capacity: 10000000 rows, 8 CPU / 16 GiB

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | seals | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 834380 rows/s | 83.7/123.5/185.7 | 0 | 0.69 cores | 1.94 GiB | 0 | 0.10 cores | PASS | 10000000 rows in 12.0s; 0 files, avg 0.0 MiB |
| exact answers | 1 | - | 17.1/41.5/110.5 | 0 | 1.52 cores | 1.42 GiB | 0 | 0.02 cores | PASS | 76 statements |
| selective sweep | 1 | 103.9 qps | 9.3/12.6/15.5 | 0 | 0.85 cores | 1.28 GiB | 0 | 0.03 cores | - | p50 9.3 ms >= 7 ms |
| selective sweep | 4 | 396.8 qps | 9.8/13.0/14.9 | 0 | 3.02 cores | 1.31 GiB | 0 | 0.07 cores | - | p50 9.8 ms >= 7 ms |
| selective sweep | 8 | 711.3 qps | 11.1/13.4/14.8 | 0 | 5.08 cores | 1.33 GiB | 0 | 0.13 cores | - | p50 11.1 ms >= 7 ms |
| selective sweep | 16 | 1109.3 qps | 14.3/16.8/18.2 | 0 | 7.66 cores | 1.35 GiB | 0 | 0.21 cores | - | p50 14.3 ms >= 7 ms |
| selective sweep | 32 | 1196.1 qps | 26.1/32.7/34.8 | 0 | 7.99 cores | 1.37 GiB | 0 | 0.24 cores | - | p50 26.1 ms >= 7 ms |
| selective sweep | 64 | 1212.1 qps | 52.9/59.1/61.3 | 0 | 7.98 cores | 1.38 GiB | 0 | 0.24 cores | - | p50 52.9 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | - | FAIL | 1 client: p50 9.3 ms >= 7 ms |
| small-aggregate sweep | 1 | 78.0 qps | 12.9/14.8/16.3 | 0 | 1.22 cores | 1.37 GiB | 0 | 0.02 cores | - | 78.0 qps < 200 |
| small-aggregate sweep | 4 | 249.8 qps | 16.0/18.4/20.0 | 0 | 3.90 cores | 1.41 GiB | 0 | 0.05 cores | - | meets target |
| small-aggregate sweep | 8 | 409.5 qps | 19.5/22.7/24.1 | 0 | 6.45 cores | 1.46 GiB | 0 | 0.08 cores | - | meets target |
| small-aggregate sweep | 16 | 517.6 qps | 30.6/38.5/42.3 | 0 | 7.99 cores | 1.47 GiB | 0 | 0.12 cores | - | meets target |
| small-aggregate sweep | 32 | 530.0 qps | 59.9/72.7/78.8 | 0 | 8.00 cores | 1.50 GiB | 0 | 0.13 cores | - | meets target |
| small-aggregate sweep | 64 | 531.7 qps | 119.8/132.2/138.2 | 0 | 7.99 cores | 1.50 GiB | 0 | 0.13 cores | - | p95 132.2 ms >= 100 ms |
| small-aggregate target | - | - | - | - | - | - | - | - | PASS | met at 4, 8, 16, 32 clients |
| 1m-aggregate | 8 | 179.3 qps | 43.8/63.6/75.0 | 0 | 7.02 cores | 1.48 GiB | 0 | 0.07 cores | PASS | meets target |
| table-aggregate | 1 | 16.7 qps | 59.5/65.0/67.0 | 0 | 3.22 cores | 1.45 GiB | 0 | 0.01 cores | PASS | meets target |
| broad-window | 1 | 14.1 qps | 70.6/75.2/77.6 | 0 | 2.61 cores | 1.44 GiB | 0 | 0.00 cores | PASS | no target on this fixture |
| full-scan | 1 | 10.4 qps | 95.7/106.6/109.3 | 0 | 6.05 cores | 1.64 GiB | 0 | 0.00 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 211.9 qps | 18.5/22.4/30.5 | 0 | 4.19 cores | 2.56 GiB | 0 | 0.17 cores | PASS | p95 alone 19.8 ms |
| write during small-aggregate | 4 | 907790 rows/s | 70.7/92.3/144.2 | 0 | 4.26 cores | 2.56 GiB | 0 | 0.17 cores | PASS | 10944512 rows in 12.1s; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 111.2 qps | 35.1/43.3/52.9 | 0 | 5.05 cores | 3.97 GiB | 0 | 0.17 cores | PASS | p95 alone 34.9 ms |
| write during 1m-aggregate | 4 | 1006206 rows/s | 61.8/89.2/142.1 | 0 | 4.98 cores | 3.97 GiB | 0 | 0.17 cores | PASS | 12140544 rows in 12.1s; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 2.46 cores | 4.29 GiB | 0 | 0.12 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | - | PASS | exited cleanly in 0.8s |
