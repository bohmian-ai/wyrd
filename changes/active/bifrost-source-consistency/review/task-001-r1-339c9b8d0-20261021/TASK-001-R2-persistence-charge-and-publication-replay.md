---
id: TASK-001-R2
kind: remediation
status: review
spec: SPEC-bifrost-source-consistency
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-4, FIND-TASK-001-6]
requirements: [REQ-003, INV-004, INV-006, AC-002]
depends_on: []
---

# Preserve persistence memory ownership and exact older publication replay

Execution skill: `$wyrd-implement`. Implement the bounded corrections independently validated in [findings-validation.md](findings-validation.md), under the existing approved behavior.

## Subject and outcome

- Approved spec: `changes/active/bifrost-source-consistency/spec.md`, revision 2.
- Original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`.
- Original base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Current reviewed candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`.
- Prior candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Current verdict: [verdict.md](verdict.md), FIX_REQUIRED.
- Prior verdict/ledger/R1 task: `changes/active/bifrost-source-consistency/review/task-001-0d3223696-20261021/`.

Keep memory charged while persistence still holds its original Arrow buffers, and let a valid publication committed before revision tracking finish its existing restart-recovery protocol. For example, retiring a memtable while its persistence payload remains alive must not free those bytes for new admission; replaying an older committed file set must succeed without inventing a new publication revision.

This task continues FIND-1, batches the remaining changed-item rustdoc gate as FIND-4, and fixes newly confirmed FIND-6. Prior FIND-2, -3 and -5 are closed and must remain closed. This is not another architecture design or performance task. REQ-006 / AC-006 remains independently failed/unqualified after this bounded correction until the original final qualification succeeds.

## Diagnosis and selected correction

### FIND-TASK-001-1 — Persistence omits the shared Arrow owner

R1 correctly carries the shared `RetainedArrowCharge` through active/frozen/capture/read/follower memory-source/stream owners. Retirement detaches the existing immutable lease without releasing root bytes and deposits it in that shared owner. But `scribe/persistence.rs:485–583` defines `ImmutableGeneration` without the charge: `from_frozen` shallow-copies `frozen.batches` at line 557, and `frozen_snapshot` copies those buffers with an unrelated empty charge at line 581.

All three production producers use this conversion: replay preparation, selective sealing and full rotation (`scribe/shards.rs:1641,2150,3403`). Pending fronts, persistence jobs and worker Arcs retain it through staging and awaited visibility acknowledgment. Owned replay completion invokes retirement while its local front and the worker still hold those rows (`shards.rs:2696–2702`; `persistence.rs:1998,2096`). Normal completion can also be followed by retirement before the awakened worker drops the payload. Once no capture/frozen holder remains, removing the memtable returns the charge despite those physical persistence holders. Root admission can reuse memory still resident in the shared process.

The passing R1 retirement proof never constructs the real `ImmutableGeneration`; it cannot detect either missing conversion edge. Dropping frozen clones in the replay-identity test removes genuine extra owners, but its `owner_generation` fixture is a literal copying the same rows without their charge. Its still-live payloads therefore mask the same omission.

**Selected correction:** extend the existing `ImmutableGeneration` owner to retain the same shared charge from `from_frozen` and preserve that same owner in `frozen_snapshot`. Continue using the existing net-zero split/deposit/lease Drop. CPU staging clones then inherit it automatically. Fresh independently allocated fixtures may create fresh owners; fixtures copying existing frozen buffers, notably `shards::owner_generation`, must share their original owner. Trace all constructors by provenance so no equivalent shallow-copy bypass remains.

Keep rollback, pending/failed-stage ownership, visibility acknowledgments and WAL/publication progress unchanged. Pre-readiness rollback disposes unsubmitted reconstructed members synchronously; it does not need a new committed-retirement policy. Do not add another reservation, per-query charge, governor, worker barrier or wait for queries before retiring durable work. Fix the shared conversion once rather than patching completion consumers.

**Focused proof:** retain an actual production-converted persistence payload and its reconstructed frozen snapshot through existing charged retirement. Drop original frozen/capture/read holders and non-final payloads: root bytes remain exactly charged and growth into them is refused. Drop the last payload while its snapshot remains: still charged, rows still readable. Drop the final snapshot: exact single return. Preserve current overlapping-capture, projected-read and ordinary WAL-progress assertions. This proves both conversion edges without scheduling stress or a new harness. Make replay fixture settlement assertions reflect genuine final physical holders while preserving response/error/identity/poison checks; do not preserve an uncharged fixture to keep a zero-byte assertion green.

### FIND-TASK-001-6 — Valid pre-revision replay keeps Scribe unready

The publication-revision migration creates an empty revision table and installs an AFTER INSERT trigger, without rows for publications already present in `vala.file_list`. An exact replay inserts zero and fires no insert trigger. A pre-upgrade process can commit its artifact set and crash before removing the durable publication manifest; that is state the existing base recovery protocol handles.

At startup `ScribeImpl::replay_wal_async` invokes staged publication recovery before readiness. `ScribeStageMover::recover_publication` verifies committed remote bytes, then still calls the reconciler's publish for the exact artifact rows. `insert_artifact_set_fenced` in `scribe/file_list_writer.rs:470` validates the active actor fence, set structure, insertion count and every ordered existing artifact identity. It then unconditionally requires a revision row at lines 553–563. For valid older state no row exists, so recovery returns an error and Scribe stays unready; repeated restart repeats it and admission cannot create a new publication to repair it. Files remain preserved and sibling roles can serve, but Scribe's recovery/ingest availability regresses.

This is a valid persisted-state upgrade, not a request for mixed-version peers or unavailable-writer recovery. Existing cut and capture semantics already interpret an absent node/table revision as 0; exact replay creates no new publication.

**Selected correction:** at the existing fenced artifact-set SQL owner, allow missing revision to mean 0 **only after a complete, identity-validated, zero-insert exact replay**. Return an existing revision unchanged. A fresh insert must still require the revision row its trigger created; contradictory/partial sets, stale actor fences, malformed input and actual SQL errors still fail. Preserve the transaction, actor validation, identity locks, remote verification, manifest replay and cleanup behavior.

Do not broadly convert SQL errors or fresh-insert missing revisions to 0, fabricate positive revisions, delete/reinsert file-list rows, skip recovery, add a second revision reader/boot exception, or introduce a backfill/recovery subsystem. This uses the approved revision-0 initial state, without a new persistent-data or rollout decision. Ordinary claim publication, uncertain retry and durable manifest recovery share the same owner and must receive the same checked behavior.

**Focused proof:** using current Postgres-backed persistence/staging fixtures, model the equivalent migrated initial state: exact committed artifact rows, a surviving publication manifest and no revision row for their producer/table. Drive actual recovery/reconciler through the real SQL owner. It must settle recovery/cleanup, preserve rows once, leave the revision absent/logically 0, then create revision 1 on the next fresh publication. Keep exact replay after a revision exists unchanged, and retain conflicting/partial replay, rollback, tenant and stale-actor refusal proof. Testing only an absent-row helper or only inserting after the trigger exists does not prove the gap. No new harness or full SDK journey is needed for this seam.

### FIND-TASK-001-4 — New cleanup-proof module lacks rustdoc

The original five method contracts are corrected. The cumulative gate is still open because newly added `crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs:440–441` test module has neither outer nor inner module rustdoc, which actual AGENTS §16 explicitly requires even for test modules.

Add substantive module documentation for its retained-capture cleanup/deadline proof and real mTLS stalled-peer fixture. Preserve its code and assertions. Inspect the completed docs against the module and run the already-required format/lint checks; no behavioral test is justified solely by this prose. Batch it with the runtime work above.

## Constraints and preserved behavior

- Keep the corrected query-facing shared charge, capture registration/expiry/open checks and original-deadline cleanup intact; do not regress FIND-2, -3 or -5.
- Continue one Oracle/Scribe/Vala SQL read/publication protocol, notification caching with in-memory revision validation, no steady-state query SQL, no per-append catalog request and no global/remote append barrier.
- Preserve exact rows, tenant/RLS and peer authority, artifact identity/fences, transaction atomicity, rollback, non-blocking audit, staged-file leases and Forge/hot-Iceberg reconciliation.
- Preserve existing public APIs, errors, SDK inputs, accepted outbox/unavailable-writer limits, group scheduling and current corrected documentation.
- Expected owners: Redux `scribe/{persistence,file_list_writer,shards}` and their existing fixtures/tests, plus local server test-module rustdoc; touch other immediate owners only as earned by real propagation. No dependency, public schema, migration edit, docs-site change or new configuration is prescribed.
- Do not relax performance targets or treat the earlier failing benchmark as final evidence for new runtime code. Performance investigation and final qualification remain original-task work outside this bounded packet.

## Acceptance mapped to retained findings

| Finding | Observable acceptance |
| --- | --- |
| FIND-1 | Every existing-buffer conversion shares its original charge; real persistence payload and reconstructed snapshot can each be the final physical holder, retaining exact root bytes and pressure refusal until their last drop. Normal/replay/WAL/rollback/identity behavior remains correct. |
| FIND-6 | Real older committed-manifest recovery through SQL completes once with logical revision 0, next fresh publication gets 1, later exact replay preserves its existing revision, and all invalid replay/fence/tenant/rollback branches remain refusals. Fresh publication cannot silently invent missing revision authority. |
| FIND-4 | New test module has substantive rustdoc; original corrected method contracts remain accurate, without runtime changes. |

## Verification

Use `$wyrd-implement`'s Red-Green discipline for the two executable gaps. New test names and local fixture layout are implementation-owned; confirm each actual package/target/selector and record its exact `mise exec -- cargo nextest run --locked ... -E 'test(=<actual-name>)'` command, intended RED failure, GREEN and final result. Do not claim planned selectors already exist. Diagnose failures from tracing first. Run Cargo-backed work sequentially.

Source-confirmed adjacent accounting/replay regressions:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops) | test(=scribe::shards::tests::replay_identity_failures_settle_response_and_release_ownership) | test(=scribe::shards::tests::replay_retirement_settles_response_and_releases_ownership) | test(=scribe::shards::tests::replay_insert_adoption_failure_rolls_back_identity_and_immutable) | test(=scribe::shards::tests::immutable_release_failure_prevents_false_retirement)'"
```

Source-confirmed publication/recovery regressions (the existing manifest proof is not a substitute for the new real-SQL gap proof):

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::file_list_writer::tests::artifact_set_publication_classifies_every_row_state) | test(=scribe::file_list_writer::tests::deterministic_artifact_rows_are_retry_stable_and_artifact_local) | test(=scribe::staging::tests::publication_manifest_recovers_rows_and_source_fence_fail_closed)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_oracle_membership::pg_tests::file_list_publications_advance_their_node_revision)'"
```

After exact new proof and adjacent regressions pass, use narrow supporting owner coverage for the actual write set:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::persistence::tests::/) | test(/^scribe::file_list_writer::tests::/) | test(/^scribe::shards::tests::/)'"
mise run fmt
mise run lints
mise run check:tenant-isolation
git diff --check
```

Add only proof earned by any additional touched owner. No whole journey suite, broad Bifrost aggregate, docs-site rebuild, dependency/codegen lane or capacity sequence is prescribed for this bounded write set. Run their own gates if actual implementation changes those boundaries. Integrated change review still owns its full capability/first-class surface proof, and original final capacity qualification remains mandatory separately.

## Evidence and handoff

Record actual candidate identities, diagnosis, selected shared boundaries, all exact new selectors/commands and results, adjacent regression/static coverage and remaining limits. Preserve both prior review ledgers and original/R1 history. This task closes only FIND-1, -4 and -6; later cumulative review must also assess the original specification and still-open REQ-006 / AC-006. No benchmark waiver, implementation during review, commit, push or merge is granted by this artifact.

## Implementation evidence

Candidates on top of reviewed `339c9b8d0`: `13bbb484e` (FIND-1), `b3863abc1` (FIND-6), and the FIND-4 / formatting / evidence commit that records this section.

### Reuse map

| Need | Existing owner | Gap | Extension |
| --- | --- | --- | --- |
| Persistence keeps Arrow charged | `RetainedArrowCharge` shared by frozen/capture/read owners (`scribe/memory.rs`) | `ImmutableGeneration::from_frozen` and `frozen_snapshot` dropped the owner (`persistence.rs`) | `ImmutableGeneration.charge` clones the frozen `Arc`; the snapshot clones it back. All three producers (`shards.rs` replay, selective seal, rotation) and CPU staging clones inherit it. The `shards::owner_generation` fixture copies frozen buffers, so it now shares their owner; `fixture_generation` allocates its own rows and keeps a fresh owner. |
| Pre-revision exact replay | `insert_artifact_set_fenced` (`scribe/file_list_writer.rs`) | Required a revision row after a zero-insert replay | Reads the row with `fetch_optional`. A missing row after a validated zero-insert replay is revision 0; a missing row after a fresh insert stays an invariant violation. Recovery, claim publication and retry all reach this through `ScribePublicationReconciler::publish`. |

### Diagnosis — `replay_identity_failures_settle_response_and_release_ownership` after FIND-1

- **Symptom:** `shards.rs` `settle_poisoned_advance` baseline assertion: left 144, right 0.
- **Evidence:** run with `WYRD_LOG=debug,vala_bifrost_redux=trace`. The lib binary installs no subscriber (the gap already recorded in R1), so the cause was read from source. Completion retires `current` through `release_committed_generation_memory` → `detach_immutable` → `commit_retirement` → `charge.hold`. The helper borrowed `current` from the test, which still held it at the assertion.
- **Cause:** `owner_generation` now shares the frozen charge (the intended fix), so the test's own `current` `Arc` is a genuine final physical holder of a retired generation. `next` is rolled back through `release_immutable` and never holds a lease.
- **Fix site:** test fixture only. `settle_poisoned_advance` takes `current` by value, asserts `baseline + current.arrow_bytes` while held, then drops `current` and `next_for_poison` and asserts the baseline. The response/error/identity/poison checks are unchanged.
- **Fresh read-only diagnostician:** independently found the same cause, test-fixture fix site and no production defect. It checked `settle_failed_terminal_identity_return`, `settle_poisoned_identity_lock`, `replay_retirement_settles_response_and_releases_ownership`, `replay_owns_retries_across_queue_backpressure_and_shutdown`, `flush_all_resubmits_a_retained_generation` and `accepted_batches_are_serialized_then_inserted_without_regeneration`: none retire and then assert a root baseline while holding a charged owner.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1: every existing-buffer conversion shares its original charge; the real payload and its snapshot can each be the final holder, keeping exact bytes and pressure refusal until the last drop | `persistence.rs` `ImmutableGeneration::{charge, from_frozen, frozen_snapshot}`; `shards.rs` `owner_generation` | New `scribe::shards::tests::a_retired_generation_stays_charged_while_persistence_holds_it`. RED: left 0, right 144 at "persistence holders keep the charge through retirement". GREEN: pass. | PASS |
| FIND-1: normal/replay/WAL/rollback/identity behavior preserved | Fixture holder settlement in `settle_poisoned_advance` | 6/6 accounting and replay regressions (command 2) | PASS |
| FIND-6: real committed-manifest recovery of a pre-revision set completes once at logical revision 0; the next fresh publication gets 1; exact replay keeps it | `file_list_writer.rs` `insert_artifact_set_fenced` | New `scribe::persistence::tests::a_publication_committed_before_revision_tracking_recovers_at_revision_zero`, which drives `ScribeStageMover::recover_publications` with a real fenced reconciler on Postgres. RED: `pre-revision replay recovers: Internal { detail: "no rows returned" }`. GREEN: pass, asserting one row, no revision row, the cleaned manifest, revision 1 after the fresh set, and 1 after its replay. | PASS |
| FIND-6: invalid replay, fence, tenant and rollback branches still refuse; a fresh insert cannot invent a missing revision | Structure, fence, partial-set and identity checks unchanged before the read; `(None, inserted > 0)` is an invariant violation | Publication regressions (command 3), `file_list_publications_advance_their_node_revision` (command 4), owner suites, `check:tenant-isolation` | PASS |
| FIND-4: the new test module has substantive rustdoc; the corrected method contracts stay accurate | `wyrd-server/src/oracle/tail_discovery.rs` test-module docs; no runtime change | `a_stalled_release_settles_by_the_query_deadline`, fmt, lints | PASS |

### Commands (all exit 0, run sequentially)

1. `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::a_retired_generation_stays_charged_while_persistence_holds_it)'"` — the RED and GREEN runs did not use the wrapper; this test needs no Postgres. 1/1.
2. The task's accounting/replay command plus selector 1 — 6/6.
3. `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::file_list_writer::tests::artifact_set_publication_classifies_every_row_state) | test(=scribe::file_list_writer::tests::deterministic_artifact_rows_are_retry_stable_and_artifact_local) | test(=scribe::staging::tests::publication_manifest_recovers_rows_and_source_fence_fail_closed)'` — 3/3.
4. `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_oracle_membership::pg_tests::file_list_publications_advance_their_node_revision)'"` — 1/1.
5. `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::a_publication_committed_before_revision_tracking_recovers_at_revision_zero)'"` — 1/1.
6. Owner coverage: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::persistence::tests::/) | test(/^scribe::file_list_writer::tests::/) | test(/^scribe::shards::tests::/)'"` — 68/68.
7. `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::tail_discovery::tests::a_stalled_release_settles_by_the_query_deadline)'` — 1/1.
8. `mise run fmt` (reformatted only the new test and one error literal), `mise run lints`, `mise run check:tenant-isolation`, `git diff --check`.

### Scope and limits

- Non-goals are excluded: no new reservation, governor, barrier, configuration, dependency, public API, migration edit, docs-site change or backfill. Prior FIND-2, -3 and -5 code is untouched.
- Write set: `scribe/{persistence,file_list_writer,shards}.rs`, server `oracle/tail_discovery.rs` (docs only) and this review packet. No unrelated file changed.
- `frozen_snapshot` widened from private to `pub(crate)` so the shards retirement test can build the staging snapshot exactly as persistence does.
- The pre-revision state is modeled by deleting the revision row the first publication's trigger created. That is the row set the migration leaves for an earlier commit; it does not run the migration against old data.
- REQ-006 / AC-006 performance remains failed/unqualified and outside this packet. No journey suite, broad aggregate or capacity run was executed.
