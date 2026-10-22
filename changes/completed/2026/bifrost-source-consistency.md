# Bifrost source consistency

- Change: `bifrost-source-consistency`
- Specification: `SPEC-bifrost-source-consistency`, approved revision 2
- Completed: 2026-10-21
- Reviewed target: `6f22d3aee` (base `1c5edf729`, the merge of PR #189)
- Completion authority: explicit maintainer direction on 2026-10-21 to close out and skip review. The last task review (TASK-001-R2) passed its bounded remediation but kept original TASK-001 at `FIX_REQUIRED` because REQ-006 / AC-006 was unqualified. No integrated change review ran, and no change-review `PASS` was recorded.
- Delivery reference: PR #192

A Bifrost query no longer loses or duplicates rows when Scribe moves them from memory into staged or published files. Oracle selects and retains both sources before execution and retries a stale selection before it emits a row. Every reader uses this one corrected path, so a group Verifier's late evidence now reaches its verdict.

## Shipped behavior

- **Validated acquisition.** Oracle serves each table's published cut from its notification cut cache, and the cut carries a publication revision per Scribe node. Every selected online Scribe retains a live capture and reports its node's revision.
  - A report above the cut's revision means the cut is stale. Oracle evicts it, releases the whole selection and retries within the query deadline.
  - A warm query issues no Postgres statement. A publication that completes before capture costs exactly one cut read and one retry.
- **Publication revisions.** Each `vala.file_list` insert advances its node's table revision in the same transaction, in commit order. An exact replay advances nothing.
  - A replay of a set committed before revision tracking existed reports revision 0, so restart recovery completes.
  - A Scribe loads its node's revisions before it serves captures.
- **Retained captures.** Scribe captures memory batches and staged files while excluding freeze and publication advancement, and binds each capture to its query, tenant, table, deadline and writer epoch.
  - Peer assignments sign the capture id.
  - A capture whose deadline has passed is refused, and it expires on its Scribe.
  - Release is bounded by the original query deadline.
  - Losing an accepted capture fails the query. A Scribe that cannot be listed degrades the result.
- **Memory charge lifetime.** A retired generation's Arrow bytes stay charged to the Scribe root until its last holder drops: a frozen copy, capture, read, persistence payload or staging snapshot.
- **Removed behavior.** Execution-time live reacquisition, pre-row degradation on availability loss, and the publication-race duplication allowance are gone.
- **Capacity reports.** Direct verification overhead is judged at p99 ≤500 µs and selective query latency at p99 ≤1,000 µs. Both limits are inclusive and reported to three decimals.

## Lasting invariants

- Oracle owns reads, Scribe owns source authority and Vala SQL owns catalog state. No client or group scheduling path bypasses them.
- Appends gain no catalog request and no cluster-wide pause. Publication adds one revision read inside its existing transaction.
- An unresolved selection emits no rows, and loss during execution never reopens live data.
- Captures live within existing admission, memory, lease and deadline budgets, and cleanup cannot delete a source still in use.

## Material decisions

- Revision 2 replaced revision 1's per-query fresh cut read and validation read. Capacity runs had shown that read path regressing hot selective latency and throughput. Oracle now validates cached cuts in memory against Scribe-reported per-node watermarks, the pattern RisingWave Hummock, Apache Fluss and Kafka tiered storage use.
- Nothing had shipped, so the revision migration was edited in place.

## Revisions and deviations

- Spec revision 1 was approved and then superseded by revision 2, both on 2026-10-21.
- Task review of TASK-001 returned `FIX_REQUIRED` twice. Remediation R1 fixed the retained charge lifetime, the capture lifecycle order, deadline-bounded release, rustdoc and docs. R2 fixed the persistence charge, pre-revision replay and test-module rustdoc. The R2 review closed FIND-1 through FIND-6.
- **Open deviation: AC-006 / REQ-006 is not met.** Final capacity ran at `05186c1e4`. The build matches or beats the `21f7f2f40` baseline on every step except the parquet table-aggregate step (56.9 vs 66.4 QPS). Hot selective one-client p50/p99 was 1.39/2.35 ms (baseline 1.6/3.7). Peak selective throughput was 8,178 QPS hot and 7,411 QPS on parquet (baselines 6,821 and 6,606).
  - The strict p99 limits still fail: selective ≤1 ms and direct-verification overhead ≤0.5 ms, which the baseline also missed.
  - Capacity was not rerun after the R1/R2 runtime changes.
- The integrated `verify:bifrost` gate and full first-class journey regressions were not run, because the maintainer skipped change review.

## Acceptance closure

| Criterion | Evidence | Result |
|---|---|---|
| AC-001 exact row coverage | `published::source_transitions_preserve_exact_rows`; `tail_rpc` rollover and retained-run tests; `pg_oracle_membership::file_list_publications_advance_their_node_revision` | PASS |
| AC-002 invalid selections contained | `published::unvalidated_captures_fail_before_rows_and_release_resources`; `tail_rpc` capture authority and expiry tests; `hot_source` publishing-refusal test; `oracle::tail_discovery::tests::a_stalled_release_settles_by_the_query_deadline`; retained-charge shards tests; `persistence::tests::a_publication_committed_before_revision_tracking_recovers_at_revision_zero` | PASS |
| AC-003 late group evidence | `verify_in_real_time::grouped_executions_include_late_evidence` | PASS |
| AC-004 one current implementation | `distributed::tenant_scoped_cut_is_one_statement_and_one_bounded_retry`; `docs:check`; diff | PASS |
| AC-005 inclusive report limits | `verification::report::tests::direct_overhead_judges_p99_only`; `query::report::tests::sweep_and_errors_are_judged` | PASS |
| AC-006 final capacity | Baseline met at `05186c1e4`, except the parquet table-aggregate step; spec p99 limits not met | FAIL (accepted open by maintainer) |
| AC-007 steady-state reads skip the catalog | `distributed::tenant_scoped_cut_is_one_statement_and_one_bounded_retry`; `published::source_transitions_preserve_exact_rows` case 2; `catalog::cut_cache` tests | PASS |

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md), [Bifrost design](../../../architecture/bifrost-design.md), [OLAP serving](../../../architecture/references/domain/olap-serving.md).
- [Revision migration](../../../crates/vala/vala-sql/migrations/20261021000000_bifrost_publication_revision.sql) and [fenced publication](../../../crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs).
- Oracle [acquisition](../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs), [live routes](../../../crates/vala/vala-bifrost-redux/src/oracle/live.rs), [cut cache](../../../crates/vala/vala-bifrost-redux/src/catalog/cut_cache.rs) and [server tail discovery](../../../crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs).
- Scribe [captures](../../../crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs), [hot sources](../../../crates/vala/vala-bifrost-redux/src/scribe/hot_source.rs), [retained charge](../../../crates/vala/vala-bifrost-redux/src/scribe/memory.rs) and [persistence](../../../crates/vala/vala-bifrost-redux/src/scribe/persistence.rs).
- Journeys: [published](../../../crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs), [distributed](../../../crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs), [late group evidence](../../../sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs), [revision SQL](../../../crates/vala/vala-sql/tests/integration/pg_oracle_membership.rs).
- Capacity reports: [query](../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/report.rs) and [verification](../../../crates/wyrd/wyrd-testing/src/bin/capacity/verification/report.rs).
