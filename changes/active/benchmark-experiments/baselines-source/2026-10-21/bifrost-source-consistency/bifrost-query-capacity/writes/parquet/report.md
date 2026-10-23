# Bifrost query capacity: 10000000 rows, parquet rows per step, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| write x1 | 1 | 529993 rows/s | 41.919/52.872/60.935 | 0 | 0.62 cores | 1.14 GiB | 0.06 cores | PASS | 10000000 rows in 18.9s, 185.3 MB/s, 299.1 MB/s per server core; 3 files, avg 79.0 MiB |
| write x4 | 4 | 1445190 rows/s | 44.498/56.396/238.781 | 0 | 1.84 cores | 2.18 GiB | 0.19 cores | PASS | 10000000 rows in 6.9s, 505.0 MB/s, 274.7 MB/s per server core; 8 files, avg 58.9 MiB |
| write x16 | 16 | 3404101 rows/s | 66.578/135.231/167.378 | 0 | 3.98 cores | 3.32 GiB | 0.57 cores | PASS | 10000000 rows in 2.9s, 1189.1 MB/s, 298.8 MB/s per server core; 9 files, avg 61.1 MiB |
| write x64 | 64 | 3183886 rows/s | 222.577/621.380/775.313 | 0 | 4.11 cores | 4.15 GiB | 0.87 cores | PASS | 10000000 rows in 3.1s, 1112.1 MB/s, 270.8 MB/s per server core; 12 files, avg 64.9 MiB |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.7s |
