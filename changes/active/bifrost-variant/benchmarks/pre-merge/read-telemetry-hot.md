# Bifrost query capacity: 10000000 rows, hot telemetry spans, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| export spans | 4 | 45492 rows/s | 44.0/71.0/75.0 | 0 | 0.90 cores | 12.46 GiB | 0.13 cores | FAIL | 45492 rows/s < 200000 |
| spans in tier | - | - | - | - | - | - | - | PASS | memtable 9437 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 56.1/877.1/877.1 | 0 | 23.38 cores | 12.77 GiB | 0.00 cores | PASS | 9 statements |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 7.8s |
