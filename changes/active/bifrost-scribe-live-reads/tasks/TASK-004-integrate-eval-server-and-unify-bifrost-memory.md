---
id: TASK-004
kind: remediation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 13
requirements: [REQ-003, REQ-005, REQ-008, REQ-010, REQ-011, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-003, AC-004, AC-008, AC-009, AC-010, AC-012, AC-013]
depends_on: []
parent_task: TASK-003
remediates: [INT-001, MEM-001, MEM-002, MEM-003, MEM-004]
---

## Outcome and Value

One Wyrd server runs continuous Eval and the current Bifrost read/write
architecture through the simplified startup and peer routing implemented on
vcc/task-006. One governed Bifrost memory budget leaves configurable server
headroom without reserving memory for idle Scribe, Oracle, or Forge roles.
The integrated 4-CPU/8-GiB system meets the approved standard and heavy
benchmark targets before the repository gate runs.

This task also remediates four source-validated defects: Forge reserves an
idle share but executes with an unbounded DataFusion pool; transport admission
uses the supposed non-Bifrost reserve; Scribe and Oracle keep fixed floors;
and admitted-query memory exhaustion is mislabeled as admission refusal.
It does not reimplement Eval or replace TASK-003.
TASK-003 code on the merged PR is the starting state, except that its
role-local WAL failure requirement remains unimplemented. TASK-004 completes
that specific requirement on the integrated branch and owns the remaining
benchmark and gate evidence; a separate TASK-003 PASS verdict is not a
prerequisite.

INT-001 is the incompatible Eval/server/Bifrost branch overlap. MEM-001 is
the fixed Scribe/Oracle floor split in resources.rs; MEM-002 is Forge's
reserved estimated budget beside its unbounded execution in
forge/managed/executor.rs; MEM-003 is transport capacity derived from the
unmanaged reserve in resources.rs; MEM-004 is Oracle's DataFusion resource
error mapping to query admission in oracle/mod.rs. Each is visible in the
current source; the replay ledger and Scenario RED tests validate their
observable consequences on the integrated base.

The approved verification contract at pinned vcc/task-006 revision 44 owns
Eval and startup behavior: REQ-077, REQ-083–085, REQ-130–131, REQ-153–164,
AC-014, AC-016, AC-027, AC-034, and AC-036 are its relevant obligations.
Carry that revision into the integrated packet. The current worktree's
verified-change-contract spec is revision 38 and must not be misrepresented
as revision 44. Replay and verify the approved implementation; do not
redefine its behavior here.

## Revision 13 — One capacity owner, no future-byte hold-back

This maintainer-approved revision applies to unfinished resource work. Keep
completed Eval, boot, peer-authentication, and Scribe-follower changes in this
worktree. It supersedes TASK-003's instruction to keep the per-table
contention *capacity vector*: retain tenant/table scheduling, not a reservation
for every future phase of a table. Do not restart TASK-004 or port old Bifrost
resource code from vcc/task-006.

### Exact read and write flow

**Before ACK:** Keep the existing configurable 16-MiB-default wire ceiling and
its derived four-times expanded-data ceiling. Charge the encoded body while
held. An OTLP decoder may temporarily charge its preflighted generated-request
backing and JSON decode scratch because those allocations occur inside
Prost/JSON before Wyrd sees them. Its existing lease transfers into Scribe;
scratch returns when decoding ends. Projected future Arrow output is not
charged. New Arrow/copied buffers receive ownership when materialized;
zero-copy Arrow views transfer the retained wire-body lease without charging
the same allocation again. Validate and complete all fallible charges before
WAL/ACK. On refusal, drop the owners and leave no WAL or ACK.

**After ACK:** The shard retains query-visible WAL and memtable data until the
existing durable stage/publication transitions retire them. Size/age rotation
and existing bounded producer/claim work drive staging. Writer output is
chunked; Wyrd-owned buffers are charged for the bytes they actually hold. A
failed stage attempt retains WAL and retries. It cannot retract an ACK or
publish a partial result. The existing volume owner still reserves real disk
use. No predicted writer, footer, transfer, or table-lifecycle memory is
admitted in advance.

**During queries:** Leader admission remains the only query queue. Scribe
followers already use the receiving pod's shared DataFusion memory view and
leader-owned response stream. A receiving Oracle must reserve actual running
slot units before accepting work from any leader. Footer reads charge decoded
metadata retained in the cache, not a fixed 40-MiB slot. The existing one
storage-I/O semaphore is a work bound: occupied permits wait within the
current operation deadline/cancellation. Only the leader retries an explicit
pre-accept peer capacity refusal, within the original query deadline.

### Required deletions and owner changes

| Existing owner/files | Implement this decision |
| --- | --- |
| `scribe/admission.rs`, `scribe/contention.rs`, `scribe/geometry.rs`, `scribe/ingress.rs`, `scribe/shards.rs`, `scribe/telemetry.rs` | Delete `memory_breaker_bytes`, `AdmissionState::bytes`, its duplicate resize/release/refusal/metrics, and the future-capacity hold-back: `ScribeContentionLedger`, `ContentionReserveVector`, `ScribeGlobalCapacity`, category shares, activation/demand records, and `max_active_tables` derived from those shares. Delete duplicate active/immutable admission counters after replacing their diagnostics with the existing root snapshot. Keep `GLOBAL_INFLIGHT_ITEMS`, bounded shard mailboxes, and the existing hierarchical tenant-then-table round-robin/FIFO scheduler in `shards.rs`. Keep stage/claim work concurrency and real volume admission; do not create another fairness or memory ledger. Replace vector-specific tests with single-root and scheduler tests. |
| `scribe/material_plan.rs`, `scribe/ingress.rs`, `scribe/preprocess.rs`, `scribe/memory.rs`, `contracts.rs`, `gate/limits.rs`, server `otlp_decode.rs`, `otlp_json.rs`, `grpc/otlp.rs`, `http/otlp.rs` | Delete `root_bytes` as an advance charge, its eight-term summation, and the per-request `maximum_envelope_decision` based on future persistence work. Keep wire, expanded-data, and structural checks; no new size knob. At boot, reject a configured maximum expanded request larger than the resolved Bifrost cap; that comparison charges zero and creates no role share. `OtlpDecodePlan::reservation_bytes` and `JsonDecodePlan::reservation_bytes` include generated-request backing and actual decode scratch only, never projected output. `OtlpDecodeOwner::complete` transfers its lease rather than resizing to a whole future Scribe plan. Charge materialized Arrow/copy ownership before WAL/ACK; transfer a zero-copy view's source lease. |
| `scribe/persistence.rs`, `scribe/parquet_writer.rs`, `scribe/memory.rs`, `parquet/object_uploader.rs` | Delete `parquet_candidate_incremental_bytes`, the `candidate × 2 + 24 MiB` projected workspace admission, fixed 8-MiB `EncodedFooterReservation` child/split, and producer byte-based waiting on that estimate. Keep existing producer/claim worker-count limits and FIFO scheduling, plus chunked Parquet/object-store writes; delete their future-byte admission. Charge owned transfer buffers at actual allocated capacity. Recovery currently allocates one 8-MiB chunk but reserves 16 MiB: charge the actual chunk once and delete the second fixed 8-MiB allowance. Any later concurrent buffer follows the same actual-byte owner rule. Keep WAL and staged authority until durable retirement. |
| `resources.rs`, server `oracle/peer_service.rs`, `oracle/live.rs`, `scribe/staged_tail.rs` | Preserve the in-progress follower change: `ScribeResources::follower_memory_pool` is a shared governed DataFusion view with no Scribe follower CPU semaphore or estimated-byte charge. Remove only stale production paths/tests. `GreedyMemoryPool` in isolated tests is not a production second pool. |
| `storage/mod.rs`, `storage/policy.rs`, `resources.rs` | Delete `ORACLE_METADATA_MEMORY_BYTES`, `OracleFooterSlotResources`, `try_acquire_footer_slot`, their 40-MiB refusal and obsolete tests. Keep the actual-byte metadata-cache lease. Keep the existing configurable `max_concurrent_requests` (128 default) and its **one** semaphore. At both `governed_decode` and `attempt_once`, replace immediate `try_acquire_owned` refusal with asynchronous acquisition selected against owner/caller cancellation and the already existing absolute operation bound. Permit wait, retries, backoff, and I/O consume that one bound; if no caller bound exists, use the existing `max_retry_elapsed`. Start per-attempt `request_timeout` after permit acquisition. Drop the permit on success, failure, cancellation, and retry. Add no storage queue, second semaphore, or timeout setting. |
| `oracle/mod.rs`, `oracle/dispatcher.rs`, fragment and analytical leader callers | Delete `OracleSlotManager`'s pending semaphore/count/capacity API, `try_pending`, `PEER_SLOT_WAIT`, `PEER_SLOT_POLL`, and its sleep loop. Keep receiving-node `try_acquire_worker` / `try_acquire_query` and pending-to-running slot transfer: several leaders can target the same pod. A genuinely full receiving node returns existing `Rejected { retry_after_ms }` before work acceptance. Both fragment and graph leaders preserve that value. If placement cannot complete because required peers explicitly reject before work acceptance, release every provisional reservation from that round, wait for at most the indicated delay and remaining leader deadline, then retry placement. Cancellation/deadline stops retry. Transport error, missing reply, or possibly delivered work is ambiguous and never retried as capacity. Add no peer-side waiter or second query queue. |

Remove all consumers of the deleted types, including
`scribe/{claim_assembly,execution_lanes,member_stager,replay,staging_runtime}.rs`,
`storage/cache.rs`, and the tests in `resources.rs`. In particular, no
test-only `EncodedFooterReservation::for_test()` remains after the type is
deleted. Keep the existing owner and IO fixtures those modules still need.

Do not claim a hard resident-memory guarantee for allocations inside Prost,
parquet-rs, DataFusion's infallible `grow`, or OpenDAL. Existing request size,
one transferred opaque-decode charge, bounded work, the shared DataFusion
pool, and server headroom are the agreed in-process protection. Add no
execution subprocess, watchdog, precharge, or configurable limit. A maximum
legal request must ACK, stage, survive restart, and read back; if it cannot,
fix the writer flow before closing this scenario instead of restoring a
lifecycle vector.

### Revision 13 TDD scenarios

#### R13-A — One write owns each held byte once

**Behavior.** A request that fits the real shared cap succeeds even when the
old 90-percent breaker or eight-term plan would refuse it. A full root
refuses before WAL/ACK, then admits a write after release. OTLP wire, decoder
backing/scratch, and new Arrow buffers are counted once while live; a
zero-copy view adds no second charge. Busy tenants still alternate through
the existing shard scheduler.

**RED.** Add the two focused lib tests below for the root total, refusal,
release, and ownership transfer. Extend the existing shard scheduler fairness
test instead of creating another scheduler harness. The current breaker,
plan, and vector fail these cases.

**GREEN.** Apply the Scribe/OTLP deletions and transfers above; preserve public
size errors and first-commit ACK order.

**REFACTOR.** Remove vector/ledger types, category arithmetic, metrics, and
tests. Retain only actual-byte root attribution and real work counters.

#### R13-B — ACKed rows remain readable through stage failure and restart

**Behavior.** A maximum legal expanded request ACKs only after WAL sync and
memtable insertion. Under memory pressure it stays readable from live
authority; a failed stage attempt retries, later stages/publishes, and
survives restart with the same rows and no duplicate ACK. No future Parquet
workspace estimate refuses this already ACKed request.

**RED.** Add `write_read::acknowledged_rows_survive_stage_pressure_and_restart`
to the real-client Scribe journey with a test-only
post-ACK stage failure and restart/readback; assert ACK identity, live read,
retry, published read, and WAL retirement. The current producer byte wait and
fixed footer/recovery charges fail the pressure case.

**GREEN.** Keep bounded writer chunks and producer concurrency, charge actual
Wyrd-owned buffers, and retain durable retry/WAL lifetime.

**REFACTOR.** Delete the projected workspace and footer-child plumbing, not
WAL or volume admission.

#### R13-C — Storage contention waits within one bound

**Behavior.** Hold the single storage permit. A second footer/object read
waits rather than returning `RateLimited`, then completes if released before
its original deadline. Cancellation/deadline clears the waiter and active
permits. Metadata retention charges actual bytes, not 40 MiB.

**RED.** Add the two focused storage lib tests below. Immediate
`try_acquire_owned` and the footer slot fail them.

**GREEN.** Change both permit sites and remove the fixed footer lease without
changing backend retry/error classification.

**REFACTOR.** Delete obsolete footer-slot and immediate-refusal tests/metrics;
retain one semaphore and one operation bound.

#### R13-D — Only the leader retries peer slot pressure

**Behavior.** Two leaders target one Oracle; its actual running slots never
exceed its local limit. A pre-accept refusal carries retry timing; a leader
retries within its original deadline after a slot returns. Cancellation and
deadline release partial fan-out reservations. Ambiguous work is never
retried. Scribe followers retain no separate semaphore.

**RED.** Add the focused dispatcher lib test below and
`peer_network::analytical::two_leaders_retry_preaccept_capacity` to the
existing real-server journey, using two leaders and one receiving node.
The current peer waiter/poll loop and discarded
`retry_after_ms` fail these assertions.

**GREEN.** Preserve receiving-node slots, remove peer-side waiting, and make
both fragment and graph leaders own bounded retry and release.

**REFACTOR.** Delete pending semaphore API/counts/tests and duplicate peer
wait constants; keep leader deadline and actual slot metrics.

Add the named tests to the indicated existing lib modules, confirm their
selectors with `mise exec -- cargo nextest list`, then run exactly:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::admission::tests::one_root_charge_has_no_secondary_memory_ceiling)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::ingress::tests::decode_to_memtable_transfers_one_charge)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=storage::governed_request_tests::occupied_storage_permit_waits_within_operation_deadline)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=storage::governed_request_tests::footer_decode_has_no_fixed_memory_slot)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::analytical::tests::cancellation_during_peer_capacity_wait_stops_retry) | test(=oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch)'
```

Run the two new real-server cases through the existing PostgreSQL wrapper,
then run their owning lanes:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=write_read::acknowledged_rows_survive_stage_pressure_and_restart)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::analytical::two_leaders_retry_preaccept_capacity)'"
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
```

Confirm their selectors with `mise exec -- cargo nextest list` after adding
them. After these scenarios, run the existing
standard benchmark **first** and report accepted QPS, client p50/p95/p99,
ingest rows/s, CPU, peak cgroup memory, refusal reasons, and complete
readback. The required heavy qualification and the existing final gate order
below remain unchanged. Unit tests or a green gate cannot substitute for an
invalid benchmark row.

## Owners, Scope, Consumers, and Prohibited Changes

- This task's implementation base is `origin/main` at
  a56ab7569aa702dddec0b36d097d38abe8918e8c, the PR #94 merge whose
  second parent is vcc/task-005 commit
  5ae8125156add44e549ac090a82a4fcb20e9cce7. The dedicated
  `wyrd-verfication-t006-merge` worktree on branch `vcc/task-004` contains
  Bifrost spec revision 13 and this task,
  ported from packet commit 310765353361c0da7d3b906fedea04ddd3f53126.
  PR #94's CI override does not certify TASK-003 or this task. Replay Eval
  and peer behavior from pinned vcc/task-006 commit
  0e9c6e98c74361b1f2d18dd857a2aff7690cfdb0, not whole files or an
  automatic branch merge.
- TASK-006 owns the approved Eval verdict, post-ACK best-effort enqueue,
  durable run/result, SDK, startup, and private-peer contracts. Its simplified
  boot and routing become the one server path.
- Current Bifrost owns durable ACK, WAL lifecycle, one published/live query
  plan, query lifetime, and benchmark driver. Adapt Eval and peer behavior to
  these owners. Do not restore older Bifrost implementations.
- The operating system owns the physical pod limit. The Bifrost resource
  owner governs tracked Scribe, Oracle, Forge, and Bifrost transport memory.
  Server work has a configurable minimum of headroom and no application cap.
  Query slots and queue bound concurrency, not preallocated query memory.
- Do not add an Eval-specific queue or engine, second boot graph, second peer
  authentication scheme, second memory ledger, execution process, watchdog,
  victim selector, second query admission path, or benchmark-only server path.

### Packet-local replay decisions

Use pinned source commit `0e9c6e98c74361b1f2d18dd857a2aff7690cfdb0`.
The following map settles ownership before coding; a diff ledger records the
mechanical replay result, not new architecture decisions. Current Bifrost
implementations named in the right column win where source code differs.

| Source behavior and exact seam | Integrated owner and required ordering | Disposition |
| --- | --- | --- |
| `vala-bifrost-redux/src/gate/mod.rs::ObservationAck`, `Gate::with_observation_ack`, and `Gate`'s `admission.first_commit` callback | Keep current Gate → current Scribe `ingest_frame` → durable ACK order. Invoke `ObservationAck::acknowledged(AuthContext, Bytes, receipt_micros)` only for the first committed Eval observations batch, after Scribe returns success. Duplicate batch ID and failed write enqueue nothing. | Replay the small callback seam, not the source Scribe or WAL graph. |
| `wyrd-server/src/verification/observations.rs::ObservationEnqueue` and `boot/mod.rs::compose_bifrost` wiring | Compose one `ObservationEnqueue` into Gate. Its tracked task uses tenant Postgres transaction and `enqueue_observation`; backlog or enqueue failure logs/counts failure, leaves ACK and readable observation intact, creates no invented run. | Replay. No second queue or synchronous enqueue in Gate. |
| `verification/runner.rs::VerifierRunner`, `verification/eval.rs::EvalEngine`, `verification/results.rs::ResultPayloadBuilder`, and `app/server.rs` verification worker | One durable run/result owner handles Eval and Drift. Eval dispatch uses approved input, trace-wait, verdict, media, and Operator rules; publication settles one run only after result write ACK. The server supervises the one verification runtime on API targets and drains its tracked work on shutdown. | Replay; adapt reads and writes to current Bifrost APIs, never port an older Oracle or Scribe. |
| `boot/mod.rs::compose_bifrost`, `build_state`, `OracleRoleBuilder`, `app/peer_plane.rs::PeerPlaneStatus`, and `components/health/mod.rs::ReadinessSnapshot` | One boot graph and one readiness snapshot. The all target calls local Bifrost role handles in process. Peer mode publishes ready membership only after role activation and the private listener is serving; shutdown withdraws readiness and role advertisements before draining accepted work. | Replay source boot and peer composition, preserving current Bifrost role lifecycles. Delete displaced boot wiring. |
| `oracle/peer_service.rs::OraclePeerGrpc`, `wyrd-spec/src/vala/api.rs::PeerContext`, source `boot/mod.rs::build_bifrost_peer_tls`, current Oracle `ExecuteFragment` and Scribe live fragment | For another pod, one mTLS peer transport carries typed tenant/table/query context; receiver validates it and the existing leader-owned deadline before work. The response stream owns the remote fragment; disconnect, cancellation, or deadline closes it and releases its work. Local fragments use the local transport. Auth, stale fence, or tenant mismatch fails the query without partial success. | Replay mTLS and typed peer composition. Preserve current published/live plan, fragment terminal rules, and leader lifetime. Delete displaced ticket/replay authentication only after callers move. |
| Source `wyrd-client/src/observe/mod.rs::Observe::eval`/`eval_json`, Python `wyrd/observe` and `wyrd/eval`, TypeScript `wyrd/src/index.ts::Observe.eval`; `mise.toml::test:server:peer`; `wyrd-testing/tests/bifrost/{server/eval_verification.rs,oracle/peer_network/}` | Preserve one Eval emission payload and options through the shared Rust client and language projections. Keep current Bifrost query API. Port the source journeys and peer lane; then add the integrated seam cases below. | Replay approved Eval SDK behavior and affected generated contracts, not whole source SDK files. Regenerate rather than copy stale generated files. |

Do not import source gateway removal or unrelated Card changes merely because
they share a source commit. If a required TASK-006 Eval or boot symbol depends
on them, carry only its approved contract closure and record the dependency
in the replay ledger.

### One memory charge contract

`resources.rs::BifrostRuntimeResources` constructs exactly one process
`BifrostResourceGovernor`. It owns the resolved Bifrost cap and a single
serialized total of **held** governed bytes, with Scribe/Oracle/Forge/transport
attribution for diagnostics only. Every fallible charge checks
`held_total + newly_held_bytes <= cap` under that owner's existing
synchronization and either records the full charge or records none. A role
estimate, query ceiling, queued query, or planned compaction charges zero.
The only opaque-decode exception is the narrow temporary charge for generated
OTLP request backing and JSON scratch allocated inside Prost/JSON; transfer
that charge to its actual owner and never also charge projected Arrow output.

| Holder | Charge and return rule |
| --- | --- |
| Scribe | Existing ingress, memtable, and staged-memory leases charge their actual held bytes through this root; release only when their owning data/work drops. Remove floor and elastic partitions. |
| Oracle | Reuse `OracleMemoryRoot`'s DataFusion `MemoryPool` view. `try_grow` checks the query ceiling then charges the shared governor and pool atomically; on refusal roll back completed charges. `shrink` returns the same consumer's bytes. `grow` remains DataFusion's infallible path: record its overshoot as headroom and release it on shrink/drop, without a watchdog. Query-owned runtime and child graph retain the pool view until they drain; only then return slot and memory. |
| Forge | Feed the managed rewrite context a DataFusion `MemoryPool` backed by the **same** governor, using its existing `with_memory_pool` and `with_spill_lease` builder inputs. Use actual reservation growth/shrink and the same accounted-headroom rule for infallible `grow`, not the estimator or an independent finite memory cap. Keep existing parallelism. A failed attempt drops/joins its context before retry; its durable task remains unsettled and publishes no partial snapshot. No extra Forge heap estimate is charged. |
| Transport | Keep the current per-message byte ceiling and the existing body-admission lifetime. Validate the declared/decoded message against that ceiling before retaining it; charge the actual encoded bytes held, not a maximum-message precharge, to the shared root. HTTP and gRPC body owners return that charge when the body is consumed or dropped, including cancellation and decode error. Delete transport's independent aggregate counter and unmanaged-reserve coupling. |

DataFusion spill bytes remain disk use managed by its normal disk manager,
not memory charged to this ledger. A genuine shared-governor invariant poison
still stops the process through its existing health watcher; an ordinary role
capacity refusal does not. This is one owner with existing role-local handles,
not a second accounting service or a new public resource interface.

Forge spill uses a `forge-spill` child of TASK-006's existing locked
`BifrostDataRoot`; do not create another data root, disk ledger, or
Forge-specific scratch cap. Use DataFusion's ordinary disk manager to spill
temporary operator data as needed; the 1-GiB output-file target is independent
of spill use. A full disk fails the Forge attempt without partial publication.
DataFusion removes its temporary files when an attempt ends. After acquiring
the data-root lock on restart, remove only stale Forge spill files left by a
crashed process before admitting Forge work; do not add a normal-attempt
cleanup service.

### Scribe failure boundary

TASK-003 decided this boundary, but the cited PR #94 merge still routes failed
WAL rollback through process-wide `BifrostResourceHealth::Volume` poison and
requires Scribe health for combined-target `/readyz`. Complete this missing
TASK-003 handling here; do not replay the rest of TASK-003. WAL
integrity/ambiguous mutation marks **Scribe** unready immediately. Its owner
stops admitting writes before ACK, withdraws Scribe readiness/heartbeat and
cluster role fence, and
drains accepted work without deleting WAL or staged files. It does not ask
the process supervisor to exit or replay in process. On a later restart,
stage recovery and WAL replay finish before Scribe advertises ready; failed
replay leaves only Scribe unready for operator repair. A combined target
keeps Oracle queries and the verification worker available if their own
dependencies are healthy; an Eval operation needing Scribe reports that
dependency failure. A Scribe-only target is unready. The existing
process-wide resource-health watcher retains fail-stop authority for a
poisoned shared governor or runtime invariant. Do not turn every supervised
task exit into a recoverable role failure or add a recovery supervisor.
Keep Scribe unready in the readiness response body even when a combined
target remains ready. Use a test-only fault at the WAL mutation/sync boundary
for the named real-server journey; the existing disk-full-before-write hook
does not exercise ambiguous mutation.

### Public query resource failure

Add `BifrostError::QueryResourcesExhausted` to the derive-backed public
catalog with code `WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED`, HTTP 503, and
remediation to narrow the query or add capacity. Add
`QueryTerminalErrorCode::QueryResourcesExhausted` to the closed terminal
catalog. The typed `DataFusionError::ResourcesExhausted` chain, including
contextual wrappers, selects these types; do not classify by message text.

| Failure point | Wire result | Shared client and first-class SDK result |
| --- | --- | --- |
| Before stream opening/first frame | HTTP problem response 503 with the stable code; gRPC `UNAVAILABLE` status with the same code in Wyrd error details | Typed `QueryResourcesExhausted`, no partial result and no automatic retry. |
| After a query stream has begun, including after rows | Existing HTTP/gRPC stream stays open only long enough to emit one `Failed` terminal carrying `QueryResourcesExhausted`; no success/degraded terminal follows | Client rejects the whole stream as typed `QueryResourcesExhausted`, including rows already received; Rust/Python/TypeScript preserve the same stable code and do not auto retry. |
| Full 1,000-place waiting queue | Existing `QueryQueueFull` 429 before execution; only this overload keeps retry metadata | Retryable queue-full remains distinct from execution memory failure. |

`grpc/query.rs::query_status` must stop attaching `retry-after-ms` to all
503 responses: attach it only to an explicitly retryable readiness/overload
class. The new resource error carries no retry hint. Keep ordinary query
execution, timeout, peer-security, and queue-full codes unchanged. Update
closed Rust matches, generated schemas, and SDK projections; no compatibility
alias is needed because nothing shipped.

## Approach

1. Record a source-to-destination behavior ledger for TASK-006: replayed,
   already present, or superseded with a reason for every material Eval,
   startup, peer, Bifrost, contract, SDK, and journey change.
2. Replay Eval and simplified boot/peer behavior through current Bifrost
   seams; complete TASK-003's missing role-local WAL failure handling and run
   focused and real-server regressions before resource changes.
3. Resolve the default 1-GiB server minimum and one shared Bifrost cap from
   the detected process limit; remove idle role memory partitions.
4. Give Forge bounded DataFusion memory and ordinary spill under the shared
   data root; retain its parallelism and durable retry. Preserve
   Oracle's queue, per-query ceiling, spill, and child-owned cleanup.
5. Run standard and heavy benchmarks, review measured misses, then run the
   broad repository gate once after benchmark acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Eval runs through the integrated server

**Behavior.** A real client writes an Eval observation through current Gate
and Scribe. After durable ACK, eligible bindings may enqueue a run without
delaying or rolling back ACK. Sampled-in, sampled-out, trace-wait, error,
result, media, and Operator outcomes follow TASK-006 across restart. Failed
post-ACK enqueue leaves the observation readable and creates no false run.

**RED.** Replay the smallest real-client Eval journey and focused enqueue
failure check from TASK-006 onto the merged base. Confirm their behavior
fails until the integrated Eval path exists; source-branch success is not
proof on the integrated base. Use the exact Eval selectors in the proof
matrix below.

**GREEN.** Replay approved Eval behavior through the current Scribe ACK and
Oracle query contracts. Run focused Eval checks, real-server journey, and
current Scribe ingest/read-back regression.

**REFACTOR.** Delete displaced Eval plumbing so one observation callback,
one run owner, and one result projection remain.

### Scenario 2 — Local startup and remote peers have one routing contract

**Behavior.** The all target calls local Scribe, Oracle, and Forge without a
private peer hop. A second peer-enabled replica is discovered through
membership and executes distributed Oracle and remote Scribe work over mTLS
with receiver-validated typed context. Wrong credentials, stale fences, and
cross-tenant contexts fail without partial results. A failed Scribe role
does not terminate Oracle or the server's Eval worker; Eval operations that
need the unavailable Scribe surface their actual dependency failure.

**RED.** Replay TASK-006's local boot and two-replica journeys onto the
merged base. Add an integrated remote-live and Scribe-role-failure case
where the source journeys do not cover current Bifrost. Show that failed WAL
rollback still poisons the process and combined-target `/readyz` still counts
Scribe as required. The source peer lane is replayed with its `mise.toml`
entry; the exact integrated selectors are below.

**GREEN.** Adapt TASK-006 startup, public routing, readiness, peer mTLS,
and typed contexts to current Bifrost services. Move uncertain WAL health to
Scribe alone, withdraw its role without shutting down the process, and make
combined-target readiness conditional on the other required healthy roles.
Run server, peer, Oracle, and Scribe journeys.

**REFACTOR.** Remove superseded startup, routing, ticket, and replay-state
paths. Keep one local call and one authenticated remote call per operation.

### Scenario 3 — Server headroom and idle roles resolve predictably

**Behavior.** An 8-GiB detected limit defaults to 1 GiB of non-Bifrost
headroom and a 7-GiB shared Bifrost cap. The server minimum can rise and
the Bifrost cap can fall; neither setting can raise the Bifrost cap above
process limit minus server minimum. Impossible settings fail boot. Other
server work may use any RAM Bifrost has not consumed. Idle roles hold no
fixed Bifrost share. Headroom is accounting, not physically reserved RAM.
The operator settings are WYRD_SERVER_MEMORY_MIN_BYTES and
bifrost.resources.server_memory_min_bytes for the minimum, and the existing
WYRD_BIFROST_MEMORY_LIMIT_BYTES and matching resource field for an optional
lower Bifrost cap. Delete the old unmanaged-reserve setting and aliases.

**RED.** Add focused resource-plan and server-config cases for defaults,
overrides, invalid values, and idle roles. Confirm the present 256-MiB
unmanaged reserve and Scribe/Oracle/Forge partitions fail these cases. Use
the named unit selectors below.

**GREEN.** Resolve one server minimum and Bifrost cap at boot for every
serving target. Retire the old unmanaged-reserve setting and role partition
semantics without a compatibility alias.

**REFACTOR.** Delete floor/elastic arithmetic, idle-Forge reservation, and
obsolete tests and metrics. Retain role attribution only for memory actually
held.

### Scenario 4 — Concurrent Bifrost work shares actual memory capacity

**Behavior.** Scribe, Oracle, Forge, and in-flight Bifrost transport charge
one governed cap while they hold memory and return charges on completion,
failure, or cancellation. Per-message size and Forge parallelism remain
separate bounds. Forge uses a bounded DataFusion memory pool and ordinary
disk spill in the existing Bifrost data root; an exhausted attempt publishes
nothing partial and follows durable retry. An estimate
does not reserve future heap use in advance. The narrow opaque OTLP decoder
charge follows Revision 13 and transfers with its held buffer.

**RED.** Add focused concurrent-charge/release cases and a Forge
failure/retry integration case. Show that Forge's current estimated budget
and unbounded pool do not prove the shared cap. Use the named resource and
Forge selectors below.

**GREEN.** Route governed allocations through the shared Bifrost authority.
Use the compaction dependency's existing bounded-pool and spill capability,
prepare `forge-spill` through `BifrostDataRoot`, and clear its crash leftovers
under the root lock before Forge starts. Charge transport body ownership to
Bifrost rather than server headroom.
Preserve Forge's durable settlement.

**REFACTOR.** Delete Forge estimated-memory admission and its fixed 80%
budget while keeping earned FIFO/parallelism behavior. Delete transport's
dependency on server headroom and any duplicate aggregate ledger.

### Scenario 5 — Only the memory-exhausted query fails

**Behavior.** A queued query holds no execution memory. If a running query
cannot make a fallible allocation within its ceiling and shared budget, its
operator spills or that query ends Failed; a sibling completes and a queued
query then runs. The failing query's children end before memory and slot
return. An admitted execution-memory fault has a typed resource-exhausted
reason, never admission rejection or queue-full. DataFusion infallible
growth is accounted as headroom and released without a new cancellation
policy. The public error is WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED, with
no automatic retry of the same query in first-class SDKs.

**RED.** Add a concurrent-query real-server failure journey and focused
error-mapping and infallible-growth cases. Confirm the current resource-to-
admission mapping fails the public distinction. Use the named Oracle,
server, and shared-client selectors below.

**GREEN.** Keep existing actual-growth accounting, per-query ceilings,
spill, slots, and queue. Correct the admitted-query error through server and
first-class clients. Ensure query-owned children finish before capacity
returns; account and release infallible headroom.

**REFACTOR.** Delete the incorrect execution-to-admission mapping and the
proposed grow watchdog/cancellation path. Queue-full remains exclusive to
an actually full waiting queue.

## Acceptance Criteria

1. The packet-local replay map is followed and the execution ledger records
   each material TASK-006 Eval, boot, peer, Bifrost, contract, SDK, and journey
   change as replayed, already present, or superseded by current Bifrost with
   the named owning seam. One startup and peer path remains; integrated local
   and cross-pod journeys satisfy both specs. The missing TASK-003 WAL boundary
   is completed: uncertain WAL stops only Scribe, combined-target readiness
   reports Scribe unready while healthy independent roles keep serving, and
   shared-governor poison remains process-terminal.
2. Default 8-GiB detection yields 1-GiB server headroom and a 7-GiB
   governed Bifrost cap. Overrides and invalid boot are proven. Server
   work has no application cap. Idle roles reserve nothing; concurrent
   fallible governed charges respect one total.
3. Forge spill is confined to the existing data root, with no Forge-specific
   scratch cap, and crash leftovers are removed before Forge restarts. Its normal
   temporary files are released by DataFusion. Forge failure cannot partially
   publish. Query memory exhaustion fails only its requesting query with a
   distinct public reason before and after
   stream opening, without retry metadata. Infallible and untracked
   allocations are documented honestly, not claimed OOM-proof.
4. Every approved standard and heavy workload has valid measured evidence
   meeting Bifrost REQ-008/AC-010. Invalid or wrong-result rows cannot pass.
5. Focused checks, generated contracts, and the broad gate pass after
   benchmark acceptance. PR #94's CI override is not completion evidence.
6. Revision 13's AC-013 holds: Scribe uses one actual-byte memory owner,
   acknowledged writes stage and read back across retry/restart, occupied
   storage permits wait within the caller's bound, and only leaders retry
   explicit pre-accept peer capacity. The R13-A–D focused and real-server
   scenarios pass before the benchmark.

## Expected Write Set and Consumer Closure

Likely owners: crates/wyrd/wyrd-server/src/boot, config.rs, role and peer
services, verification; crates/vala/vala-bifrost-redux/src/resources.rs,
Scribe ingress/transport, Oracle query/error mapping, Forge worker/execution;
crates/wyrd-spec/src/vala, shared client and first-class SDK projections,
and real-server journeys under crates/wyrd/wyrd-testing. Update
architecture/bifrost-design.md and operator docs to replace old role-floor,
Forge-estimate, and ticket descriptions. Paths guide ownership and are not
an implementation allowlist. Regenerate public artifacts from source.

For the public error, the concrete closure is
`crates/wyrd-spec/src/vala/{error.rs,api.rs}`,
`crates/vala/vala-bifrost-redux/src/oracle/{mod.rs,query_stream.rs}`,
`crates/wyrd/wyrd-server/src/{query/routes.rs,grpc/query.rs,http/error.rs}`,
`crates/shared/wyrd-client/src/{error.rs,bifrost/query.rs}`, and the Rust,
Python, and TypeScript client error projections. For memory, modify the
existing root in `resources.rs`, encoded-body owner in `gate/limits.rs`, and
Forge context in `forge/managed/executor.rs`, and the existing
`boot/data_root.rs` for its spill child; do not create a second root. Complete
the Scribe WAL boundary through `scribe/wal.rs`, the Scribe role lifecycle,
and the existing server readiness snapshot. Revision 13 additionally owns
`scribe/{admission,contention,geometry,material_plan,ingress,persistence,parquet_writer}.rs`,
`storage/mod.rs`, `oracle/{mod,dispatcher}.rs`, and the server OTLP decode
boundary. The revision's owner table gives the symbol-level deletions.

## Verification and Evidence

The table fixes the required proof and its owner. New names are the required
planned selectors in the existing crate targets; confirm their registration
with `mise exec -- cargo nextest list` after adding them, then run the exact
commands below. Fixtures remain owned by their test modules. An integrated
journey must use a real client and server; a unit case cannot replace it.

| Test and tier | Setup → action → assertions | Acceptance |
| --- | --- | --- |
| Existing `server::eval_verification::continuous_eval_runs_the_terminal_matrix` journey from TASK-006 | Real client/server and Postgres: emit Eval observation, run verifier, query run/result; cover sampled in/out, error, media, Operator, and persisted terminal; no duplicate run on replay. | 1 |
| New `server::eval_verification::integrated_enqueue_failure_preserves_ack` journey | Real Gate/Scribe/Eval; force post-ACK enqueue failure, read the acknowledged observation, and prove no run/result was invented. Also retry same batch ID and prove no second enqueue. | 1 |
| Existing source `peer_network::join::peer_join_and_remote_query` and `peer_network::security::peer_context_refusals` journeys | Two real server processes: peer joins over mTLS, remote query succeeds; bad peer identity and wrong tenant/context/fence fail without result. | 1 |
| New `peer_network::analytical::remote_live_scribe_drop_releases_query` journey | Remote Scribe on another process, leader opens a live fragment then client disconnects; prove remote stream/lease release and leader deadline behavior, including one failed terminal after rows if remote Scribe fails. | 1 |
| New `server::owner_inspection::scribe_wal_fault_is_role_local` journey | Combined target with Eval worker: expose a narrow test-only WAL fault that leaves mutation/sync uncertain after a durable write; new write gets no ACK, Scribe is unready and withdrawn, combined `/readyz` stays ready while its body reports Scribe unready, Oracle published read and Eval worker remain healthy; restart recovers WAL before Scribe ready. A separate shared-governor poison still shuts down the process. The disk-full-before-write hook cannot substitute for this fault. | 1 |
| New `resources::tests::shared_cap_defaults_overrides_and_concurrent_charges` unit | Inject 8-GiB observation, exercise default and operator limits, idle roles, simultaneous Scribe/Oracle/Forge/transport charges, failed charge, release, and re-admission; assert 1-GiB headroom, 7-GiB cap, no partition or precharge, and aggregate invariant. | 2 |
| New `config::tests::server_memory_minimum_rejects_impossible_plan` unit | Parse env/config combinations including impossible minimum/cap and removed unmanaged setting; assert one resolved boot plan or boot error, no alias. | 2 |
| New `forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel` unit plus `forge::live_rewrite::failed_memory_attempt_retries_without_partial_publication` journey | Hold Oracle/Scribe charges; run Forge bounded DataFusion growth and spill, force exhaustion/cancel, prove root returns bytes only after work ends; assert normal temporary-file release under `forge-spill`. Real Forge task retries and publishes one complete snapshot, never a partial one. | 2, 3 |
| New `boot::data_root::tests::prepare_clears_stale_forge_spill` unit | Start from a stale file in `forge-spill`; prepare the shared root and prove its lock is held, the file is removed, and WAL, staging, and Oracle files remain untouched. | 3 |
| New `oracle::capacity::memory_failure_is_query_local_and_typed` journey | Two running queries plus one queued: force one admitted query's fallible DataFusion allocation failure after rows; sibling finishes, failed stream has one typed Failed terminal, queued query runs only after children and memory release; server stays healthy. | 2, 3 |
| New `grpc::query::tests::resource_failure_has_no_retry_hint` unit and `bifrost::query::tests::resource_terminal_rejects_partial_rows` unit | Map pre-stream 503 and post-stream Failed terminal; assert stable code, no `retry-after-ms` for resource failure, no partial client success, and queue-full remains 429/retryable. | 3 |
| Revision 13 R13-A and R13-B focused units plus `write_read::acknowledged_rows_survive_stage_pressure_and_restart` journey | Prove one Scribe held-byte owner and no second breaker or future-byte hold-back; ACK, live read, stage retry, restart, published read, and WAL retirement. | 6 |
| Revision 13 R13-C focused units plus `peer_network::analytical::two_leaders_retry_preaccept_capacity` journey and R13-D dispatcher unit | Prove storage permit waits within the original bound, actual footer charge, receiver running slots, leader-only explicit pre-accept retry, and no ambiguous replay. | 6 |

Exact focused commands (run sequentially; each new test is added to the
named existing module/target before invoking its selector):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::continuous_eval_runs_the_terminal_matrix)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=eval_verification::integrated_enqueue_failure_preserves_ack)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::join::peer_join_and_remote_query)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::security::peer_context_refusals)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::analytical::remote_live_scribe_drop_releases_query)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=owner_inspection::scribe_wal_fault_is_role_local)'"
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=resources::tests::shared_cap_defaults_overrides_and_concurrent_charges)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::server_memory_minimum_rejects_impossible_plan)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::data_root::tests::prepare_clears_stale_forge_spill)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E 'test(=live_rewrite::failed_memory_attempt_retries_without_partial_publication)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=capacity::memory_failure_is_query_local_and_typed)'"
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=grpc::query::tests::resource_failure_has_no_retry_hint)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::resource_terminal_rejects_partial_rows)'
```

The `test:server:peer` entry is part of the pinned TASK-006 `mise.toml`
replay and must run after integration. The source peer journey selectors above
are additional focused proof, not a replacement for that lane. Run the
existing Python and TypeScript Eval and Bifrost journeys after shared-client
projection; their current runtime-owned lanes remain authoritative. Existing
supporting lanes are:

    mise run test:bifrost:integration:redux
    mise run test:bifrost:integration:server
    mise run test:bifrost:journey:scribe
    mise run test:bifrost:journey:oracle
    mise run test:bifrost:journey:server
    mise run test:bifrost:journey:python
    mise run test:bifrost:journey:typescript

Run `mise run test:server:peer` and the source TASK-006 Eval, SDK, and peer
checks on the integrated base. The related approved spec requires real Rust,
Python, and TypeScript Eval journeys. Static docs checks need no manufactured
RED; executable boot and memory behavior is covered above.

After all scenarios pass, run the existing standard benchmark first:

    mise run bench:bifrost:query-capacity

Then run the separate 100M-row heavy-scan qualification:

    WYRD_BENCH_HEAVY_SCAN=1 mise run bench:bifrost:query-capacity

Both run one at a time on this Linux host. The existing local process_cluster
child gets 4 CPUs/8 GiB through systemd; the public client and PostgreSQL
are outside that child limit. Docker is only for repository-managed
PostgreSQL. Save raw samples, reports, logs, refusal reasons, CPU evidence,
peak cgroup memory, ingest throughput, and scan geometry. Present misses,
fix their measured cause, and rerun affected windows until targets pass.
Do not run the broad gate before benchmark acceptance.

Then run, one at a time:

    mise run fmt
    mise run lints
    mise run codegen:check
    mise run docs:check
    mise run gate

If replay or contract changes touch Python files, also run mise run
py:format and mise run py:lints before gate. Run the corresponding owning
SDK journeys and typing checks when public Python or TypeScript contracts
change.

Gate includes Bifrost verification; do not duplicate verify:bifrost.
Record command exits, an acceptance-to-evidence table, the replay ledger,
and benchmark comparison. A green gate with failed capacity evidence does
not complete this task.

## Material Stop Conditions

- Stop if Eval replay changes approved post-ACK best-effort delivery,
  verdict/result semantics, tenant/media authority, or durable run lifecycle.
- Stop if peer replay weakens mTLS, receiver context checks, tenant
  isolation, or remote failure semantics. Do not keep both ticket and mTLS
  protocols as a shortcut.
- Stop if memory work moves ACK earlier, loses retained WAL/read authority,
  returns query capacity before child work ends, or substitutes Forge
  estimates for governed execution.
- Stop if Revision 13 adds a second memory ledger or capacity refusal,
  charges one held buffer twice, makes acknowledged staging permanently
  unstageable, or retries peer work that may already have been accepted.
- A benchmark miss is a failed outcome to diagnose, not permission to lower
  the target, remove the workload, or substitute the gate.

## Authority Links

- Approved Bifrost spec revision 13: ../spec.md
- Approved verification contract revision 44 at pinned vcc/task-006 commit
  `0e9c6e98c74361b1f2d18dd857a2aff7690cfdb0` (the local
  [verified-change-contract spec](../../verified-change-contract/spec.md)
  is still revision 38 until replay).
- Continuous Eval source task:
  ../../verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md
- Repository rules: ../../../../AGENTS.md
- Bifrost architecture: ../../../../architecture/bifrost-design.md
- Wyrd protocol: ../../../../architecture/wyrd-design.md

## Revision 13 Implementation Evidence

### Diagnoses

**D1 — follower pool poisoned Scribe attribution.**
- Symptom: `ingest_bounds::forty_mib_row_stages_publishes_and_reads_back_at_48_mib_wire` poisoned the governor.
- Evidence: `Scribe root attribution does not reconcile to live ownership category_total=41947396 shard_total=0 held_total=83892491 scribe_memory_used_bytes=83892337`.
- Cause: the one-pool follower view charged `MemoryHolder::Scribe` without a Scribe category, so held Scribe bytes exceeded category attribution.
- Fix site: `ScribeResources::follower_memory_pool` (`resources.rs`). Follower views are query execution and now charge the query holder like every other query view. Scribe reconciliation is unchanged. The only other caller of the one pool is Forge, which already uses its own holder.

**D2 — the live-lifetime journeys asserted the deleted precharge** (read-only diagnostician report).
- Symptom: `distributed::live_stream_backpressure_and_query_owned_lifetime` failed with "held past 30 seconds … (producers=1)". `peer_network::analytical::remote_live_scribe_drop_releases_query` failed with "held 1 producers and 13260 bytes over a 13260-byte baseline".
- Evidence:
  - `follower_lease_held` / `lease_held` required `scribe_memory_used_bytes >= baseline + ORACLE_PARTITION_MEMORY_BYTES / 2` (`distributed.rs:1215`, `analytical.rs:1403`).
  - The producer was held in both runs.
- Cause: the tests encoded the 256 MiB follower precharge that TASK-003/R13 delete. A paused follower legitimately holds about 0 bytes on the one pool.
- Fix site: the test helpers. "Held" is now one open producer plus the leader's admitted query. "Released" is zero producers, zero follower query bytes on the Scribe pod, and zero Oracle admission. The `LiveScribeHolds` control reply carries follower query bytes.
- Callers checked: `limit_stops_unneeded_live_fragment_without_footer` (same release helper).
- Verification: all three journeys pass.

**D3 — stale compressed-first-frame test.**
- Symptom: `grpc::tests::a_compressed_first_frame_is_bounded_as_unknown` expected `Ok(None)` and got `Ok(Some(4))`.
- Evidence: 61018283a changed the gRPC head's `declared()` to charge flags `0 | 1` alike and removed `try_acquire_unknown`.
- Cause: the test still expected the pre-change contract. The design says transport charges the encoded wire body (`bifrost-design.md` §admission; this task's memory charge contract), and tonic's decode limit bounds decompression.
- Fix site: the test only. It is renamed `a_compressed_first_frame_is_bounded_by_its_encoded_length` and asserts `Ok(Some(4))`.

**D4 — explicit peer transport loss in the R13-D fixture** (recorded by the R13-C/D implementor).
- Symptom: `peer_loss_is_one_terminal_attempt` could not tell transport loss apart from a pre-accept capacity refusal after leader-only retry landed.
- Cause: the dispatcher fixture had no transport-loss mode.
- Fix site: the fixture gained an explicit transport-loss mode. Production dispatch is unchanged.

**D5 — a failed stage attempt wedged its member forever** (read-only diagnostician report).
- Symptom: after an R13-B stage refusal (`ingest busy for table: memory`), restart failed with `WAL recovery failed … staged member 0-1 already occupies its durable directory`.
- Evidence: `ScribeMemberStager::encode_runs` refused whenever the member directory existed. R13-B moves the memory charge inside `encode_batch`, after `create_dir_all`, so a refusal leaves a directory with no record. `ScribeHotStage::recover` skips record-less directories by contract.
- Cause: only a published `member.staged.json` authorizes WAL retirement, but the refusal treated record-less residue as authority. Every in-process retry and every WAL-replay rebuild of that member was refused.
- Fix site: `encode_runs` (`member_stager.rs`). It is the only code that creates member directories and is reached by both live rotation and replay via `staging_runtime::encode_member`. It now refuses only when a record exists and otherwise removes the residue before encoding.
- Concurrency: the shard submits one encode per member at a time, and retry follows settlement. Recovery is unchanged.
- Tests:
  - `restaging_an_existing_member_is_refused` protected a false invariant: it re-encoded without publishing a record. It now publishes the record first.
  - `unrecorded_member_residue_is_reclaimed_by_the_retry` pins the retry.

**D6 — the two-leader journey miscounted leader graphs** (read-only diagnostician report).
- Symptom: `two_leaders_retry_preaccept_capacity` failed with "leader 1 never registered its graph", holding `leader_graphs: 2, follower_graphs: 1`.
- Evidence:
  - Each node has one `AnalyticalSupervisor`, and follower ingress registers followed graphs there too (`oracle/analytical.rs` `register_graph`).
  - `leader_graphs` reads `supervisor.live_graphs()`.
  - Leader 1 follows leader 0's held graph by design (`RETRY_LEADER_SLOTS` doc).
- Cause: the new test expected `leader_graphs == 1`. The node's own graph is `leader_graphs - follower_graphs`.
- Fix site: the test's two checks now compare `leader_graphs == follower_graphs + 1`, and the `OracleOwnershipSnapshot::leader_graphs` doc now states what it counts. Production is unchanged. Refused rounds release every remote reservation before waiting.
- Residual: two leaders with tight per-node limits can starve each other until their deadlines. This is a livelock bounded by the deadline, not a deadlock. It is admission sizing, outside R13.

**D7 — an explicit flush skipped retained generations** (read-only diagnostician report).
- Symptom: after the root occupant was released, a second `flush_bifrost` published 0 of 300,000 acknowledged rows.
- Evidence: `fail_persistence_completion` → `mark_front_retryable` leaves the front unsubmitted. `ShardOwner::flush_all` only froze active keys, and the frozen key is neither active nor "strandable". Only `flush_expired` (the age tick) called `retry_pending`.
- Cause: explicit flush and the shutdown drain could not re-drive what a failed stage attempt left retryable.
- Fix site: `ShardOwner::flush_all` calls `retry_pending` first.
  - `submit_front` submits only an unsubmitted front, so FIFO order holds and an in-flight front is never submitted twice.
  - The pressure flush and replay are unchanged.
  - The shutdown drain gains one retry inside its deadline.
- Test: `scribe::shards::tests::flush_all_resubmits_a_retained_generation`.
- Follow-up (existing gap, not widened): a failed replay-owned front stays queued without its chunk. Recovery fails first today.

**D8 — `lints:default` was red.**
- Symptom: `cargo clippy --workspace --all-targets -D warnings` failed on `scribe/shards.rs` `next_prepared_slice` (`unnecessary_wraps`). Plain crate builds also warned on 11 items used only by test or test-support code.
- Cause:
  - `next_prepared_slice` kept a dead CPU-pool parameter and a `Result`, but only popped a materialized slice.
  - `OracleAdmission.slots` duplicated the slot manager that boot already hands to `ReservationRegistry`, and only the test readiness snapshot read it.
- Fix site:
  - `next_prepared_slice` returns `Option` and drops the dead parameter and the unreachable error branch.
  - Deleted: `OracleAdmission.slots`, its `with_config` parameter, and `OracleBuildConfig.local_slots`. The readiness snapshot reads `ReservationRegistry::total_slot_units`.
  - The engine's `reservations`, the staging and metadata snapshots, and the tail `produced` pause flag are gated to their only (test-support) readers.
  - Imports used only by gated code are path-qualified at the use site.

### Revision 13 acceptance evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R13-A: one Scribe held-byte owner, no secondary ceiling | aacd0bd27 | `scribe::admission::tests::one_root_charge_has_no_secondary_memory_ceiling`, `scribe::ingress::tests::decode_to_memtable_transfers_one_charge` | PASS |
| R13-B: ACKed rows survive stage pressure, retry, publish once, WAL retires, survive restart without duplicates; no future writer/footer/workspace admission | `parquet_writer::append_charged_batch`, `persistence::publish_claim` (one transfer chunk), `member_stager::encode_runs` (D5), `shards::flush_all` (D7) | `write_read::acknowledged_rows_survive_stage_pressure_and_restart`; `member_stager::tests::*`; `shards::tests::flush_all_resubmits_a_retained_generation` | PASS |
| R13-C: storage permit waits within the operation deadline; no fixed footer slot | 65da92a03 | `storage::governed_request_tests::{occupied_storage_permit_waits_within_operation_deadline, footer_decode_has_no_fixed_memory_slot}` | PASS |
| R13-D: only the leader retries pre-accept peer capacity; receiver never exceeds its running slots | 65da92a03, D6 | `oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`; `oracle::analytical::tests::cancellation_during_peer_capacity_wait_stops_retry`; `peer_network::analytical::two_leaders_retry_preaccept_capacity` | PASS |
| Lint lanes | D8 | `mise run lints:default`, `mise run lints` | PASS |
| Crate lib suite | — | `vala-bifrost-redux --lib --features test-support`: 848/848 | PASS |

### D9 — selective first-row cost at one client (benchmark miss)

- **Symptom:** standard benchmark selective@1 p50/p95/p99 24.3/28.2/41.1 ms,
  server `first_row` mean 17.6 ms, while selective@8 `first_row` was 3.7 ms.
  Reproduced on a second run (24.4/27.9/41.0 ms), so not host variance.
- **Evidence:** selective@1 window metrics: 3.07 MB scanned per point lookup,
  one row group scanned and nine pruned per query, all via
  `HotParquetExec` (`bifrost_storage_metadata_cache_effects_total{hit}` one
  per query). From selective@4 onward Forge `scribe_promotion` had published
  the fixture, and hot files were pruned by `snapshot_overlap`; those windows
  read through `OracleIcebergScanExec`, which already uses page-index row
  selection. That path switch is why the cost fell as concurrency rose.
- **Cause:** `hot_stream` restricted the reader to retained row groups only.
  A point lookup decoded the whole 1,048,576-row group, about 122 batches, for
  every projected column. Cached hot metadata was decoded without the page
  index, so page-level selection was not possible.
- **Fix site:** `storage::BifrostStorage::decode_metadata` (the sole hot
  metadata decode) now loads the page index when present. `oracle::exec`
  adds `select_pages_for_predicates`, which reuses row-group pruning's
  exclusion decision (`leaf_excludes_span`) on page-index bounds. `hot_stream`
  applies the resulting `RowSelection`. The row-group caller
  (`select_row_groups_for_predicates`) and the follower resolver share the
  same decision function. No test assertion, timeout, retry, or concurrency
  was touched, so no diagnostician was required.
- **Result:** two post-fix standard runs: selective@1 8.8–9.0/10.6–11.0/
  12.4–12.7 ms, server `first_row` 2.8–2.9 ms, 0.31 MB scanned per query;
  remote-live 137→333 QPS; every row exact (0 wrong).

### Residual misses after D9 (measured, not yet fixed)

- **Selective capacity ~760–780 QPS vs 1000 floor; latency targets 2/5/10 ms.**
  At saturation the Iceberg path costs ~5.1 ms CPU per lookup on 4 CPUs.
  A perf profile of the selective@8–16 window gives zstd 25%, libc
  copy/alloc 18–22%, DataFusion 8.5%, parquet metadata/thrift 6%, and kernel
  6.5%. Offline on the real fixture, one lookup costs 0.18 ms of footer plus
  page-index decode and 1.0–1.2 ms of page decode, including the 1 MB
  `event_id` dictionary page. The scan is therefore ~1.4 ms of the ~5.1 ms.
  Server phases at c1 are `snapshot_pin` 4.8 ms, split into `table_lookup`
  1.7, `hot_cut` 1.6, `metadata_pointer` 0.7 and `revalidation` 0.7
  (sequential Postgres round-trips), then `first_row` 2.8 ms. The 2 ms p50
  target is below the catalog pin alone.
- **small-aggregate c32/c64 p95 107–110 / 210–219 ms vs 100 ms.** The cgroup is
  CPU-saturated at ~340 QPS. Admission wait is 55/149 ms mean, and closed-loop
  c64 at a 100 ms p95 would need about 640 QPS. Same as baseline.
- **table-aggregate p95 301–338 ms vs 300 ms** (run-to-run spread). A single
  published file's ten row groups decode sequentially in one Iceberg task
  (1.3 cores), with ~267 ms server `first_row`.
- **Seed ingest:** 557k rows/s on the first R13 run versus 676k and 681k on
  reruns of the same code, so it is host variance.

### D10 — one execution path and one slot per query (spec revision 17)

User-approved consolidation ("THERE SHOULD BE 1 CORRECT way to do something";
"roll this deletion in with all other consoldiation work"; "yes. drop it").
This supersedes this task's row at line 99 that kept `try_acquire_worker` and
"fragment and graph leaders": no fragment leader exists in production.

- **Deleted, Oracle fragment-worker path:** no production leader dispatched
  Oracle-target fragments. Deleted `try_acquire_worker`,
  `OracleWorkerResources`, `OracleWorkerClass`, `OracleSlotCharge`, the
  attempt buffer (`oracle/attempt.rs`), the follower reader-authority install
  (`FixedCohortResolver`, `install_reader_authority`, `AuthorityAlreadyInstalled`),
  `PeerTicketClaims::execution_deadline`, `NoopPeerSecurityAudit`,
  `FragmentTelemetry`, `record_slot`, `record_security`, `admission_limits`,
  the `oracle_fragment_duration_seconds` bucket, and the harness
  `fragments_active` gauge. The server refuses an Oracle-target fragment as
  terminal.
- **Deleted, class-weighted slot charge:** every query charges one unit on each
  node it runs on. Deleted `ANALYTICAL_QUERY_SLOT_UNITS`,
  `ANALYTICAL_GRAPH_SLOT_UNITS`, `OracleResourceRequest::{memory_bytes,
  slot_units}` and their exact-quantum validation, the separate
  `oracle_query_slot_units` / `oracle_analytical_slot_units` ledger (the live
  query counters are the ledger), `charge_oracle_slots` /
  `release_oracle_slots`, `AdmissionClass::slot_units`, and the below-two-unit
  fold in `OracleClassSplit::derive` and calibration translation.
  `max_concurrent_graphs` is the local slot total.
- **Deleted, duplicate encodings:** `AdmissionClass` (admission uses the
  derived `QueryClass`), `PendingReservation::query_class`,
  `CommittedGraphActivation` (`PendingGraphActivation::commit` returns the
  stream directly), `proposal_u32_allowing_zero`, and the unreachable
  disabled-Analytical calibration branch. An Analytical tenant cap is now
  `1..=analytical_slots`.
- **Wire:** `ReserveNodeSlotsRequest.slot_units` removed (proto tag 5
  reserved); `graph` is required (missing → `PrivateConversionError::Missing`).
  `query_class` is removed (tag 4 reserved): only Analytical graphs reserve
  peers, so the field carried no information. The persisted audit `BifrostQueryReadDecision.slot_units`
  is kept and records `1`, so existing audit records' canonical hashes still
  verify.
- **Fixtures:** process journeys that sized a pod as "one two-unit graph plus
  the Interactive floor" now use 2 units (`RETRY_RECEIVER_SLOTS`,
  activation topology). Unit fixtures that saturated the Analytical class with
  one query use `analytical_slots: 1`.
- **Selector:** deleting the fragment-worker path removed
  `oracle::dispatcher::tests::leader_retries_only_preaccept_peer_capacity`.
  R13-D is proven by
  `oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`
  (retry within the deadline) and
  `oracle::analytical::tests::cancellation_during_peer_capacity_wait_stops_retry`
  (cancellation during the wait stops the retry).

### D11 — the query class is the execution path

User-approved ("consolidate QueryClass and QueryExecutionPath and delete
query_sql_inactive_analytical"). Oracle installs the distributed planner only
when it composed the Analytical handle (peer mode), so a `DistributedExec` root,
and therefore `QueryClass::Analytical`, implies the distributed path runs.

- **Deleted:** `QueryExecutionPath` (spec and proto enum). The terminal field
  is `query_class: QueryClass` (proto field 8, same numeric values; JSON and
  MCP key `query_class`). An Analytical class without a handle now fails with
  `OracleRoleUnavailable` rather than running locally under the wrong class.
- **Deleted:** `Oracle::query_sql_inactive_analytical` and the `analytical`
  override threaded through `run_sql_query`, `SqlAttemptInput`, and
  `ClassifyInput`; journeys call `query_sql`. The attempt identity is derived
  exactly when the class is Analytical.
- **Renamed:** the stale "inactive" Analytical vocabulary in Oracle docs and
  the process-cluster harness (`ExecuteSql`, `StartSql`, `CancelSql`,
  `AwaitSql`, `lease_analytical_attempt`, `analytical_lifecycle.rs`).
- **Fixed:** the checked-in `wyrd.v1.bin` descriptor snapshot was stale since
  the D10 wire change; regenerated (`check:proto-drift`).

### D12 — duplicate-path sweep

An independent read-only sweep found the remaining second ways to do one thing;
each was deleted or routed through its production owner.

- `Oracle::lease_analytical_attempt` now runs production preparation,
  classification, and `admit_built_attempt` instead of hand-finalizing an
  Analytical cut; it no longer takes a caller-built attempt identity
  (`support::attempt_context` deleted). The attempt deadline conversion is one
  `instant_deadline` helper.
- `OracleSlotManager` deleted: `ReservationRegistry` holds the local slot
  total directly.
- `ResourceState.oracle_active_queries` is derived from the two class counters
  rather than stored beside them; the public snapshot field is unchanged.
- `OracleQueryClassLabel` replaced by one `query_class_label` function;
  `exec.rs` `NoopAudit` replaced by `AcceptingOracleAudit`.
- Dead test-support deleted: `TestPostgresOracleAudit`, `OracleTopologyProbe`,
  `analytical_plan_failure_armed_for_test`, `deadline_projection_for_test`
  (replaced by a unit test of `projected_request_deadline`).
- Harness: `ControlRequest::ExecuteSql` deleted; `execute_sql` is
  `start_sql` + `await_sql`. The Analytical baseline reads its grant from
  `oracle_query_memory_limit` instead of acquiring and dropping an envelope.
- Kept deliberately: the per-call frame-decode loops (transport, fold, and
  partial-drain shapes differ, so a shared helper would add generics without
  removing a path), and the two-field slot sums in admission and validation.
- The Analytical audit topology was decided in D13.

### D13 — non-blocking read audit and Analytical topology

Decided by the user: the read-decision audit is fire-and-forget, and an
Analytical decision records its real topology.

- `OracleAudit` methods are synchronous and return `()`. The writer
  (`OracleQueryAudit`) already stages through its tracked task and counts
  commit failures in `oracle_audit_commit_failures_total`; the Oracle-side
  timeout and error mapping were deleted, and `audit_and_bind` is synchronous.
  Building the locked detail stays fail-closed. The tenant tripwire stages its
  security event and still refuses with `QueryTenantInvariant`.
- Analytical records `Distributed`, `selected_node_count` = the frozen
  participant cut's Oracles (leader included), and `worker_count` = that − 1.
  Interactive stays `Local`/1/0. The persisted schema is unchanged. Unit test:
  `oracle::tests::analytical_audit_topology_counts_the_frozen_cut`.

Oracle journey failures after D12 (diagnosed by a fresh read-only diagnostician):

- **Symptom:** `pg_analytical_stale_attempt_and_sibling_graph_cannot_emit_or_cancel`
  and `pg_analytical_raw_sql_proves_pushdown_exchange_and_qualified_spill`
  returned `OracleRoleUnavailable` from `lease_analytical_attempt`.
  **Evidence:** the lease SQL was `SELECT id ... ORDER BY id` over one hot file;
  `AnalyticalCutTaskCount::new` yields one task and no shuffle, so the root is
  not `DistributedExec`. **Cause:** D12 made the lease use production
  classification, and that SQL classifies Interactive. **Fix site:** the tests
  now lease the grouped statement that distributes.
- **Symptom:** `two_tenants_make_bounded_progress_across_query_classes` saw a
  second Analytical query admitted. **Evidence:** boot split
  `interactive_floor_units=1 analytical_max_units=3`; one unit per query.
  **Cause:** the sub-proof kept the deleted two-unit premise. **Fix site:** the
  sub-proof parks `SCHEDULING_HOLDS - 1` Analytical queries, proves the next
  one queues to its deadline, and proves both tenants' Interactive floor
  queries still run.

After the lease SQL fix, both lifecycle tests failed at settle (second
diagnostician):

- **Symptom:** `ownership.settle` returned `Internal` "Oracle analytical graph
  cleanup did not complete". **Evidence:** "leader graph was not confirmed
  drained within its deadline … memory_bytes=161"; the governor showed
  `oracle_total=161` until the deadline. **Cause:** a production bug. Only the
  stream path (`take_analytical`) cleared the guard's physical projections, which
  are children of the graph's own pool, before handing the guard to the graph.
  `lease_analytical_attempt` handed over a guard that still held them, so the
  graph waited on itself. **Fix site:** `AnalyticalSupervisor::retain_admission`,
  the single transfer seam, now calls
  `AdmittedQueryGuard::release_physical_projections`; `take_analytical` lost the
  side effect. Callers checked: `settle_analytical` (unchanged behavior), the
  lease seam (fixed), and the `analytical.rs` unit test (no projections).

### D14 — spill-merge fan-in cap deleted (spec revision 18)

- **Symptom:** `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
  intermittently failed with `QueryResourcesExhausted`: a 1.5 GiB query held
  an in-memory `ExternalSorterMerge[0]` of 673 MB and a spill
  `ExternalSorterMerge[1]` of 625 MB.
- **Cause:** 1a6ec2554 added `spill_merge_fan_in`, turning a file count into
  bytes with a fixed 64 MiB-per-file guess. DataFusion seats two buffers per
  file, so the cap over-reserved; any fixed guess is wrong for data shapes
  Bifrost cannot know. The cap also contradicted the user's recorded "do not
  add the cap for now"; DataFusion's defaults are bounded only by the
  per-query pool.
- **Decision (user, 2026-09-30):** delete the cap and keep DataFusion's defaults; a query
  exceeding its memory limit fails with a typed resource error.
- **Change:** `spill_merge_fan_in`, `SPILL_MERGE_FILE_BUDGET_BYTES`, their two
  tests, and `build_query_runtime`'s memory-limit and partition parameters are
  deleted. `bifrost-design.md`, `datafusion.md`, and the spec now state the
  per-query limit as the only bound.
- **Residual:** DataFusion frees `sort_spill_reservation_bytes` before a
  non-spilled partition's in-memory merge, so an over-limit sort can fail
  nondeterministically. Under the decision, that failure is the typed error,
  not a defect.
- **Capacity journey follow-up:** `lowest_rung_analytical_contention_preserves_two_interactive_tenants`
  proves the Interactive floor under a live Analytical query. It no longer runs
  the over-grant wide-key sort: the contending query is a distributed join and
  grouped aggregate that fits its grant, and the unpulled 300k-row result keeps
  it holding its envelope across both Interactive windows. The result-digest
  and spill checks were duplicates of the peer-network baseline, their one
  owner, and were removed. Passed 3/3 alone. The typed over-limit contract is
  already covered by `memory_failure_is_query_local_and_typed` and
  `memory_refusal_preserves_oracle_health_and_next_query`.
- **Peer baseline follow-up:** the baseline no longer forces an over-grant
  sort. Deleted: the 6 KiB key filler, the sort-input lower bound, the
  grant-equality and spill-counter assertions, `QueryStyle.spills`, and the
  child's unused `batch_memory_bytes`, `granted_memory_bytes`, and scratch
  before/after evidence. The test is renamed
  `baseline_executes_join_group_sort_and_interchangeable_topology`. Spill
  confinement stays proved by
  `pg_analytical_raw_sql_proves_pushdown_exchange_and_qualified_spill`.

### D15 — heavy benchmark OOM: cgroup detection read the hierarchy root

- **Symptom:** the heavy benchmark was OOM-killed at 8 GiB during the
  100M-row write on every run.
- **Evidence:** boot logged host RAM (91 GiB) as the memory bound and 32 CPU
  workers under the benchmark's systemd scope.
- **Cause:** memory and CPU detection read only `/sys/fs/cgroup/memory.max`
  and `cpu.max` at the hierarchy root. Under a Kubernetes cgroup namespace the
  root is the pod; under a systemd scope the limit sits on an ancestor slice.
- **Fix site:** `resources::MemoryCgroup` and `detect_cgroup_cpu` resolve the
  process's own cgroup from `/proc/self/cgroup`, walk its ancestors, and take
  the tightest `memory.max`/`cpu.max`; usage is read from the bounding group.
  Scribe's duplicate cgroup readers were deleted (db4bddd34). Test:
  `resources::tests::memory_cgroup_is_the_tightest_limit_above_the_process`.

### D16 — heavy benchmark OOM: publishing blocked age and pressure seals

- **Symptom:** with D15 fixed, boot detected 4 CPUs and 7 GiB, and governed
  Scribe bytes still grew about 200 MB/s until the OOM.
- **Cause:** d5dd6f0f8 made the 1 s lifecycle tick await `publish_due`, which
  awaits a claim merge (25+ s for about 200 staged runs). `check_age` (age
  seals, pressure seals, gauges) did not run for that time.
- **Fix site:** `app::server` runs `check_age` and `publish_due` on separate
  loops; the publisher races shutdown (dfc90f393). Verified: redux lib
  811/811, `test:bifrost:integration:redux` 874/874,
  `test:bifrost:journey:scribe` 20/20, `test:bifrost:journey:server` 26/26.

### Spec revisions 22 and 23 (user-approved 2026-10-01)

- Revision 22: reads while writing are judged against each case's absolute
  target; the read-only p95 is reported beside it (c61552ed1). Reads and
  writes share 4 CPUs, so the former 20% rise measured CPU sharing.
- Revision 23: selective latency is judged at one client and a 600 qps floor
  at the sweep's busiest point (364e5d8dd). One client reaches p50 6.9 ms at
  143 qps; 4 CPUs saturate near 660 qps.
- Report logic test: `report::tests::sweep_and_errors_are_judged`.

### Spec revision 24: 8-CPU/16-GiB benchmark node (user-approved 2026-10-01)

The benchmark node is 8 CPUs / 16 GiB, replacing 4/8, and throughput floors
double: selective 1,200 qps peak, small aggregate 200, medium 40, heavy scan
1,000 MB/s, ingest 200,000 rows/s; the memory ceiling is 15 GiB (f0365f9ea).

### D17 — selective peak below 1,200 qps at 8 CPUs

- **Symptom:** selective peak 1,018 qps; CPU per lookup rose from 6.3 ms
  (4 CPUs) to 8.1 ms (8 CPUs) even at one client.
- **Evidence:** frame-pointer `perf` of selective@1: 13.5% in
  `iceberg::Table::reader_builder` → `std::thread::available_parallelism`
  (cgroup file reads), 9.5% in `ArrowReaderMetadata::try_new` from
  `tenant_proven_reader_metadata`. Both ran once per partition, and the
  session partition count follows CPUs. Phase metrics: `first_row` 2.8 →
  4.1 ms at one client.
- **Cause:** `OracleIcebergScanExec::start_stream` built one Iceberg reader per
  partition; `hot_stream` converted every owned file's footer into Arrow
  reader metadata before range ownership and pruning.
- **Fix site:** `OracleIcebergScanExec` builds one reader per scan in a shared
  `OnceCell` (the `planned` pattern) and partitions clone it;
  `hot_piece_metadata` proves the tenant, prunes on the cached footer, and
  converts only for pieces with surviving row groups (61d1b5cd1). No test,
  timeout, retry, or concurrency was touched. Verified: redux lib 811/811,
  `test:bifrost:integration:redux` 874/874, `test:bifrost:journey:oracle`
  42/42.
- **Result:** selective peak 1,222 qps, one-client 6.8/8.2/8.9 ms.

### D18 — gate: stale snapshot-burst lock and uncovered SQL code

- `capacity::three_concurrent_snapshots_are_not_refused` locked
  `vala.bifrost_tables`, which 3d6b53ed0's uid cache no longer reads per
  query. A fresh diagnostician confirmed the cause; the test now holds the
  uncached `iceberg_catalog.iceberg_tables` pointer read (fb934d6b4).
- `check:error-coverage`: `WYRD_SQL_503_SCHEMA_NOT_READY` lacked a mapping
  test; added to `metadata_codes_and_statuses_match_sql_catalog`.
- `check:workspace-hack` drift regenerated (c58e25b03).

### Final benchmark evidence (8 CPU / 16 GiB, after 61d1b5cd1)

Standard, 10M rows: every row PASS. Write 1,081,154 rows/s; selective target
PASS (one client 6.8/8.2/8.9 ms, peak 1,222.3 qps at 32 clients);
small-aggregate met at 4–32 clients (peak 554.7 qps); 1m-aggregate 231.5 qps
p95 41.8 ms; table-aggregate p95 62.6 ms; reads while writing PASS
(small 226.5 qps p95 20.9 ms; 1m 118.4 qps p95 38.2 ms) with writes at
1.0–1.1M rows/s; full queue PASS; peak 4.96 GiB; shutdown 1.1 s.

Heavy, 100M rows: every row PASS. Write 880,073 rows/s, peak 6.37 GiB, no
OOM; broad-window p99 332.9 ms; full scan p99 1,082.7 ms at 7.97 cores.
Reports: `target/bifrost-query-capacity/{standard,heavy}/`.

The audit publisher's `audit_chain_head` lock warning under load is designed
behavior: `freeze_publication_range` uses `NOWAIT`, skips a busy tenant, and
retries next sweep (`held_chain_head_fails_immediately_and_retries_unchanged`).

### D19 — One-round-trip tenant begin (gate fix and latent pool-poisoning bug)

- **Symptom:** `check:tenant-isolation` failed on `tenant_conn.rs: raw transaction control`. 1a6ec2554 had changed `TenantConn::acquire` to begin with `BEGIN; SELECT set_config(...)`.
- **Cause:** that order had a real bug as well as tripping the check. On a begin failure, SQLx queues `ROLLBACK` only when transaction depth > 0, and depth is still 0 during begin. A failed bind therefore sent the connection back to the pool inside an aborted transaction. Proven: with that order, the next `TenantConn::acquire` on a one-connection pool fails with `25P02 current transaction is aborted`.
- **Fix (owner approved one round trip, a rollback test and a doc update):**
  - `TenantConn` now sends `SELECT set_config('app.current_tenant', '<uuid>', true); BEGIN`. Postgres runs both as one implicit transaction that `BEGIN` makes explicit. If the bind fails, `BEGIN` never runs and Postgres rolls back by itself.
  - A Postgres rejection maps to `TxFailed`; pool and IO failures map to `Connect`.
  - `check_tenant_isolation.py` is unchanged and passes: the new statement does not match its raw transaction-control pattern. An owner exemption added earlier was dead and is deleted.
  - `architecture/v1/00-foundations/sql-foundation.md` documents the statement and why its order matters.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant begin and bind in one round trip | `tenant_conn.rs` `begin_tenant_sql`, `begin_bound` | `tenant_conn::tests::begin_tenant_sql_binds_then_begins` | PASS |
| Failed bind leaves no hanging transaction | statement order | `tenant_conn::pg_tests::failed_bind_rolls_back_and_the_connection_stays_usable` (PG lane only); fails with 25P02 when the order is reversed | PASS |
| Binding is transaction-local | same test: visible inside, empty after commit | same | PASS |
| Gate and docs | unmodified check; sql-foundation.md | `mise run check:tenant-isolation`, `mise run lints`, `mise run py:lints`, `mise run test:sql` (173/6/114/2 passed) | PASS |
import sys
p=sys.argv[1]; s=open(p).read()
reps=[
("  - `check_tenant_isolation.py` names `tenant_conn.rs` as the single owner allowed raw transaction control.\n",
 "  - `check_tenant_isolation.py` is unchanged and passes: the new statement does not match its raw transaction-control pattern. An owner exemption added earlier was dead and is deleted.\n"),
("| `tenant_conn::tests::failed_bind_rolls_back_and_the_connection_stays_usable` (PG); fails with 25P02",
 "| `tenant_conn::pg_tests::failed_bind_rolls_back_and_the_connection_stays_usable` (PG lane only); fails with 25P02"),
("| Gate and docs | check script owner exemption; sql-foundation.md |",
 "| Gate and docs | unmodified check; sql-foundation.md |"),
]
for a,b in reps:
    assert s.count(a)==1,a; s=s.replace(a,b)
s=s.rstrip("\n")+"\n"+open("/dev/stdin").read()
open(p,'w').write(s)

## Replay ledger

Source: TASK-006 commit `0e9c6e98c`. Destination: replay commit `4ae6a1992`
plus the later TASK-004 commits on this branch. Every path in
`git show --stat 4ae6a1992 -- crates sdks scripts mise.toml` falls in exactly
one row. Spec IDs are from `changes/active/verified-change-contract/spec.md`.

Status words: **replayed** (source behavior carried onto current Bifrost seams),
**superseded** (source removed it and current Bifrost has the one owner),
**carried closure** (not Eval or boot itself, but required by a replayed symbol).

| Source behavior | Destination paths | Status | Owning seam / REQ |
|---|---|---|---|
| Eval verification runtime: run owner, Eval engine dispatch, observation enqueue | `wyrd-server/src/verification/{eval,engines,mod,observations,runner}.rs`, `wyrd-server/src/query/{scheduled,service}.rs`, `wyrd-server/src/bifrost/service.rs` | replayed | `VerifierRunner` / `ObservationEnqueue`; REQ-077, REQ-083, REQ-084, REQ-115, REQ-130 |
| Eval scoring, judge, sampling, media and trace tasks | `vala/vala-eval/src/**`, `vala/vala-eval/tests/orchestrator_judge_skald.rs` | replayed | Vala evaluation engine; REQ-130, REQ-131 |
| Eval observation table and post-ACK hook | `vala-bifrost-redux/src/tables/eval/{mod,observations}.rs`, `gate/mod.rs`, `scribe/{ingress,preprocess,shards,mod,execution_lanes}.rs` | replayed onto the current Scribe ACK | Gate `ObservationAck`; REQ-077, REQ-087 |
| Verifier run queue for observations | `wyrd-sql/src/queries/verifier_runs.rs`, `wyrd-sql/migrations/20260601000032_verifier_run_observation_ordinal.sql`, `wyrd-sql/tests/pg_verifier_runs.rs`, `wyrd-server/tests/pg_verification_runtime.rs` | replayed | `VerifierRunQueue`; REQ-079, REQ-083 |
| One boot graph, config, readiness | `wyrd-server/src/{boot/mod,app/server,config,main,lib,state,postgres,test_support}.rs`, `components/health/mod.rs`, `components/{mod,admin/routes,principals/routes}.rs`, `http/{error,router}.rs`, `wyrd-client/tests/startup_image_journey.rs` | replayed; source startup branches superseded | `compose_bifrost` / `ReadinessSnapshot`; REQ-159, REQ-165 |
| DSN-only Postgres startup; embedded Postgres and migrator removed | `wyrd-sql/src/{dsn,error,lib,operator_pool,pool,postgres,schema_check}.rs`, deleted `wyrd-sql/src/postgres_boot.rs` and `postgres_boot/role_bootstrap.rs`, `wyrd-sql/{Cargo.toml,bootstrap/roles.sql,migrations/20260601000000_platform.sql}`, `wyrd-sql/tests/{pg_migration,pg_admin_principals}.rs`, `vala-sql/src/{lib,postgres}.rs`, `vala-sql/migrations/{20260601000000_vala_init,20260619000000_iceberg_catalog,20260910000025_oracle_reader_authority}.sql`, `vala-sql/tests/{pg_migration,pg_oracle_membership}.rs`, `wyrd-dev-fixtures/src/pg.rs`, `scripts/postgres/*`, `scripts/checks/from-pools-allowlist.sh`, `scripts/test-families.sh` | replayed; embedded Postgres superseded | `ResolvedDsns`; REQ-158, REQ-165 |
| Peer plane: mTLS identity and typed, receiver-checked context; ticket keyring removed | `wyrd-server/src/oracle/{peer_authority,peer_service,forwarding,lifecycle_service,lifecycle_transport,tail_discovery,mod}.rs`, deleted `oracle/{peer_credentials,peer_keyring}.rs`, `grpc/{mod,scribe_tail}.rs`, deleted `grpc/peer_auth.rs`, `vala-bifrost-redux/src/oracle/{peer,dispatcher,analytical,analytical_transport,follower,mod,telemetry}.rs`, `scribe/tail_rpc.rs`, `cluster/mod.rs`, `contracts.rs`, `wyrd-tonic/{proto/wyrd.v1.proto,src/private_conversion.rs}`, `wyrd-spec/src/vala/api.rs`, `wyrd-tls/src/lib.rs` | replayed; signed tickets superseded | `OraclePeerGrpc` / `ScribeFragmentExecutor`; REQ-160, REQ-161, REQ-162, REQ-164 |
| One shared durable object store in peer mode | `wyrd-storage/src/{error,factory/mod,handle,service,settings}.rs`, `wyrd-storage/tests/{handle_crud,pg_sweeper}.rs`, `vala-bifrost-redux/src/catalog/iceberg_sql.rs`, `vala-bifrost-redux/src/forge/{mod,planning_scheduler,worker}.rs`, `vala-bifrost-redux/tests/integration/forge/support.rs`, `vala-bifrost-redux/Cargo.toml` | replayed | `wyrd-storage` factory; REQ-163 |
| Authz check route and `PolicyHook` removed | deleted `shared/wyrd-auth-check/**`, `components/authz/{check,mod,routes}.rs`, `components/auth/{policy_hook,audit_writer}.rs`, `components/auth/{mod,routes,state}.rs`, `wyrd-spec/src/error.rs` (`WYRD_AUTHZ_403_POLICY_DENIED`, `…REQUIRES_DELEGATED_TOKEN`), deleted `wyrd-server/tests/{auth_e2e,authz_check_e2e,pg_authz_check_route}.rs`, `wyrd-auth/{Cargo.toml,src/exchange_api_key.rs,src/issuance.rs}`, `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | carried closure | REQ-166 |
| Audit publisher drained by the one boot graph | `wyrd-server/src/audit/publication.rs`, `wyrd-testing/tests/bifrost/server/audit_publication.rs` | carried closure | `AuditPublisher`; boot shutdown order |
| Client transport config without public certificate inputs; SDK URL getters | `wyrd-client/{Cargo.toml,schemas/*,tests/schemas/*}`, `wyrd-client/src/{bifrost/facade,cards/config,cards/hydrate/bundle,client,config,error,transport/config,transport/grpc,transport/http}.rs`, `wyrd-client/tests/{pg_auth_e2e_against_fixture,pg_bifrost_e2e,transport/config_enum,transport/grpc,transport/http}.rs`, `wyrd-cli/src/client.rs`, `wyrd-cli/tests/card_lifecycle.rs`, `sdks/wyrd-sdk-python/{src/client.rs,examples/transport_*.py,tests/examples/test_examples_smoke.py,tests/unit/client/test_client.py}`, `sdks/wyrd-sdk-ts/{native/src/client.rs,wyrd/index.d.cts,wyrd/index.d.ts,wyrd/src/index.ts,wyrd/tests/unit/wyrd-client.test.ts}` | replayed | `wyrd_client` config; REQ-165 |
| Server route tests on the new boot | `wyrd-server/tests/{identity_e2e,pg_card_registration_route,pg_grpc_ingest_smoke,pg_merge_http_protected,pg_router_smoke,storage_e2e}.rs`, `wyrd-server/Cargo.toml` | replayed | boot graph; REQ-159 |
| Test harness and journeys | `wyrd-testing/{Cargo.toml,src/lib.rs,src/principal.rs,src/server.rs,src/verification.rs}`, `wyrd-testing/src/bifrost/{cluster,forge_harness,mod,peer_ca,process_cluster,process_cluster/child}.rs`, deleted `wyrd-testing/src/bifrost/{deployment_contract,peer_keyring}.rs`, `wyrd-testing/tests/bifrost/oracle/{analytical_inactive,distributed}.rs`, `wyrd-testing/tests/bifrost/oracle/peer_network/{join,listener,mod,security,support,transport}.rs`, `wyrd-testing/tests/bifrost/server/{eval_verification,main,query}.rs` | replayed | journey lanes; REQ-101, REQ-161 |
| UI upstream without a public base URL | `wyrd-server/wyrd-ui/src/lib/server/{upstream.ts,upstream.test.ts,routing/tenant.test.ts}`, `wyrd-ui/src/routes/+page.server.ts` | replayed | UI host; REQ-165 |
| Build and lanes | `shared/workspace-hack/Cargo.toml`, `mise.toml` (`test:server:peer`, startup lanes), `scripts/server/{test-startup,test-kind-autoscale}.sh`, `scripts/check_tenant_isolation.py` | replayed; the tenant-isolation exemption added later was deleted (FIND-004-11) | verification lanes |

## Review remediation evidence (task-004-review)

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 operator journey DSN | `platform_admin_e2e.rs::operator_command` swaps both DSNs | both `platform_admin_e2e` tests | PASS |
| 2 Scribe-only WAL fault is unready | `components/health/mod.rs` | `components::health::pg_tests::scribe_only_wal_fault_is_unready_with_verification_composed` | PASS |
| 3 failed replay is role-local | `boot/mod.rs` replay branch; `state.rs` `Scribe::activate` | `owner_inspection::failed_wal_replay_is_role_local` | PASS |
| 4 undeclared bodies charged per frame | `http/middleware/body_limit.rs` | `undeclared_body_is_charged_while_collected_and_released_on_drop` | PASS |
| 5 stopped Eval enqueue logged | `verification/observations.rs::UnfinishedEnqueue` | `verification::observations::tests::dropped_enqueue_records_failure` | PASS |
| 6 peer docs, schema, `MissingPeerIdentity` | design doc, rustdoc, regenerated schema | `mise run codegen:check` | PASS |
| 7 wrong-tenant peer context | `peer_network/security.rs::a_foreign_tenant_context_reads_no_scribe_rows` | `peer_network::security::peer_context_refusals` (correct fragment executes; foreign tenant refused before execution) | PASS |
| 8 cancellation during capacity wait | `oracle/analytical.rs` test; D10 selector note | `oracle::analytical::tests::cancellation_during_peer_capacity_wait_stops_retry` | PASS |
| 9 Forge rewrite pool | `forge/managed/executor.rs` test | `forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel` | PASS |
| 10 replay ledger | this section above | packet review | PASS |
| 11 dead isolation exemption | `scripts/check_tenant_isolation.py`; D19 row | `mise run check:tenant-isolation` | PASS |
| 12 tenant_conn PG test | `tenant_conn.rs` `mod pg_tests` | `tenant_conn::pg_tests::failed_bind_rolls_back_and_the_connection_stays_usable` | PASS |
| 13 memory-wait API deleted | `resources.rs` | `mise run lints` | PASS |
| 14 Python stub properties | `client.pyi` | `mise run py:typecheck` | PASS |
| 15 owning structs | `OracleAuditWriter`, `ScribeWalFaultMonitor` | `mise run lints` | PASS |
| 16 rustdoc | listed items | `mise run lints` | PASS |
| 17 script task reference | `scripts/server/test-kind-autoscale.sh` | `rg TASK- scripts/server` is empty | PASS |

### Diagnosis: intermittent "the Scribe reports no live partition" (peer_context_refusals)

- **Symptom:** `mise run test:server:peer` sometimes failed in `a_foreign_tenant_context_reads_no_scribe_rows` because `list_active_streams` returned no partition right after an acknowledged `ingest_live_rows`.
- **Evidence:** `ShardOwner::run` calls `publish_snapshot` only after `process_group` returns, and `process_group` acknowledges the write in `acknowledge_visible` before it returns. `ScribeShardRuntime::active_seal_keys_for_table` read those published snapshots. The row read path (`ScribeShardRuntime::snapshot`) reads memtables directly.
- **Cause:** partition discovery read a copy that is republished only after the acknowledgement. A listing between the acknowledgement and the republish missed an acknowledged write, so a query issued in that window silently missed acknowledged rows.
- **Fix site:** `active_seal_keys_for_table` now reads each shard memtable through `Memtable::seal_keys_for_table`, the same method the single-memtable branch of `active_partitions_for_table` uses. Its only caller is `active_partitions_for_table`, which serves both `list_active_streams` and the in-process live planner.
- **Verification:** `vala-bifrost-redux` lib `tail_rpc|live_tail|shards::` (54/54, Postgres wrapper); `mise run test:server:peer` (11/11); `mise run fmt`; `mise run lints`.
