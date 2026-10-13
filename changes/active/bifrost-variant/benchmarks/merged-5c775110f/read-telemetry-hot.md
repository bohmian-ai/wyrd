# Bifrost query capacity: 10000000 rows, hot telemetry spans, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| export spans | 4 | 73360 rows/s | 19.7/49.7/53.3 | 0 | 0.94 cores | 11.55 GiB | 0.20 cores | FAIL | 73360 rows/s < 200000 |
| spans in tier | - | - | - | - | - | - | - | PASS | memtable 8085 MiB, 0 staged, 0 claims, 0 published files |
| exact answers | 1 | - | 47.5/624.9/624.9 | 0 | 23.02 cores | 11.83 GiB | 0.00 cores | PASS | 9 statements |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 9.3s |
