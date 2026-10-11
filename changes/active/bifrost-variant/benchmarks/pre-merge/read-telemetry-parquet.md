# Bifrost query capacity: 10000000 rows, parquet telemetry spans, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| export spans | 4 | 57169 rows/s | 30.7/60.0/65.6 | 0 | 1.19 cores | 1.56 GiB | 0.16 cores | FAIL | 57169 rows/s < 200000 |
| spans in tier | - | - | - | - | - | - | - | PASS | published in 87s: memtable 0 MiB, 0 staged, 0 claims, 32 published files |
| exact answers | 1 | - | 27.7/303.2/303.2 | 0 | 16.30 cores | 1.33 GiB | 0.01 cores | PASS | 9 statements |
| variant shredding | - | - | - | - | - | - | - | PASS | 25 shredded and 0 unshredded span files; each shreds the 13 core and mid keys, not retry.count or custom.* |
| span-time pruning | - | - | - | - | - | - | - | PASS | window scanned 1 files and 1 row groups, unbounded 25 and 25 |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.1s |
