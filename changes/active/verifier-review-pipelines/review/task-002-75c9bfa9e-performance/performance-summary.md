# TASK-002 performance addendum

Bounded static review completed against candidate `75c9bfa9e07163099eac40d5a556c4447c6a519d`, base `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`. No new material performance finding was independently validated. Original task verdict remains FIX_REQUIRED with FIND-TASK-002-1 through -7; this is not throughput or memory qualification.

Existing behavior and maintainer reviewers independently examined allocations, content copies, CPU/disk work, shared locks, blocking work, concurrency and backpressure. A fresh validator checked their source claims, coverage, bounds, arithmetic, materiality and empty new ledger. See [performance-validation.md](performance-validation.md), [performance-behavior.md](performance-behavior.md) and [performance-maintainer.md](performance-maintainer.md).

Useful preserved properties: borrowed call identity, shared Arc owners, one recording pass, bounded encoded result length, bounded active tool futures/workspace admission, blocking workers for filesystem traversal/writes/snapshots, short recording mutex holds, writer release before storage upload, shared pending checkpoint revisions and existing outbox backpressure. Snapshot immutability, domain authorization, durability ACK and provenance remain required; speed changes must preserve them.

Three concrete scaling costs remain:

1. Each fs.write traverses the current scratch tree and sorts file paths for byte accounting: O(E + F log F), plus path/content costs. Repeatedly creating N files can accumulate O(N² log N) work. The existing writer sees code/extraction mutations too; a write-only cached counter is insufficient.
2. Publication freezes full trees in memory. Writer exclusion ends before upload, so concurrent calls can retain multiple snapshots. Default illustrative overlap is 16 workspaces × 8 concurrent calls × 50 MiB = 6.25 GiB raw snapshot bytes, conditional on sufficient concurrency/slow storage. It excludes other allocations and is neither measured RSS nor a process maximum. Accepted publication-byte and Python-worker ceilings do not cap these buffers.
3. Recording retains full fs.write arguments, including overwrites/refusals, and full-state checkpoint cloning/counting/encoding revisits that history. Final scratch size therefore does not bound provenance or checkpoint memory/CPU. Full-state mechanics predate this task and remain the existing approved owner.

Capped bounds encoded Vec length, not allocator capacity or original input Value memory. No zero-copy, constant-time, latency/QPS or peak-memory claim is made. No new counter, cache, uploader, capture policy, process limit, checkpoint format or benchmark gate is prescribed without evidence of a material workload requirement. Future performance work should measure these three costs using representative entry counts, sizes, concurrency, storage latency and ACK waits, then correct the measured shared cause with minimum code.

Candidate HEAD and four original authority hashes remained unchanged. Concurrent user skill edits were preserved. No implementation, runtime tests, benchmarks, synthetic load, generators, commits or pushes occurred. Only additive review reports were written. Host-memory capacity and actual throughput remain unmeasured.
