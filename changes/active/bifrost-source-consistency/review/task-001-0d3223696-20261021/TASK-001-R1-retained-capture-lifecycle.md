---
id: TASK-001-R1
kind: remediation
status: review
spec: SPEC-bifrost-source-consistency
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
requirements: [REQ-002, REQ-005, INV-002, INV-005, INV-006, AC-002, AC-004]
depends_on: []
---

# Keep retained captures governed until they are released

Execution skill: `$wyrd-implement`. This task packages the independently validated findings from [findings-validation.md](findings-validation.md); it does not introduce a new design decision.

Approved spec: `changes/active/bifrost-source-consistency/spec.md`, revision 2. Original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`. Review base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`; cumulative candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`. Prior verdict: [verdict.md](verdict.md), FIX_REQUIRED. There is no earlier remediation round.

## Outcome and limits

A query may retain the exact live rows it selected while publication and ingestion continue. Those retained buffers must remain charged to the existing shared memory budget, expire under the query's authority, and never make failed acquisition wait indefinitely for cleanup. For example, when a query retains an old generation and Scribe stages it, ingestion cannot reuse that generation's memory charge while the query still holds its Arrow buffers.

Fix the shared owners below, preserving the source-consistency protocol, error vocabulary and published/live row semantics. Update the related mandatory rustdoc and current contract descriptions in this same correction. No public API, SQL schema, new dependency, configuration, compatibility protocol or alternate resource governor is required.

This task closes five bounded findings. **It does not close REQ-006 / AC-006.** Actual performance qualification remains failed, with no independently established sufficient optimization. Do not change thresholds, workloads, sample accounting or acceptance to clear that blocker. The original task's performance investigation and final qualification remain due separately; closure of this remediation cannot authorize PASS for TASK-001.

## Diagnosis and selected correction

### FIND-TASK-001-1: retained allocations lose their charge

`scribe/memtable.rs:1678,1701,1809,1886` retains and projects shallow RecordBatch references in `LiveCapture` and `CapturedRead`, without retaining accounting ownership. Normal generation retirement in `scribe/shards.rs:2375–2417` calls `ScribeOwnership::release_immutable` at line 2405, returning the root-backed charge even if those references survive. The full accounting owner is `scribe/memory.rs`; follower `oracle/follower.rs:414` projects the retained memory into `MemtableSource`, also without adopting its original charge.

Oracle captures during classification, before `admit_built_attempt`. A normal query can hold table A while another table settles, while queued for admission, or during execution. Rotation/staging retires A's generation and frees its ledger charge; new writes may then reuse that capacity while A's captured allocations remain resident. Count/lease assertions in current tests do not prove this byte-lifetime invariant. Opened projected reads can outlive the capture-table entry, so a registry-only charge is insufficient.

**Selected correction:** keep the already admitted Arrow charge attached to shared allocation ownership across active memory, freeze, retirement, captures and opened readers. At retirement, transfer the retiring generation's existing root-backed charge into the retained allocation lifetime instead of returning it while physical holders remain. Reuse `ScribeOwnership` and `ScribeMemoryLease`'s existing net-zero transfer capabilities. Overlapping captures/readers share that charge and the final physical holder returns it exactly once. Preserve rollback/error handling and the sibling retirement/replay writers identified in validation.

This fixes the common allocation/accounting boundary. Do not double-charge each query, reserve a whole table per capture, create a capture quota/governor, wait for query completion before durable publication/WAL progress, or move admission ahead of physical classification. Private representation and local helper choices remain implementation-owned.

### FIND-TASK-001-2: expiry can miss installation or authorize an elapsed open

`scribe/tail_rpc.rs:805–829` spawns a one-shot expiry remover before inserting its retained entry. Synchronous selection/sizing can cross the deadline first. On another runtime worker the already-due remover may observe no entry, finish, and then be followed by insertion. If the listing response is lost/cancelled, no caller knows the ID and no later expiry exists. The retained staged lease can block normal/recovered publication cleanup indefinitely.

Separately, `RetainedCapture` at line 468 stores no deadline. `open_capture` at line 877 checks stream/query/table/ID but can open an elapsed capture while timer cleanup has not been polled. Earlier peer expiry checks precede awaited schema resolution and do not enforce this capture-local authority at open. A leader timeout later in the call does not satisfy the explicit expired-source refusal promise.

**Selected correction:** make registration and expiry activation/removal one ordered lifecycle at the existing capture-table owner. An expiry must never complete before an entry can later be installed. Refuse elapsed acquisition before exposing its identity, retain the original monotonic deadline as capture authority, and refuse a new open after it elapses before projecting sources. The guard belongs here because this owner grants access to the retained source. A precheck alone does not close the installation race.

Reuse the same registry/removal/timer and existing refusal vocabulary. Preserve exact query/table/stream authority, authenticated idempotent release, removal drops outside table locks, and already-opened memory/file reader protection. Do not add a sweep/reaper, fresh snapshot, renewed deadline or per-follower workaround.

### FIND-TASK-001-3: release can keep failed acquisition pending forever

Server `oracle/tail_discovery.rs:304` times only the listing join at line 354. Its subsequent `admit` at line 244 awaits release of successful captures after a sibling listing failed/staled. `release` at line 369 awaits ReleaseCapture RPCs at lines 399–413 without a remaining-deadline bound. The transport at Redux `scribe/tail_rpc.rs:139` and TLS endpoint set no request timeout. `LiveCaptures::drop` is the sibling detached release caller and has the same unbounded wait.

The validator traced local public and scheduled/internal callers through Gate/forwarding/Oracle and found no outer timeout covering this wait; the HTTP edge timer has handed off before dispatch. One peer can return a capture, a second can fail listing, and the first can then never answer release. Scribe expiry cannot wake that pending RPC. Failed requests and detached cleanup tasks may therefore outlive their original deadline indefinitely. Existing global listing-outage tests retain no successful peer capture and miss this seam.

**Selected correction:** carry the existing absolute query deadline through retained-route cleanup and failed-acquisition cleanup, and bound release at the shared discovery owner by that deadline. On exhaustion stop waiting, log the failed best-effort cleanup and rely on the corrected Scribe expiry from FIND-2. Preserve cleanup to responsive peers and the original listing/stale/elapsed error mapping. No separate timeout configuration, retry loop, renewed budget, replacement capture or process-wide failure is needed.

### FIND-TASK-001-4: five changed methods lack mandatory error/lifecycle docs

The actual `AGENTS.md` §16 and agent-rules require substantive rustdoc and explicit `# Errors` for changed fallible methods, including private/test/formatter implementations. Complete the contracts on:

- Server `grpc/scribe_tail.rs:49`, `list_active_streams`: malformed binding/query ID, elapsed/state failures, and interrupted-response retention/release/expiry behavior.
- Same file line 83, `release_capture`: malformed binding/IDs and state failures.
- Redux `oracle/follower.rs:158`, Arc resolver delegation: errors propagate unchanged.
- Redux `oracle/live.rs:138`, `NoCaptureDiscovery::discover`: fixture always succeeds.
- Same file line 193, `LiveCaptures::fmt`: formatter errors propagate.

Describe corrected bodies and their cancellation/partial-progress semantics, without changing error behavior or inventing runtime tests for prose. This is batched with the runtime correction, not a separate style round.

### FIND-TASK-001-5: current contract guidance contradicts accepted-source loss

`architecture/references/domain/olap-serving.md:93–94` still permits publication duplication. `architecture/bifrost-design.md:357–359`, Redux `oracle/dispatcher.rs:1782–1787` and `oracle/query_stream.rs:891–893` retain general pre-row execution degradation guidance. Newly changed `architecture/wyrd-design.md:1699–1700`, product docs `understand/bifrost.md:61`, `understand/runs-observations-and-evidence.md:107`, and diagram `bifrost.ts:300` promise degraded results whenever the holding Scribe is unreachable. Generated `docs/public/llms-full.txt` repeats those claims.

Actual `list_live_routes` records listing-only degradation. After a capture is accepted, `LiveFragmentRead::into_stream` treats open/stream loss as fatal even before rows; the updated terminal matrix expects Failed. The documentation therefore misstates the usable-result boundary and preserves the exact obsolete overlap allowance required to be removed.

**Selected correction:** document listing degradation separately from fatal accepted-capture loss, including before the first row, and remove publication-duplication and execution-degrade instructions. Keep runtime behavior and historical packets. Change authored docs/diagram prose and regenerate LLM text through the existing generator. Do not modify runtime semantics to match stale prose or add unrelated wording cleanup.

## Preserved behavior and scope

- Source acquisition still chooses coherent memory/staged/published sources once, validates reported revisions, and discards stale attempts before rows.
- Current cached cuts still perform no catalog read; revisions remain committed, per-node and replay-exact.
- Appends continue without a global pause, remote publication barrier or extra append catalog request.
- Existing admission classification, typed pressure/errors, rollback, tenant/peer authority, non-blocking audit, publication recovery and retained-file safety remain intact.
- Reader projections/repartitioned clones that retain Arrow must preserve its shared accounting owner. Already-open readers remain readable and protected after registry removal.
- Group scheduling, outbox accepted-loss bounds, SDK/public inputs, unavailable-writer behavior and final capacity targets are unchanged.
- Reconcile stale task revision/status and overly broad feature-evidence wording when recording disposition; do not rewrite historical evidence or implement carried draft Verifier specs.

Expected write set: existing Redux Scribe memory/memtable/retirement/capture and Oracle retained-route/follower owners; server discovery/private capture adapters and focused tests; the cited current contract docs/diagram/generated LLM text. Follow dependencies actually touched without unrelated rewrites. No manifest/schema/public binding change is prescribed.

## Acceptance and focused proof

| Finding | Required observable closure |
| --- | --- |
| FIND-1 | Retain real charged active/frozen buffers through staging/retirement and pre-admission waiting. The shared root continues to account them and refuses growth at its existing cap. Two captures and an opened projected reader share one charge; dropping a non-final owner cannot free it, and the last holder frees the exact bytes once. Rows and ordinary WAL/publication retirement still progress. |
| FIND-2 | A deterministic expiry-during-installation schedule cannot leave an entry or staged lease after deadline without explicit release. Passing the clock beyond deadline without polling cleanup refuses a new open. Existing valid/foreign/query-bound release and opened-reader retention behaviors remain. |
| FIND-3 | With one successful peer capture, one failed listing and a permanently pending release RPC, discovery/query settles by the original deadline; detached last-route cleanup is bounded too. Responsive release still frees promptly; expiry settles otherwise abandoned retention. |
| FIND-4 | All five changed methods have body-accurate substantive error and relevant cancellation contracts, with no placeholder docs or behavior change. |
| FIND-5 | Every cited current statement agrees with listing-only degradation, fatal accepted-capture loss and exact source-transition coverage; authored and generated docs agree. |

Use deterministic clock/scheduling seams and existing resource/shard/private-peer fixtures, not host stress or a new harness. For each new executable regression, confirm its actual owning module and exact selector, run the exact `mise exec -- cargo nextest run --locked -p <owner> --lib -E 'test(=<actual-name>)'` command for RED, confirm the diagnosed failure, and record the concrete command/results in task evidence before GREEN. New test symbols/fixture layout are implementation-owned; this packet does not claim planned tests already exist. Use the repository Postgres wrapper only for proofs whose real fixture requires it, and trace any failure with logging before diagnosis.

Existing focused regression command (all names are existing source-confirmed tests):

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::active_rollover_during_capture_is_read_once) | test(=scribe::tail_rpc::tests::a_generation_staged_before_the_shard_settles_is_read_once) | test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication) | test(=scribe::tail_rpc::tests::a_capture_is_opened_only_by_its_query) | test(=scribe::tail_rpc::tests::an_unreleased_capture_expires_at_its_deadline) | test(=scribe::memory::tests::generation_ownership_moves_exact_category_bytes)'
```

After new exact proofs and that regression command pass, run the narrow supporting owner coverage for the actual write set:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::tail_rpc::tests::/) | test(/^scribe::memory::tests::/) | test(/^oracle::follower::tests::/)'
```

Run the newly added server/private-discovery proof with its exact package/target/test selector. If its fixture requires Postgres, include the existing `scripts/postgres/with-test-postgres.sh` setup and owning migration step. Use `mise run test:bifrost:integration:server` only if the actual correction exercises that integration seam; do not automatically add SQL or unrelated integration families. The existing `test:bifrost:unit:rust:inner` is not a substitute for Redux coverage; it selects server/testing/client.

Complete the Rust/docs write set sequentially with:

```bash
mise run fmt
mise run lints
mise run docs:generate
mise run docs:check
git diff --check
```

Add only missing proof earned by the actual write set. Dependency/contract checks are required if such boundaries actually change, although none is prescribed. Do not require the full journey suites or broad `verify:bifrost` aggregate inside this remediation; integrated change review owns those once. Do not run a capacity sequence as a substitute for diagnosing its still-open acceptance failure. The original approved final qualification sequence remains mandatory on the final candidate after affected checks, outside this bounded closure packet.

## Completion evidence

Record the correction boundary and focused Red-Green results for each FIND, concrete selectors/commands and candidate identity, final scoped verification, and any remaining limits. Preserve the independent review reports and original task. Closing these five findings does not mark TASK-001 complete: its next cumulative review must reassess all original obligations, including the separately failed capacity qualification. No threshold waiver, acceptance decision, implementation, merge or publication is authorized by this remediation artifact itself beyond routing its bounded work to `$wyrd-implement`.

### Implementation evidence

Candidate: branch `change/bifrost-source-consistency`, commits `e9b2552e5` (FIND-1..3), `4ded8ffd5` (FIND-4), `48a6624a2` (FIND-5), `a29b20e06` (lint/format), on top of reviewed candidate `0d3223696`. This packet's status/revision wording and TASK-001's stale revision-1/proposed header are reconciled to revision 2 (`review`, `in_progress`).

**Reuse map.** FIND-1 reuses `ScribeMemoryLease::split` on the immutable ledger (`ScribeOwnership::detach_immutable`) and the lease's own `Drop`; the new `RetainedArrowCharge` is only the shared slot holding that lease, cloned by `ReadableSource`, `CapturedRead`, `LiveTailBatches` and `MemtableSource` (including streams it opens and its fetch/projection rewraps). No quota, governor, reservation or per-query charge was added. FIND-2 reorders the existing `RetainedCapture` registration and expiry. FIND-3 passes the existing absolute query deadline through `TailStreamDiscovery::release`, `LiveCaptures` and server `admit`/`release`, bounded with `tokio::time::timeout_at`. No retry, configuration or renewed budget was added.

| FIND | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 | `scribe/memory.rs` `RetainedArrowCharge`, `detach_immutable`; `memtable.rs` charge on bucket → frozen → captures/reads and `commit_retirement` deposit; `shards.rs` `release_committed_generation_memory`; `oracle/memtable_source.rs` charge-holding source/stream; `oracle/follower.rs` | `scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops`. RED (old `release_immutable` restored): panicked at `shards.rs:8097`, root total 0 ≠ 144 retained bytes. GREEN: pass. | PASS |
| FIND-2 | `scribe/tail_rpc.rs` `retain` inserts, refuses an elapsed deadline, then installs expiry; `open_capture` refuses past `deadline` | `scribe::tail_rpc::tests::a_capture_elapsed_before_installation_is_refused_and_retains_nothing` (RED: panicked at `tail_rpc.rs:1766`, elapsed capture accepted); `scribe::tail_rpc::tests::an_elapsed_capture_refuses_a_new_open_before_its_expiry_runs` (RED: panicked at `tail_rpc.rs:1803`, elapsed open succeeded). GREEN: both pass. | PASS |
| FIND-3 | redux `oracle/tail_discovery.rs` trait `release(.., deadline)`; `oracle/live.rs` `LiveCaptures.deadline` passed in `Drop`; `oracle/mod.rs` construction; server `oracle/tail_discovery.rs` `admit`/`release` bounded by `timeout_at` and a warning at expiry | `oracle::tail_discovery::tests::a_stalled_release_settles_by_the_query_deadline` (mTLS loopback Scribe whose release never answers; one retained capture plus one failed listing; then a detached release). RED (bound pushed out by 1 h): panicked at `tail_discovery.rs:599`, `admission settles by the query deadline: Elapsed(())` after the 5 s guard. GREEN: pass in ~1 s. | PASS |
| FIND-4 | server `grpc/scribe_tail.rs` `list_active_streams`, `release_capture`; redux `oracle/follower.rs` Arc delegation; `oracle/live.rs` `NoCaptureDiscovery::discover`, `LiveCaptures::fmt` | Docs checked against bodies (`binding_from_wire` → invalid-argument, `capture_active_streams` → deadline/internal, poisoned table → internal); `mise run lints` | PASS |
| FIND-5 | `olap-serving.md` duplication allowance replaced; `bifrost-design.md`, `wyrd-design.md`, `understand/bifrost.md`, `understand/runs-observations-and-evidence.md`, `diagrams/bifrost.ts`, redux `dispatcher.rs` and `query_stream.rs` describe listing-only degradation and fatal accepted-capture loss even before the first row; `docs/public/llms-full.txt` regenerated | `mise run docs:generate`, `mise run docs:check`; behaviour confirmed at `live.rs` `into_stream` (open error → fatal) | PASS |

**Diagnosis of the intermittent FIND-2 failure.** Symptom: an earlier diagnostician run reported `a_capture_elapsed_before_installation_is_refused_and_retains_nothing` failing at `tail_rpc.rs:1766`. Evidence: 1766 is the RED assertion, and those runs overlapped the window in which the temporary RED edit (`if false && retained.elapsed_at(..)`) was in the working tree. Cause: the tests ran against the RED source, not a race in the test. Three later traced runs (`WYRD_LOG=info,vala_bifrost_redux=debug`) on the GREEN source passed both FIND-2 tests. No test or timing change was made.

**Commands (final tree, all exit 0):**

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops) | test(=scribe::tail_rpc::tests::a_capture_elapsed_before_installation_is_refused_and_retains_nothing) | test(=scribe::tail_rpc::tests::an_elapsed_capture_refuses_a_new_open_before_its_expiry_runs)'   # 3 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=oracle::tail_discovery::tests::a_stalled_release_settles_by_the_query_deadline)'   # 1 passed
# listed regression command above: 6 passed
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::tail_rpc::tests::/) | test(/^scribe::memory::tests::/) | test(/^oracle::follower::tests::/)'   # 26 passed
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::shards::tests::/) | test(/^scribe::memtable::tests::/) | test(/^oracle::memtable_source::tests::/)'"   # 79 passed
mise run fmt && mise run lints && mise run docs:generate && mise run docs:check && git diff --check
```

The server proof needs no Postgres, and it drives `RegistryTailStreamDiscovery` directly over a real mTLS peer, so `test:bifrost:integration:server` was not added. `replay_identity_failures_settle_response_and_release_ownership` needed a test-only drop of the `freeze()` clones it held: they are holders of the retired generation's charge, as production persistence holds them. A diagnostician confirmed this is not a production defect.

**Limits.** REQ-006 / AC-006 performance qualification remains open, and no capacity benchmark was run. No thresholds, public API, SQL, configuration or dependency changed. No sweep or reaper was added. Full journey suites and broad aggregates are left to change review.
