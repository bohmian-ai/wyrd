# Bifrost query capacity: 10000000 rows, parquet telemetry spans, AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux

| step | clients | rate | p50/p95/p99 ms | errors | server CPU | peak memory | driver CPU | verdict | reason |
|---|---|---|---|---|---|---|---|---|---|
| export spans | 4 | 67841 rows/s | 20.8/50.4/54.9 | 0 | 1.32 cores | 1.60 GiB | 0.19 cores | FAIL | 67841 rows/s < 200000 |
| spans in tier | - | - | - | - | - | - | - | PASS | published in 58s: memtable 0 MiB, 0 staged, 0 claims, 30 published files |
| exact answers | 1 | - | 26.2/161.4/161.4 | 0 | 13.04 cores | 1.33 GiB | 0.00 cores | PASS | 9 statements |
| variant shredding | - | - | - | - | - | - | - | PASS | 24 shredded and 0 unshredded span files; each shreds the 13 core and mid keys, not retry.count or custom.* |
| span-time pruning | - | - | - | - | - | - | - | PASS | window scanned 1 files and 1 row groups, unbounded 24 and 24 |
| shutdown | - | - | - | - | - | - | - | PASS | exited cleanly in 0.1s |
