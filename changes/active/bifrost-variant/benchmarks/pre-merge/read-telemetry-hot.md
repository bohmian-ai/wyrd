# Bifrost query capacity: 10000000 rows, hot telemetry spans, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| export spans | 4 | 61297 rows/s | 41.7/55.5/58.3 | 0 | 0.83 cores | 11.34 GiB | 0.18 cores | FAIL | 61297 rows/s < 200000 |
| spans in tier | - | - | - | - | - | - | - | PASS | memtable 8077 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 47.8/920.8/920.8 | 0 | 24.80 cores | 11.59 GiB | 0.00 cores | PASS | 9 statements |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 10.5s |
