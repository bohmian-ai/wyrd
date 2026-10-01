# Focused follow-up: footer proof across file partitions

Result: **RESOLVED**. No proposed finding.

## Question and immutable subject

Independent focused review of whether the hot/staged footer comparison in every file partition contradicts REQ-015's “Opening a file compares that value with the query's tenant once”, or adds machinery beyond the authorized per-file proof. Candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`; immediate base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; cumulative original TASK-007 base: `a7582db587c6170a290760f1741673125612b797`. HEAD remained the supplied candidate when checked.

Scope is this uncertainty only, identified by `domain-review-concurrency.md`. Authority reviewed: approved spec revision 20 REQ-013/014/015, TASK-008, repository agent rules, spec-driven-development and maintainer-style references. Static review only; no tests, source changes, new synchronization, or cache proposals.

## Reachable producer-to-reader trace

1. `oracle/exec.rs:212-242` `partition_byte_ranges` divides concatenated file bytes into disjoint partition ranges. A single file can occur in several partitions; this is the existing REQ-013 partition policy, not footer-proof machinery.
2. Oracle hot source construction in `OracleTableProvider::persisted_inputs` (`exec.rs:1915-1936`) uses `HotParquetExec::with_partitions` with the session count. Scribe staged construction in `ScribeTailResolver::live_leaf` (`oracle/follower.rs:814-858`) uses the same leaf, count, and an authenticated tenant-bearing staged metadata key.
3. `HotParquetExec::partition_pieces` (`exec.rs:2741-2759`) gives each partition its own file/range list. `HotParquetExec::execute` (`exec.rs:2843-2868`) starts that partition's `hot_stream`.
4. In `hot_stream` (`exec.rs:2899-2966`), each file/range opens its own Parquet reader. It obtains retained metadata through the existing storage metadata owner, calls `tenant_proven_reader_metadata` once, then constructs `ParquetRecordBatchStreamBuilder::new_with_metadata`. The footer proof occurs before row-group ownership/pruning and before any decode. It is not repeated inside the yielded-batch loop (`exec.rs:2993-3001`).
5. `tenant_proven_reader_metadata` (`exec.rs:1135-1148`) calls the shared `verify_scanned_footer_tenant` once, then converts immutable retained metadata into the reader metadata. `parquet/footer.rs:49-55` performs the single exact tenant comparison; existing footer-key handling refuses missing/duplicate keys. No mutable proof state or per-row tenant check appears here.
6. The published sibling has the same opening boundary: `OracleIcebergScanExec::start_stream` (`exec.rs:1418-1459`) creates partition-specific scan tasks and readers. `PublishedFooterLoader::load` (`exec.rs:990-1046`) obtains metadata from the same cache and calls the same proof before delivering metadata to that reader. Thus the difference is the Parquet reader API, not a second tenant mechanism.

`storage/cache.rs:67-100` confirms the retained footer is immutable `Arc<ParquetMetaData>`. The candidate adds no proof cache, lock, global tenant-state owner, or coordination between partitions. The existing metadata owner remains responsible for decode single-flight and retention.

## Resolution

The domain report's observation is correct: a file split across N reading partitions can have N footer comparisons. Each comparison accompanies a distinct reader opening of that file/range, and each opening performs one comparison before any data is decoded. It is fixed opening work, independent of row count. This satisfies the stated once-when-opening boundary and preserves the mandated session partitioning.

REQ-013 separately and explicitly requires telemetry to count a file once “however many partitions read it”; REQ-015 does not state that stronger across-reader accounting rule for tenant proof. The existing first-range telemetry guard is therefore not authority to omit a later reader's proof. Requiring exactly one comparison per logical filename across all reader opens would add an unstated coordinated proof lifetime. Nothing in this focused trace justifies that machinery or changing the existing partition policy.

The added work is one footer tenant field and the one shared per-open tenant proof, with narrow adapters for the published and hot/staged reader APIs. This uncertainty yields no reachable noncompliance or material excess-complexity finding. No remediation is recommended.

Verification limit: source/caller evidence resolves the placement and frequency; no runtime comparison counter was executed, and this report does not assess other acceptance gaps or other reviewers' findings.
