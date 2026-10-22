# Independent structured Ponytail validation — TASK-001 R1

Recommended verdict: **FIX_REQUIRED**. Retain **FIND-TASK-001-1**, **FIND-TASK-001-4**, and new **FIND-TASK-001-6**. Original **REQ-006 / AC-006 remains FAIL**, independently of these bounded corrections. No source-backed performance diagnosis supplies a decision-complete optimization finding, and no evidence requires changing approved behavior.

## Immutable subject and method

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`.
- Prior reviewed candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Authority: approved revision-2 `changes/active/bifrost-source-consistency/spec.md`; original `tasks/TASK-001-source-consistency.md`; prior verdict/validation and R1 task/evidence. Repository AGENTS, agent rules, task-review skill, spec-driven-development, maintainer-style and applicable Bifrost source/publication, memory, recovery, tenant and deadline contracts govern interpretation. Revision 2 supersedes obsolete revision-1 private task mechanics.

I am a fresh validator, independent of implementation, discovery and follow-up. Inputs include all eight current discovery reports, claims comparison, focused follow-up and verification report. I inspected the actual cumulative range and current source, using the latest remediation range as a locator, not as the acceptance boundary. CodeGraph preceded source searches. The inspected ownership graph includes production constructors, payload/snapshot/CPU staging, ordinary and owned replay completion, rollback, publication/reconciliation and boot, capture authority, route cleanup, follower/source rewrites, cut/revision SQL, tests and current contract guidance. Historical packets and carried draft specifications do not become product authority.

HEAD was source-confirmed as the candidate and tracked source remained unchanged; the current review directory was untracked. Only this report was written. This is static validation: no Cargo, test, benchmark, commit or additional agent execution. Recorded executions are assessed for coverage, not represented as executions by this validator.

## Disposition of every proposal and disputed closure

| Discovery/follow-up claim | Disposition | Validated result |
| --- | --- | --- |
| BEH-R1-001 | CONFIRMED | Residual FIND-TASK-001-1, production persistence loses the shared charge. |
| INV-R1-01 | CONFIRMED | Same cause and correction boundary; no separate caller finding. |
| M-R1-001 | CONFIRMED | Same FIND-1; the payload conversion is an actual physical holder. |
| CONC-R1-001 | CONFIRMED | Same FIND-1; owned replay provides a direct synchronous reachable failure. |
| DUR-R1-001 | REVISED | Same FIND-1, with fixture provenance corrected: `owner_generation` is a literal, not a call to `from_frozen`. Its cloned buffers need the same owner. |
| FOLLOWUP-R1-001 | CONFIRMED | Resolves the scoped closure conflict by identifying the omitted persistence edge, not by voting. |
| Standards' FIND-1 complete-closure assertion | REVISED | Its capture/read/source evidence is correct, but does not cover `ImmutableGeneration`. Complete closure is false. |
| System's FIND-1 capture/read closure | REVISED | Closed for those named edges only; task-wide allocation lifetime remains open. No claim of measured OOM or row loss is accepted. |
| REPO-R1-001 | CONFIRMED | Missing module rustdoc is the explicit AGENTS §16 exception to the material-consequence threshold. Retain prior documentation-gate ID FIND-TASK-001-4; original five sites themselves are closed. |
| SYSTEM-R1-001 | CONFIRMED | New FIND-TASK-001-6. Pre-revision committed artifact rows and a retained manifest reach an unconditional revision `fetch_one` without a revision row. |
| All reports' prior FIND-2 closure | CONFIRMED | Ordered installation and owner-local deadline enforcement close both original paths. |
| All reports' prior FIND-3 closure | CONFIRMED | Failed acquisition and final-route Drop use the original deadline at the shared release owner. |
| All reports' prior FIND-4 five-site closure | CONFIRMED | Those five method contracts now match their bodies. The newly omitted module keeps the cumulative documentation gate open. |
| All reports' prior FIND-5 closure | CONFIRMED | Cited current text agrees with listing-only degradation and fatal accepted-capture loss. |
| Failed/unqualified original capacity claims | CONFIRMED as failed acceptance | No numbered optimization FIND: the retained measurements establish failure, not its corrective source change. |
| Security/tenancy report's empty proposal set | CONFIRMED | Query/table/epoch/deadline checks, checked peer context/digest, mTLS and RLS remain present; no additional security machinery is justified. |

No proposal is rejected merely because it is unique. No human-directed addition receives a retrospective FIND ID. Placement/import preferences, unrelated pre-existing raw SQL-capability vocabulary discrepancies and disclosed draft packets remain outside the actionable ledger: no new required behavioral correction was established for them.

## Final deduplicated actionable ledger

### FIND-TASK-001-1 — Persistence still holds original Arrow without its shared charge

**Status:** CONFIRMED, with DUR fixture wording revised above. **Classification:** INCORRECT. **Sources:** BEH-R1-001, INV-R1-01, M-R1-001, CONC-R1-001, DUR-R1-001, FOLLOWUP-R1-001. **Obligation:** INV-006 / AC-002 and R1's already-selected final-physical-holder accounting outcome.

**Source boundary:** `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:485` defines `ImmutableGeneration` with rows but no shared charge. Full `from_frozen` at 528 clones `frozen.batches` at 557. Full `frozen_snapshot` at 568 clones `self.rows` and assigns unrelated `Arc::default()` at 581. Neither operation reconstructs those Arrow buffers. Both retain the original allocations while discarding their accounting-owner relationship.

**Reachability, producers and sinks:**

1. Bucket creation, freeze and `FrozenMemtable` clones now preserve one charge slot. Normal selective sealing (`shards.rs:2150`), cohort rotation (`:3403`) and replay preparation (`:1641`) all convert through `ImmutableGeneration::from_frozen`. There is no alternate production persistence conversion that already solves the omission.
2. Pending FIFO and `PersistenceJob` retain that converted payload. `PersistenceWorker::process_job` at `persistence.rs:1998` clones its Arc, awaits staging, and awaits `deliver_completion`. The latter at 2096 retains the job while awaiting the owning shard's visibility acknowledgment. The generation also creates a snapshot in `persist_once:2177`; `stage_member:2199` clones that snapshot into `StageMemberOp`. Frozen clones would automatically preserve a correct shared charge, but this snapshot starts with an unrelated empty one.
3. `handle_persistence_completion` removes the pending front, then calls `complete_owned_replay_generation(&front, ...)`. `complete_replay_generation:2696` calls `release_committed_generation_memory` at 2702 while `front.generation` remains alive, subsequently uses its stream/key/LSN and advances the next replay member. The worker also still awaits acknowledgment. This is an actual production path; the retained single-generation sibling corroborates it but is unnecessary to prove reachability.
4. Retirement (`shards.rs:2381`) splits existing ledger bytes through `detach_immutable`; `Memtable::commit_retirement:1142` installs the lease into the original frozen charge and removes the frozen entry. With no query/frozen holder left, that slot drops and the root returns its bytes despite the live persistence payload's original rows. Normal retirement also permits this between acknowledgment send and the worker being scheduled to drop its payload.

**Sibling ownership assessment:** capture → projected read → live batches → follower memory source, source clone/pruning/fetch/projection and opened stream carry the charge correctly. Disk staged leases and sorted/encoding reservations govern different resources. The only other immutable-release writer, `rollback_replayed_generation:1938`, is used for unsubmitted/pre-readiness reconstructed members; `discard_prepared_replay`, `rollback_prepared_replay`, failed chunk and failed submission paths dispose those values synchronously. Failed submitted fronts remain pending. Preserve that existing checked rollback/identity settlement; do not force it through committed retirement or invent a separate rollback finding.

**Consequence:** the root advertises reusable memory while an actual persistence/replay holder still owns the allocations. Another write/shard/replay member can reserve that capacity. This proves premature accounting release; it does not prove row loss, an unbounded leak or a measured OOM.

**Decision-complete minimum correction:** extend the existing `ImmutableGeneration` payload to retain the **same** `Arc<RetainedArrowCharge>` from `from_frozen`, and propagate that same owner through `frozen_snapshot`. Keep the current split/deposit/lease Drop. Existing CPU frozen clones then inherit it. Correct literal fixture constructors by provenance: fresh independent fixture allocations may start with a fresh slot; `shards::owner_generation:6774` copies an existing frozen generation and must share its owner. Existing constructors/callers benefit at this one conversion boundary.

Do not add another reservation/governor, charge each query, add completion barriers, change rollback policy, or delay WAL/publication/visibility progress. These alternatives either duplicate the existing owner or alter adjacent behavior. This correction is an omitted edge in approved resource ownership, not a new resource-policy decision.

**Focused closure proof:** use existing retirement/resource and persistence fixtures to hold an actual production-converted payload **and its reconstructed frozen snapshot** across retirement. Drop original frozen/capture/read holders, then non-final payloads: root bytes must remain exactly charged and growth into them refused. Drop the final payload while its snapshot remains: still charged and rows readable. Drop the final snapshot: exact one-time return. This detects independently fixing only one of the two conversions, without a scheduler race or new harness. Retain the current active/frozen overlapping-capture and projected-reader assertions, and prove ordinary WAL retirement still progresses.

The current new retirement test never constructs `ImmutableGeneration`; its green result cannot refute the omission. The adjusted replay-identity test drops frozen clones but leaves `current` alive through settlement assertions. After its fixture is faithful, release genuinely finished row holders or account for their retained bytes at the intended assertion boundary. Preserve identity poison/error/response assertions; do not keep a fixture-only accounting bypass or weaken expected ownership to obtain green.

### FIND-TASK-001-4 — Cumulative changed-item rustdoc gate remains incomplete

**Status:** REVISED residual gate; original five sites CLOSED. **Classification:** VIOLATION. **Source:** REPO-R1-001. **Obligation:** actual AGENTS §16 expressly requires substantive rustdoc on modules, including test modules, and makes omission blocking. The task-review skill preserves that exception.

`crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs:440–441` adds `#[cfg(test)] mod tests` with neither outer `///` nor inner `//!`. The following imports begin its body; individually documented `StalledRelease`, RPC methods, helper and test do not document the module.

Keep ID 4 because it is the same mandatory changed-item documentation completion obligation in the cumulative remediation; do not claim the original five methods are still defective. A new location within that same documentation gate does not warrant counting another runtime cause. New ID 6 below is reserved for the genuinely new recovery defect.

**Minimum correction/proof:** add substantive module rustdoc explaining its retained-capture deadline proof and real mTLS stalled-peer fixture. Preserve behavior. Source inspection and already-required format/lint checks suffice; no new behavioral test or docs-site regeneration is earned by this local rustdoc addition. Batch it with the runtime correction, never create a style-only remediation round.

### FIND-TASK-001-6 — Exact recovery replay of pre-revision committed rows cannot finish

**Status:** CONFIRMED. **Classification:** REGRESSION. **Source:** SYSTEM-R1-001. **Obligation:** REQ-003 recovery and preserved exact publication replay, INV-004 replay does not invent a revision, retained-data preservation and the already-defined absent-node revision 0.

**Producer:** migration `crates/vala/vala-sql/migrations/20261021000000_bifrost_publication_revision.sql:22–57` creates an empty revision table and an AFTER INSERT trigger; it does not backfill existing `vala.file_list`. Conflict-no-op replay inserts no row and fires no insert trigger. Existing base-created rows therefore legitimately have no revision. A base process may commit its file-list set and crash before manifest cleanup: the base itself already implements this recovery protocol. The prohibition on mixed private protocol versions does not authorize deleting or refusing existing retained analytical data, and this path requires no mixed-version peers.

**Full consumer path:** `ScribeImpl::replay_wal_async` at `scribe/mod.rs:2495` restores staging and calls `recover_staged_publications` before loading revisions/resuming claims/readiness. `ScribeStageMover::recover_publications` reads durable manifests. Full `recover_publication` at `persistence.rs:1666` classifies the artifact set; a committed set still verifies its remote bytes, then calls `reconciler.publish(&publication.rows)`. The reconciler invokes `insert_artifact_set_fenced` at `file_list_writer.rs:470`. Its full body checks active replacement actor fence, validates complete set structure, inserts with conflict-no-op, refuses partial insertion, locks/compares every ordered existing identity, and only then at **553–563** unconditionally `fetch_one`s the revision. An exact old set inserts zero, has no revision row, and returns NoRows. Reconciliation reports unknown outcome, recovery propagates the error, and boot (`wyrd-server/src/boot/mod.rs:694–709`) leaves Scribe unready with files preserved. It cannot admit a fresh publication to repair itself; repeated restart reaches the same failure. Sibling roles continue, so this is Scribe-role availability loss, not a claimed whole-process crash.

**Other writers/callers and existing mechanism:** ordinary claim publication, uncertain-commit exact retry and durable-manifest recovery share this fenced SQL owner. All fresh file-list inserts are covered by the trigger, whose increments roll back with the insert. Cut grouping and `capture_staged` already treat absent revisions as 0, and registry advancement takes a monotonic maximum. No second revision reader, boot exception or backfill workflow is needed.

**Decision-complete minimum correction:** at the existing artifact-set SQL owner, return revision **0 only when this is a fully validated exact zero-insert replay and its node/table revision row is absent**. Return an existing revision unchanged when present. A fresh insert must still require its trigger-created revision; partial sets, contradictory rows, malformed input, stale actor fence and SQL failures must still fail. Keep the transaction/commit, identity locks and error mapping. Do not blanket-convert SQL errors to 0, fabricate a positive revision, delete/reinsert published rows, skip remote verification/manifest recovery, or add a compatibility service.

This uses the approved initial state, so no new persistent-data or rollout decision is needed. At migration startup all existing publications belong to revision 0; subsequent inserts create positive revisions atomically, which current cached-cut validation detects as before. Exact old replay creates no new publication and cannot make a cached cut miss a newly inserted old row, because no such insertion occurs. A universal missing-row fallback for fresh inserts would weaken the invariant and is expressly excluded.

**Focused closure proof:** reuse current Postgres-backed persistence/staging owners. Model the equivalent migrated initial state with exact committed file-list rows plus a surviving publication manifest and no revision for that producer/table. Drive actual recovery/reconciler, not merely a helper returning zero. Require successful exact replay/cleanup, no duplicate rows, unchanged absent/logical-0 revision, and next fresh publication creating revision 1. Also cover exact replay after a revision exists (unchanged), conflicting/partial replay and stale actor refusal. Existing SQL trigger test inserts after migration and therefore cannot alone cover this recovery state. No full SDK journey or new harness is necessary for this seam.

## Prior-ID closure reconciled against source

| Prior ID | Result and evidence |
| --- | --- |
| FIND-TASK-001-1 | **OPEN, partially corrected**, as above. Original query edges are fixed; persistence copies remain outside the shared ownership graph. |
| FIND-TASK-001-2 | **CLOSED.** `tail_rpc.rs::retain:842` inserts first, removes/refuses elapsed installation, then arms expiry. A release before abort-handle installation leaves at most an empty-table timer. `RetainedCapture.deadline` and `open_capture:953` check the original monotonic authority before projection; explicit release remains query/table bound and readers retain independent charge/lease. New elapsed-installation/open proofs distinguish both original gaps. |
| FIND-TASK-001-3 | **CLOSED.** `LiveCaptures` stores the original deadline and its final Drop passes it to discovery. Server `admit` passes the same deadline on partial-listing failure. Shared server `release:378` bounds the concurrent RPC join with `timeout_at(deadline)` at 421. Local release is synchronous. No renewed budget, broad fail-stop or unbounded sibling release remains in these traced callers. |
| FIND-TASK-001-4 | **Original five sites CLOSED; cumulative gate OPEN** as above. Server list/release docs now name actual decode/state/elapsed errors and cancellation retention; Arc resolver documents unchanged delegation; fixture discover documents success; formatter documents formatter errors. |
| FIND-TASK-001-5 | **CLOSED.** OLAP reference removes publication-duplicate permission. Current Bifrost/Wyrd architecture, dispatcher/query-stream guidance, product pages, diagram and generated LLM prose distinguish listing-only degradation from fatal accepted-capture open/stream loss, including before rows. `LiveFragmentRead::into_stream` has no pre-row execution fallback. Historical review prose is preserved as history. |

## Reconciled original acceptance matrix

PASS below is task-scoped source plus recorded proof; it does not claim fresh execution or completion of explicitly deferred change-review SDK/agent gates. Discovery matrices' REQ-003/INV-004 PASS is revised for FIND-6, and complete rustdoc PASS is revised for FIND-4.

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| REQ-001 rows survive source changes | Coherent `LiveCapture::take`, exclusive registry memory/staged/published selection, retained reads; recorded exact-row source-transition/rollover/staged proof | PASS |
| REQ-002 acquire/discover/validate before execution | Oracle classification retries stale cuts before admission/output, one attempt's routes; recorded invalid-capture/transition proof | PASS |
| REQ-003 uncertainty and recovery | Publication marks, atomic advancement and revision restoration present; pre-revision committed recovery refuses valid exact state, FIND-6 | FAIL |
| REQ-004 preserve merged verification | Shared Oracle remains the reader; late-evidence group proof recorded; full integrated checklist deferred by approved packet | PASS at task scope |
| REQ-005 remove superseded behavior | Fresh execution snapshots and pre-row accepted-loss fallback removed; current guidance corrected | PASS |
| REQ-006 actual final capacity | Retained sequence fails absolute targets and predates runtime R1; no final R1 qualification | FAIL |
| INV-001 one server read/authority owner | Existing Oracle/Scribe/Vala SQL owners extended; no client/group workaround | PASS |
| INV-002 bound capture authority | Full query/table/stream/deadline checks, authenticated context and capture digest; recorded authority/expiry/peer proofs | PASS |
| INV-003 ingestion continues | Local short capture guards only; no append SQL/global or remote barrier | PASS |
| INV-004 committed ordered/replay-exact revisions | Trigger transaction/counter lock and rollback correct; valid initial-state exact replay fails, FIND-6 | FAIL |
| INV-005 invalid acquisition/accepted loss cannot succeed | Stale selection discarded, uncertainty refused, execution loss fatal; recorded negative matrix | PASS |
| INV-006 bounded physical retention | Expiry/release/file safety corrected; missing persistence charge, FIND-1 | FAIL |
| INV-007 real measurements | Acquisition included; answer/error/refusal/sample/headroom checks retained, failed reports preserved | PASS |
| INV-008 warm current cuts have no catalog statement | In-memory comparison, cache eviction generation, repin only when required; recorded warm zero-read/stale exactly-one-read proof | PASS |
| AC-001 exact real-server transitions/final partition/rollover | Published source-transition journey and local rollover/staged proof retain exact identities without deduplication | PASS |
| AC-002 negative acquisition/resource lifetime | Authority/expiry/cleanup proofs supported, but persistence charge and old-state recovery remain incomplete | FAIL |
| AC-003 late evidence reaches verdict | Recorded shared-reader group proof; integrated Rust/Python/TS/MCP/HTTP checklist remains change review | PASS at task scope |
| AC-004 one current implementation/docs | Removed superseded entry points and allowance; authored/generated prose agrees | PASS |
| AC-005 inclusive equality and next representable failure | Report comparison changes/boundary tests retain exact limits and unrelated strict/sample/correctness checks | PASS |
| AC-006 final identified capacity sequence, queries last | Older measured `05186c1e4` fails; no newer R1 sequence | FAIL |
| AC-007 cached read and one stale retry | Cache generation eviction and shared profile/retry; recorded warm and held-publication proof | PASS |
| No alternate reader/group scheduling/history scan/WAL/unavailable-writer recovery | None added; preserve current group/read boundaries | PASS |
| No global point-in-time or late-append promise | Selection scoped to selected online writers and acknowledged qualifying rows | PASS |
| No public query/SDK selector or mixed-version path | Internal capture protocol only, coordinated rollout | PASS |
| Preserve Forge/hot-Iceberg reconciliation/pinned-file protection/lazy decode | Existing metadata/scan/leases retained; capture holds existing file leases | PASS |
| Preserve tenancy, authorization, peer authority, non-blocking audit | Tenant-scoped cut/revision state, invoker/RLS and existing private/public authority retained | PASS |
| Preserve rollback/failure/identity/WAL progress | Existing owner paths remain; correction must retain them, exact focused regression below | PASS except FIND-1 ownership edge |
| Mandatory changed-item documentation | Five original methods corrected; new test module omitted | FAIL |
| Carried workflow prose/drafts/span-permit correction | Disclosed drafts do not implement features; permit test waits on actual semaphore with a failure deadline | PASS for disclosed scope |

## Qualification evidence and limits

Directly checked `../bsc-rev2/commit`, `status` and reported measurements: source `05186c1e443096e6c33d6b5f08831b613fa2caa2`; write exit 0, verification/hot/Parquet exits 1. Hot selective one-client p99 **2.352 ms**, Parquet **2.745 ms**, versus **≤1.000 ms**. Direct 64-client verification has **435,378** observations and the **0.9 ms** p99 overhead bucket, failing the required **≤0.5 ms** qualification. Passing throughput/zero errors and baseline-relative comparisons do not repair failed latency. R1 changes runtime ownership/cleanup; no final-candidate comparable write → verification → hot/Parquet queries sequence is supplied.

Keep that original failed obligation visible. Diagnose from existing traces/profiles and run the approved final sequence after affected correctness checks; do not prescribe an imagined performance optimization, omit samples, raise thresholds or declare capacity waived by R1's bounded scope. Performance failure alone establishes neither a bounded code correction nor a need for spec revision.

`verification.md` records three exact Redux regressions and one exact server release regression passing in this review, plus clean diff whitespace. They support their exercised paths. The charge test lacks the production conversion. The mTLS release test drives `admit` with supplied partial results and directly calls the shared release for its second case; its five-second watchdog is looser than its 500-ms query deadline. The actual source's propagated absolute deadline establishes the bound; do not describe this test as a precise 500-ms latency measurement or direct final-route Drop execution. These limitations do not establish another production defect.

## Minimum focused proof and lanes for remediation

New test names remain implementation-owned; source-confirm and record their exact package/target/selectors for RED/GREEN. Do not invent selectors. The new payload/snapshot holder proof can live where private snapshot access exists and reuse the existing charged fixture. The new old-state SQL/recovery proof must run with repository-managed Postgres and real migrations, using the owning persistence/staging fixtures rather than a new harness. Do not require broad journey suites or capacity inside this bounded correction.

Source-confirmed existing selectors for adjacent accounting/replay behavior (all Redux lib; the wrapper/migration covers owner fixtures that consult SQL):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops) | test(=scribe::shards::tests::replay_identity_failures_settle_response_and_release_ownership) | test(=scribe::shards::tests::replay_retirement_settles_response_and_releases_ownership) | test(=scribe::shards::tests::replay_insert_adoption_failure_rolls_back_identity_and_immutable) | test(=scribe::shards::tests::immutable_release_failure_prevents_false_retirement)'"
```

Source-confirmed publication/recovery regression selectors (the manifest test proves typed durable recovery, not the missing SQL seam; the new proof must reach the real SQL owner):

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::file_list_writer::tests::artifact_set_publication_classifies_every_row_state) | test(=scribe::file_list_writer::tests::deterministic_artifact_rows_are_retry_stable_and_artifact_local) | test(=scribe::staging::tests::publication_manifest_recovers_rows_and_source_fence_fail_closed)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test integration -E 'test(=pg_oracle_membership::pg_tests::file_list_publications_advance_their_node_revision)'"
```

The preserved failure-readiness selector is `scribe::pg_scribe_restart::replay_failure_keeps_scribe_unready` in `vala-bifrost-redux --lib` (its in-memory object-store/WAL fixture needs no Postgres); run it with the same exact `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_restart::replay_failure_keeps_scribe_unready)'` form if the new proof touches boot/replay wiring. The selected correction need not change boot at all.

After focused proofs, supporting owner coverage should be restricted to the actually changed Redux `scribe::{persistence,file_list_writer,shards}` test modules, under the same Postgres/migration wrapper where needed. Source can express that with `-E 'test(/^scribe::persistence::tests::/) | test(/^scribe::file_list_writer::tests::/) | test(/^scribe::shards::tests::/)'`; add only genuinely touched owners. A module rustdoc edit earns no server runtime family rerun. Final static lanes: `mise run fmt`, `mise run lints`, `git diff --check`; run `mise run check:tenant-isolation` for the changed SQL read boundary. No dependency, public contract, migration edit or docs-site change is prescribed, so codegen/dependency/docs-site lanes are not automatically added. If implementation actually changes those surfaces, run their owning gates.

Full integrated SDK/agent gates and original capacity qualification remain change-review obligations and cannot be replaced by these focused tests. No test, lane or benchmark was run by this validator.

Skipped: source changes, Cargo/test/benchmark execution and integrated client/agent qualification, as assigned.
Risk: persistence can expose already-owned memory as free, valid old committed recovery can keep Scribe unready, and original capacity remains failed/unqualified.
