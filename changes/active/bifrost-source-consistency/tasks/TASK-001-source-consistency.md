---
id: TASK-001
kind: implementation
status: in_progress
spec: SPEC-bifrost-source-consistency
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006]
depends_on: []
---

# Reliable Bifrost reads

[Specification revision 2](../spec.md) is approved and governs this task; its
validated cut cache replaces the per-query fresh cut read this task first
described. Review remediation TASK-001-R1 is in progress. There is one implementation task and no task dependency.
PR #189 is merged. Execute this same fix in a new worktree based on current
`origin/main` containing that merge. Delivered long-horizon work is the base,
not work to repeat or reapprove. Follow the spec's [worktree handoff](../spec.md#execution-base-and-worktree-handoff),
including transfer of the uncommitted packet/skill edits and preservation of
the separate test-only commit. The protocol, scope, targets and three scenarios
below are unchanged by the merge.

## Outcome and Value

Make an ordinary Bifrost query return the expected evidence once even while
Scribe moves it into files. Today that movement can lose or duplicate rows in
a query; a group Verifier can then omit its last evidence permanently. After
this task, Oracle chooses stable sources before execution and retries an
invalid selection before returning any rows.

For example, three acknowledged evidence rows must produce those three row
identities once each across publication. The same corrected service must make
the existing late-evidence group regression pass on the follow-up build. Finish by deleting the
superseded read path and measuring the final build against the agreed targets.

## What Changes and What Must Stay True

Oracle gains a validated selection phase. It reads the published file list and
publication revision, obtains retained live captures from every selected Scribe,
then validates before execution. Scribe makes its capture coherent across local
publication and memory rollover. A capture keeps its memory references and
staged-file leases until the query releases them.

Keep these rules visible during implementation:

- A row cannot disappear or be selected twice because its source changed.
- A conflicting selection returns no rows; a lost capture never opens a new one
  underneath an executing query.
- A capture is usable only by its tenant, table, query and current writer epoch.
  Failure, cancellation and expiry release its retained resources.
- Appends continue. Source selection may briefly delay rotation/publication,
  but there is no cluster-wide pause or extra per-append catalog request.
- Group scheduling and real missing-input checks stay intact. The fix belongs
  to Bifrost, not a caller-specific reread loop.

The material cost is additional work before the first result: fresh catalog
reads, live-source capture and retries when publication overlaps acquisition.
Meet verification server-overhead p99 ≤500 µs and selective-query client latency
p99 ≤1,000 µs under the spec's workload conditions, including that new cost.
No public query/SDK input, dependency, feature, second scanner, unavailable-writer
recovery or compatibility path is part of this task.

## Approach

1. Record the new worktree's merged-main base and any separately carried
   test-only commit, then recheck owners and commands there. Preserve comparable
   baseline capacity artifacts from that base. Use original evidence as history
   so completed work is not restarted.
2. Extend the catalog's existing publication and cut reads with durable
   revision evidence, including replay/recovery.
3. Make Scribe's existing capture safe across source changes and retain it
   under the query's existing authority, budget and lifetime.
4. Carry capture identity through authenticated peer assignments. Acquire,
   validate and then execute retained sources; retry selection only.
5. Delete the superseded read path and update current architecture/product
   explanations. Keep independent integrity checks and historical evidence.
6. Tighten existing benchmark verdicts, run the focused checks below, then run
   the final capacity sequence. Record evidence for independent review.

## Ordered Implementation Scenarios

### Scenario 1 — A source change does not change the query's rows

**Behavior.** A query sees each expected row identity once. These timing cases
exercise one coverage promise rather than create separate duplicate scenarios:

| Source change | Expected result |
| --- | --- |
| Publication finishes before the initial catalog read | The file supplies the rows; live capture does not supply another copy. |
| Publication occurs during acquisition, including removal of the final live partition | Validation rejects that acquisition; a replacement selection returns every expected row once. |
| Publication occurs after validation | Retained memory/files remain readable until the query finishes. |
| Writable rows freeze into staging during local capture | The rows appear once, not once from memory and again from staging. |

**RED.** Add the planned [exact-row journey](#exact-row-coverage) and local
[rollover proof](#rollover-and-retained-files). Run their focused commands and
confirm a missing/repeated identity under the relevant old behavior, rather
than a setup error. This is implementation TDD; no separate reproduction
investigation or approval gate is needed.

**GREEN.** Add the publication revision and coherent retained capture, then
validate the selection before execution. Use existing Arrow references, staged
leases and reader protection. Rerun both focused proofs. Check row identities
and counts; SQL deduplication must not conceal a repeated source.

**REFACTOR.** Replace the old selection path. Keep decoding, streaming and
filtering lazy where useful, while the selected raw sources remain fixed.
Keep these proofs green.

### Scenario 2 — Invalid or abandoned captures cannot become successful reads

**Behavior.** Protect the query's chosen sources and their lifetime. Each case
has its own expected observation:

| Case | Expected result |
| --- | --- |
| Publication revision or writer epoch changes during acquisition | Release that attempt and retry before output, under the original deadline. An explicit empty capture still participates in validation. |
| A publication may have committed but local advancement is unresolved | Reconcile before capture or refuse it; do not expose both or neither source as a valid choice. |
| A capture reference is foreign, substituted or expired | Refuse access before reading its data. |
| An accepted capture is lost during execution | Fail the dependent execution; never reopen live data or append another attempt's rows. |
| Cancellation, failed acquisition or deadline expiry ends the query | Stop work and release captures, leases and charged resources within existing lifecycle bounds. |

**RED.** Add the planned [acquisition failure and cleanup
journey](#acquisition-failure-and-cleanup), exercise these cases, and run its
focused command. Assert the expected refusal/retry/terminal and resource release,
not merely that an error occurred. Force publication uncertainty at its owning
subsystem if full-journey injection is materially harder; document the reason
and any cross-boundary state still proven by the journey.

**GREEN.** Finish capture authority, uncertainty reconciliation, bounded retry
and cleanup. Reuse existing typed failures, deadlines and cancellation. Rerun
this journey and scenario 1's proofs. No retry occurs after execution starts.

**REFACTOR.** Remove the old discovery-only/live-reopen branches and stale
cache authority. Update the [catalog regression](#catalog-acquisition-regression)
without weakening its authorization or metadata-error assertions. Remove a cache
only after checking whether another consumer still needs it.

### Scenario 3 — Capacity reports judge the promised limits accurately

**Behavior.** The report accepts direct verification overhead p99 at 500 µs
and selective query latency p99 at 1,000 µs. The next representable value or
histogram bucket above each limit fails. Existing throughput, sample, refusal,
correctness, memory and background-reporting criteria keep their meaning.

**RED.** Update the existing [capacity verdict tests](#capacity-verdict-boundaries)
and run their exact commands. Use precise query samples and the existing
verification histogram boundary. Confirm the old report incorrectly accepts a
value above the new limit or rejects equality.

**GREEN.** Change the existing report/target owners to enforce the inclusive
limits. Output enough precision to distinguish 1,000 from 1,001 µs. Update stale
help text and descriptions, preserving unrelated strict limits. Rerun these
report tests and the earlier scenario proofs.

**REFACTOR.** Remove obsolete old-limit expectations. Keep the workload and
its error checks intact. Report tests prove the judgment logic; final real
capacity runs must still prove actual performance.

## Acceptance Criteria

This is the single mapping from specification obligations to task evidence.
The expected observations remain explicit so IDs are only navigation aids.

| Spec obligations | Required observation and proof |
| --- | --- |
| AC-001; REQ-001/003; INV-003/004/006 | Publication and rollover leave every expected row identity readable once, including disappearance of the last live partition. Prove with the [row journey](#exact-row-coverage) and [local retention tests](#rollover-and-retained-files). Recovery shares atomic revision accounting. |
| AC-002; REQ-002/003; INV-002/005/006 | Each invalidation, authority and cleanup case above gives its stated result. Prove with the [capture journey](#acquisition-failure-and-cleanup) and owning uncertainty proof. No unvalidated rows or retained-resource leaks are accepted. |
| AC-003; REQ-004; INV-001 | The [late-evidence group regression](#late-group-evidence) includes the final source evidence in its verdict on this build. Account for each row of the [merged-workflow regression checklist](../spec.md#regression-checklist-from-merged-pr-189), with independent review of this fix and integrated client/contract proof. Old tasks do not need reapproval. |
| AC-004; REQ-005; INV-001 | Diff and docs checks show one read protocol, no scheduler workaround and no stale promise that publication races are acceptable. Preserve useful independent checks and historical records. |
| AC-005; REQ-006; INV-007 | [Report tests](#capacity-verdict-boundaries) accept the exact limits and reject the next measurable value, while throughput/error and background-reporting tests keep their assertions. |
| AC-006; REQ-004/006; INV-003/007 | The [final capacity runs](#required-final-capacity-sequence) meet the agreed latency and existing capacity criteria, with candidate identity and comparable artifacts recorded. Bifrost query capacity is last. |

## Implementation Reference

Use the existing owners below. Paths identify where to inspect and extend
behavior, not a private implementation allowlist. Engine paths are relative to
`crates/vala/vala-bifrost-redux/src/` unless another crate is named.

- **Published cut and revision.** `vala-sql`'s `OracleTableCuts::read` in
  `oracle_reader_authority.rs` and `catalog/bifrost_catalog.rs` already acquire
  a coherent published cut. Oracle acquisition and the [catalog regression](#catalog-acquisition-regression)
  consume it. Add durable revision evidence to that read and publication's SQL
  transaction; the notification cache has no authoritative commit revision.
- **Local publication and recovery.** `ClaimPublisher::publish`,
  `ScribePublicationReconciler`, `scribe/persistence.rs` and their
  `staging_runtime.rs` callers own commit, recovery and local advancement.
  Close their capture window and preserve unresolved outcomes here, rather
  than add another publisher. Existing [retained-file tests](#rollover-and-retained-files)
  prove a lease survives publication.
- **Live capture.** `FetchLiveTailService::open_live_batches`, `LiveTailBatches`,
  the memtable freeze path and `StagedSourceLease` already retain memory/files.
  Their follower/tail callers and [rollover tests](#rollover-and-retained-files)
  identify the gap: source selection can overlap freezing, and execution cannot
  address a retained capture. Extend those owners without a second scanner.
- **Oracle execution.** `Oracle::run_sql_attempt`, `classify_one_build`,
  `discover_live_routes` and `ScribeTailResolver::resolve` in the Oracle modules
  own acquisition, planning and follower reads. Replace lazy fresh acquisition
  with validated capture consumption; keep one physical build and execution.
  The [new journeys](#exact-row-coverage) and [published regression](#published-reads-and-shutdown)
  cover these consumers.
- **Peer authority.** `ScribeProviderCut`, `FollowerScanAssignment` and
  `oracle/peer.rs::assignment_authority_digest_for` bind follower work, but
  currently not to a retained capture. Extend those internal wire owners,
  private protobuf/server gRPC handlers and signing inputs so references cannot
  be substituted. Use the [capture authority cases](#acquisition-failure-and-cleanup).
- **Groups.** `GroupPoller`, `VerificationScheduler` and `VerifierExecutionQueue`
  already own scheduling and frozen inputs. The [existing group journey](#late-group-evidence)
  is the consumer regression. No new group machinery is justified.
- **Benchmark judgments.** Existing `query/workload.rs::Case::target`,
  `query/report.rs` and `verification/report.rs` under
  `crates/wyrd/wyrd-testing/src/bin/capacity/` own the targets and reports. Existing samples
  and the 500 µs metrics bucket suffice. Extend their [report tests](#capacity-verdict-boundaries)
  rather than add a benchmark harness.

The write set also includes Vala SQL migrations and normal SQL metadata,
`wyrd-spec` internal contracts and source-driven generated outputs, existing
Oracle/Scribe tests, and the Rust SDK group regression. Public SDK inputs do
not change. Update relevant `mise.toml` descriptions.

Update `architecture/bifrost-design.md`, `architecture/wyrd-design.md`, the
OLAP-serving reference and affected current Bifrost/evidence product docs.
Identify this spec's supersession in active-packet links. Preserve the old
approved specification and historical task/review evidence; do not silently
rewrite or delete that packet. Full first-class client/agent regression
coverage still belongs to integrated change review.

## Verification and Evidence

Run Cargo-backed checks sequentially. The row-coverage, acquisition-lifecycle
and first rollover selectors below are **planned** tests in inspected existing
package/module targets. Add them there and confirm their actual selectors
before RED. Existing names and setup are source-confirmed. No optional feature
change is required. Use tracing to diagnose failures.

Each command appears once below. The scenarios link to these descriptions so
the maintainer can understand the proof before reading its invocation.

### Separately carried test correction

If carrying `9a93d9310`, confirm its span-permit regression still passes on the
new base. It waits for asynchronous cleanup to return all permits with a
deadline; it changes no production behavior and adds no implementation scenario.

```bash
WYRD_LOG=info mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans)'
```

### Exact row coverage

Planned real-client/server journey. Compare the returned row identities with
the acknowledged inputs across the ordering cases in scenario 1. Every expected
identity appears once; missing or repeated identities fail.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::source_transitions_preserve_exact_rows)"'
```

### Acquisition failure and cleanup

Planned journey. Exercise the distinct cases in scenario 2 and check each stated
result, including no pre-validation output, capture authority and release of
retained resources. A fresh live read cannot substitute for a lost capture.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::unvalidated_captures_fail_before_rows_and_release_resources)"'
```

### Rollover and retained files

The first command is the planned local rollover test: selecting writable rows
while they freeze must not select the same rows from staging again. The second
runs existing staging/lease regressions: an opened read survives publication
without deleting its staged files early.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::active_rollover_during_capture_is_read_once)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::a_generation_staged_before_the_shard_settles_is_read_once) | test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication)'
```

### Catalog acquisition regression

Update the obsolete assertion that a warm notification cache permits zero SQL
for acquisition. Preserve this existing test's checks for a coherent tenant
cut, authorization before metadata access and bounded metadata-read errors.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::tenant_scoped_cut_is_one_statement_and_one_bounded_retry)"'
```

### Published reads and shutdown

Keep the existing real-client published-read, pruning and shutdown behavior
working with retained captures. Capture lifetime must not block shutdown
indefinitely or release files still being read.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::published_cache_pruning_and_shutdown_are_production_governed)"'
```

### Late group evidence

Run the existing group journey through the corrected Bifrost service. The final
source evidence must appear in the next group execution and affect its verdict.
This is regression proof, not a new scheduler feature or additional TDD scenario.

```bash
WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_executions_include_late_evidence)"'
```

### Capacity verdict boundaries

The direct-overhead test must accept the 500 µs histogram boundary and reject
the next higher bucket. The queued test preserves report-only background latency.
The query test must distinguish 999, 1,000 and 1,001 µs, retaining throughput and
error checks. These tests establish report correctness; only the actual final
benchmarks establish that the system meets the targets.

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::report::tests::direct_overhead_judges_p99_only)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::report::tests::queued_latency_is_reported_not_judged)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=query::report::tests::sweep_and_errors_are_judged)'
```

After scenario proofs, run the affected owner/static lanes:

```bash
mise run test:bifrost:unit:rust:inner
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:sql
mise run test:bifrost:integration:server
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run docs:check
mise run fmt
mise run lints
git diff --check
```

Use these lanes only after confirming their current task bodies; if the
implementation touches an additional owner, add only its missing bounded proof.
Documentation, generated outputs and deletion obligations use static/diff proof,
not manufactured RED tests. Full language/journey suites and the integrated
Bifrost gate run once at change review; do not repeat them as task aggregates.

### Required final capacity sequence

Capture the same workloads on the recorded merged-main implementation base
(plus any separately carried test-only commit) before changes; copy
their reports/raw samples/logs/profiles to a durable comparison location because
the harness clears its output directories. Record baseline and candidate commit
or exact dirty-diff identity, host fingerprint, dataset, tiers, concurrency,
build flags and commands. Old TASK-004 reports use older latency targets and
cannot replace this candidate's qualification.

After all changes and correctness/static checks, run sequentially:

```bash
mise run bench:bifrost:write-capacity
mise run bench:verification:capacity
mise run bench:bifrost:query-capacity
```

The last command is mandatory and is the final capacity run. Keep both query
tiers and both verification cases; no reduced fixture, hot-only run, shortened
window, lower concurrency or disabled profiling as the final proof. Preserve
the existing write capacity floor as well as selective peak ≥1,200 QPS.

Artifacts are under `target/bifrost-query-capacity/{writes/parquet,standard/hot,standard/parquet}/`
and `target/verification-capacity/`. Retain raw microsecond samples and exact
verification histogram evidence alongside reports/profiles; separate startup,
first-row and terminal timing when interpreting overhead. Record changed
acquisition retries/contention using existing telemetry owners with bounded
labels. The capacity commands are outside `verify:bifrost`; neither a unit test
nor a normal correctness gate substitutes for them.

If a final benchmark fails, diagnose using its traces/profiles, fix within the
approved boundaries, rerun affected correctness proof, then repeat the final
capacity sequence. Do not declare completion with failed targets or claim that
a prior successful build qualifies a changed implementation.

## Material Stop Conditions

- The approved coverage guarantee requires unavailable-writer recovery,
  global point-in-time semantics, a new durable evidence subsystem or a change
  in accepted group scheduling/outbox-loss behavior.
- Meeting the latency targets would require weakening correctness, excluding
  acquisition from measured query time, reducing the established workload or
  adding a public tuning/freshness selector. Report measured evidence; do not
  change the target or silently accept the old limits.
- A new public error/API, dependency, Cargo feature, destructive migration,
  compatibility protocol or material ownership/security change proves necessary.

Missing local repository-managed Postgres, migrations or profiler setup are
execution mechanics to provision, not reasons to stop before attempting them.
New-worktree creation, push, merging the follow-up and independent review
scheduling remain caller-owned. This task delivers the same Bifrost fix and
fresh correctness/capacity evidence; it does not reopen or close PR #189.

## Authority Links

- [Specification revision 2](../spec.md).
- [Original long-horizon specification revision 21](../../long-horizon-verification/spec.md), with this change's approved supersession limited to the named publication/performance obligations.
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Bifrost design](../../../../architecture/bifrost-design.md), [Wyrd design](../../../../architecture/wyrd-design.md).
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md), [implementation execution](../../../../architecture/references/languages/implementation-execution.md), [testing workflows](../../../../architecture/references/languages/testing-workflows.md).

## Implementation Evidence

Base: `1c5edf729` (merge of PR #189). Baseline capacity at `21f7f2f40`,
artifacts preserved under `../bsc-baseline/{write,verification,query}-artifacts`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 rows once across publication, incl. last live partition removed | revision 2: migration `20261021000000_bifrost_publication_revision.sql` (per-node revision trigger, `publication_revisions` in the cut, `scribe_node_publication_revisions`); `insert_artifact_set_fenced` returns the in-transaction revision → `advance_all_atomic` raises it with the `Published` move; `StagedCapture`/`LiveCapture`/`TailCapture` report it; `Oracle::list_live_routes` marks a cut stale when a report exceeds it, `build_physical_root` evicts and repins; boot load in `replay_wal_async`, missed-advance load in `finish_committed` | `published::source_transitions_preserve_exact_rows`; `scribe::tail_rpc::tests::active_rollover_during_capture_is_read_once`, `a_generation_staged_before_the_shard_settles_is_read_once`, `an_open_live_read_keeps_staged_runs_across_publication`; `pg_oracle_membership::file_list_publications_advance_their_node_revision` (per-node counts, replay and rollback advance nothing, tenant isolated) | PASS |
| AC-002 invalid/abandoned captures never succeed; resources released | capture id bound into `ScribeProviderCut` and the assignment-authority digest; Scribe-wide `RetainedCaptures`; `RegistryTailStreamDiscovery::admit/release`; lost capture fails (`live.rs` into_stream) | `published::unvalidated_captures_fail_before_rows_and_release_resources` (moved revision → retry, lost capture → Failed, cancel → released, failed listing → Degraded, zero retained after each); `tail_rpc::tests::a_capture_is_opened_only_by_its_query`, `an_unreleased_capture_expires_at_its_deadline`; uncertainty: `hot_source::tests::a_publishing_generation_refuses_capture_until_it_settles` (an unknown commit outcome cannot be forced through real Postgres from a journey); `distributed::live_query_terminal_failure_matrix`; `peer_network::*` (21 journeys) | PASS |
| AC-003 late group evidence in verdict | no group change; Bifrost fix only | `verify_in_real_time::grouped_executions_include_late_evidence` | PASS |
| AC-004 one read protocol, no stale race promise | revision 2 keeps the notification cut cache and Forge announcement for freshness only; pre-row degrade and `is_availability_loss` removed; docs updated in `bifrost-design.md`, `wyrd-design.md`, `olap-serving.md`, `analytical-operations-reliability.md`, product docs; supersession note on long-horizon spec | `distributed::tenant_scoped_cut_is_one_statement_and_one_bounded_retry`; `docs:check`; diff | PASS |
| AC-005 inclusive report limits | `verification/report.rs` (p99 bucket ≤ 0.0005 s), `query/workload.rs` `p99_at_most_ms`, `query/report.rs` (3-decimal output, strict limits kept elsewhere) | `verification::report::tests::direct_overhead_judges_p99_only` (RED: 0.9 ms bucket was accepted), `queued_latency_is_reported_not_judged`, `query::report::tests::sweep_and_errors_are_judged` (RED: 1,001 µs was accepted; 999/1,000 pass, strict p95 kept) | PASS |
| AC-007 steady-state reads skip the catalog | cached cut served without SQL; `BifrostCatalog::evict_cut` + `CutCache::evict` (generation bump drops a raced fill) | `distributed::tenant_scoped_cut_is_one_statement_and_one_bounded_retry` (warm one-table and join: 0 reads); `published::source_transitions_preserve_exact_rows` case 2 (after a warm zero-read query, a publication before capture costs exactly 1 cut read); `catalog::cut_cache` unit tests | PASS |
| AC-006 final capacity | sharded `CaptureTable` (16 locks, removed captures dropped outside the lock) | quiet-host sequence at `05186c1e4`, artifacts `../bsc-rev2`: at or above the `21f7f2f40` baseline on every step except parquet table-aggregate (56.9 vs 66.4 qps). Hot selective 1-client p50/p99 1.39/2.35 ms (baseline 1.6/3.7), peak 8,178 qps (baseline 6,821); parquet selective peak 7,411 qps (baseline 6,606). Verdicts still FAIL on the spec's strict p99 limits (selective ≤ 1 ms, direct-verify overhead ≤ 0.5 ms), which the baseline also missed | BASELINE MET; SPEC LIMITS OPEN |

Acquisition retries are counted on the query profile as `Oracle.selection_retries`
(`oracle_query_profile_count_total{event}`); a retry now reuses the query's one
profile instead of emitting a second.

Lanes: `fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`,
`test:bifrost:unit:rust:inner` (225 passed, under `with-test-postgres.sh`),
`test:bifrost:integration:{redux,sql,server}`, `docs:check`, `git diff --check` — all exit 0.
Span-permit regression `scribe_outbox::tests::sustained_scribe_outage_caps_tenant_spans` passed.

Final task verification at `051be28a3`: every command in Verification and
Evidence above except the capacity sequence (run at `05186c1e4`, code
unchanged since) — the span-permit test, the five named journeys, the
rollover and staging tests, the three report tests, `test:bifrost:unit:rust:inner`,
`test:bifrost:integration:{redux,sql,server}`, `codegen:check`, `check:deps`,
`check:tenant-isolation`, `docs:check`, `fmt`, `lints` and `git diff --check`
— all exit 0.

### Diagnoses
- **Symptom (rev 2):** six `staging_runtime` recovery tests failed "failed to lookup address information". **Cause:** the revision load ran inside `ScribeStagingRuntime::restore` and on every `finish_committed`, and those tests recover with an unconnected lazy pool. **Fix site:** boot load moved to `replay_wal_async` beside the other operator-backed recovery; `finish_committed` reads only when a member never advanced to `Published`.
- **Symptom (rev 2):** `file_list_publications_advance_their_node_revision` hit `file_list_stream_range_ordinal_uniq`. **Cause:** both test republishes used the same WAL range. **Fix site:** the test binds distinct offsets.
- **Symptom (rev 2):** case 2 of `source_transitions_preserve_exact_rows` saw 2 cut reads. **Evidence:** Forge `scribe_promotion` commit notices between case 1 and case 2 in the trace. **Cause:** Forge promotion of case 1's files moved the cut, so the held query's first pin was a genuine miss. **Fix site:** the journey waits (bounded) until a query is served with zero cut reads before holding.

- **Symptom:** `unvalidated_captures…` failed "cancelled read: the Scribe producer never paused". **Evidence:** `observe_live_production_for_test` holds only before a second batch (`tail_rpc.rs`). **Cause:** only one live row (one batch) existed. **Fix site:** the new journey's fixture appends a second live row; no shared owner changed.
- **Symptom:** follower tests failed "Scribe live-tail snapshot failed" then `TailReadError::Binding`. **Cause:** a nil capture id, then a `vala.`-prefixed wire namespace that `binding_from_wire` rejects. **Fix site:** test helper `reading_capture` takes a real capture with the stripped namespace.
- **Symptom:** a capture listed through one `tail_service()` could not be opened through another. **Cause:** each `tail_service()` built its own capture table. **Fix site:** one `RetainedCaptures` owned by `ScribeImpl`, shared by every instance.
- **Symptom:** unit lane mass-failed "test DB env unset". **Cause:** `:inner` lane run without the Postgres wrapper. **Fix:** rerun under `with-test-postgres.sh`; no code change.

Non-goals held: no public query/SDK input, dependency, feature, second scanner,
unavailable-writer recovery or compatibility path was added.
