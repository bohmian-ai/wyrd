# Bifrost query capacity: 10000000 rows, 8 CPU / 16 GiB

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | seals | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|---|
| write events | 4 | 1081154 rows/s | 48.5/90.0/111.3 | 0 | 0.78 cores | 2.21 GiB | 0 | 0.12 cores | PASS | 10000000 rows in 9.2s; 0 files, avg 0.0 MiB |
| exact answers | 1 | - | 13.5/26.1/94.7 | 0 | 1.57 cores | 2.31 GiB | 0 | 0.02 cores | PASS | 76 statements |
| selective sweep | 1 | 146.1 qps | 6.8/8.2/8.9 | 0 | 0.95 cores | 1.33 GiB | 0 | 0.03 cores | - | meets target |
| selective sweep | 4 | 474.1 qps | 8.4/10.0/11.0 | 0 | 3.19 cores | 1.35 GiB | 0 | 0.07 cores | - | p50 8.4 ms >= 7 ms |
| selective sweep | 8 | 771.6 qps | 10.3/12.1/13.3 | 0 | 5.32 cores | 1.38 GiB | 0 | 0.13 cores | - | p50 10.3 ms >= 7 ms |
| selective sweep | 16 | 1174.9 qps | 13.5/16.0/17.6 | 0 | 7.92 cores | 1.39 GiB | 0 | 0.21 cores | - | p50 13.5 ms >= 7 ms |
| selective sweep | 32 | 1222.3 qps | 25.0/33.7/36.1 | 0 | 8.00 cores | 1.39 GiB | 0 | 0.23 cores | - | p50 25.0 ms >= 7 ms |
| selective sweep | 64 | 1218.0 qps | 52.6/59.2/62.2 | 0 | 7.99 cores | 1.40 GiB | 0 | 0.23 cores | - | p50 52.6 ms >= 7 ms |
| selective target | - | - | - | - | - | - | - | - | PASS | latency met at 1 client; peak 1222.3 qps at 32 clients |
| small-aggregate sweep | 1 | 78.5 qps | 12.8/14.7/16.2 | 0 | 1.20 cores | 1.38 GiB | 0 | 0.02 cores | - | 78.5 qps < 200 |
| small-aggregate sweep | 4 | 252.3 qps | 15.8/18.9/21.2 | 0 | 3.95 cores | 1.46 GiB | 0 | 0.04 cores | - | meets target |
| small-aggregate sweep | 8 | 428.6 qps | 18.7/21.8/23.5 | 0 | 6.46 cores | 1.48 GiB | 0 | 0.08 cores | - | meets target |
| small-aggregate sweep | 16 | 550.0 qps | 28.8/36.2/39.5 | 0 | 7.99 cores | 1.50 GiB | 0 | 0.12 cores | - | meets target |
| small-aggregate sweep | 32 | 554.7 qps | 57.4/69.7/74.5 | 0 | 8.00 cores | 1.50 GiB | 0 | 0.13 cores | - | meets target |
| small-aggregate sweep | 64 | 551.5 qps | 115.4/128.7/134.3 | 0 | 7.99 cores | 1.53 GiB | 0 | 0.13 cores | - | p95 128.7 ms >= 100 ms |
| small-aggregate target | - | - | - | - | - | - | - | - | PASS | met at 4, 8, 16, 32 clients |
| 1m-aggregate | 8 | 231.5 qps | 35.2/41.8/45.2 | 0 | 7.79 cores | 1.52 GiB | 0 | 0.06 cores | PASS | meets target |
| table-aggregate | 1 | 17.2 qps | 57.8/62.6/63.8 | 0 | 3.18 cores | 1.48 GiB | 0 | 0.00 cores | PASS | meets target |
| broad-window | 1 | 14.3 qps | 69.3/74.6/76.7 | 0 | 2.52 cores | 1.46 GiB | 0 | 0.00 cores | PASS | no target on this fixture |
| full-scan | 1 | 12.2 qps | 81.6/87.8/89.5 | 0 | 6.38 cores | 1.63 GiB | 0 | 0.00 cores | PASS | no target on this fixture |
| small-aggregate while writing | 4 | 226.5 qps | 17.5/20.9/24.7 | 0 | 4.52 cores | 3.05 GiB | 0 | 0.19 cores | PASS | p95 alone 18.5 ms |
| write during small-aggregate | 4 | 1102451 rows/s | 51.3/89.9/106.1 | 0 | 4.54 cores | 3.05 GiB | 0 | 0.19 cores | PASS | 13287424 rows in 12.1s; 0 files, avg 0.0 MiB |
| 1m-aggregate while writing | 4 | 118.4 qps | 33.6/38.2/42.2 | 0 | 5.29 cores | 4.80 GiB | 0 | 0.16 cores | PASS | p95 alone 33.2 ms |
| write during 1m-aggregate | 4 | 1008809 rows/s | 62.7/89.6/99.2 | 0 | 5.16 cores | 4.80 GiB | 0 | 0.16 cores | PASS | 12189696 rows in 12.1s; 0 files, avg 0.0 MiB |
| full queue | 1001 | - | - | 0 | 2.31 cores | 4.96 GiB | 0 | 0.12 cores | PASS | 1000 queued, overflow WYRD_VALA_429_QUERY_QUEUE_FULL, emptied in 0.1s |
| shutdown | - | - | - | - | - | - | - | - | PASS | exited cleanly in 1.1s |
