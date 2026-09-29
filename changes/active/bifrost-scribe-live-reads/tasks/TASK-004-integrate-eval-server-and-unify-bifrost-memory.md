---
id: TASK-004
kind: remediation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 12
requirements: [REQ-003, REQ-005, REQ-008, REQ-010, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-003, AC-004, AC-008, AC-009, AC-010, AC-012]
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
TASK-003 code on the merged PR is the starting state; TASK-004 owns the
remaining integrated benchmark and gate evidence, so a separate TASK-003
PASS verdict is not a prerequisite.

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

## Owners, Scope, Consumers, and Prohibited Changes

- This task's implementation base is `origin/main` at
  a56ab7569aa702dddec0b36d097d38abe8918e8c, the PR #94 merge whose
  second parent is vcc/task-005 commit
  5ae8125156add44e549ac090a82a4fcb20e9cce7. The dedicated
  `vcc/task-004` worktree contains Bifrost spec revision 12 and this task,
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

| Holder | Charge and return rule |
| --- | --- |
| Scribe | Existing ingress, memtable, and staged-memory leases charge their actual held bytes through this root; release only when their owning data/work drops. Remove floor and elastic partitions. |
| Oracle | Reuse `OracleMemoryRoot`'s DataFusion `MemoryPool` view. `try_grow` checks the query ceiling then charges the shared governor and pool atomically; on refusal roll back completed charges. `shrink` returns the same consumer's bytes. `grow` remains DataFusion's infallible path: record its overshoot as headroom and release it on shrink/drop, without a watchdog. Query-owned runtime and child graph retain the pool view until they drain; only then return slot and memory. |
| Forge | Feed the managed rewrite context a DataFusion `MemoryPool` backed by the **same** governor, using its existing `with_memory_pool` and `with_spill_lease` builder inputs. Let that dependency create the normal DataFusion disk manager. Use actual reservation growth/shrink and the same accounted-headroom rule for infallible `grow`, not the estimator or an independent finite cap. Keep existing parallelism. A failed attempt drops/join its context before retry; its durable task remains unsettled and publishes no partial snapshot. No extra Forge heap estimate is charged. |
| Transport | Keep the current per-message byte ceiling and the existing body-admission lifetime. Validate the declared/decoded message against that ceiling before retaining it; charge the actual encoded bytes held, not a maximum-message precharge, to the shared root. HTTP and gRPC body owners return that charge when the body is consumed or dropped, including cancellation and decode error. Delete transport's independent aggregate counter and unmanaged-reserve coupling. |

DataFusion spill bytes remain disk use managed by its normal disk manager,
not memory charged to this ledger. A genuine shared-governor invariant poison
still stops the process through its existing health watcher; an ordinary role
capacity refusal does not. This is one owner with existing role-local handles,
not a second accounting service or a new public resource interface.

### Scribe failure boundary

TASK-003 already decided this boundary. WAL integrity/ambiguous mutation
marks **Scribe** unready immediately. Its owner stops admitting writes before
ACK, withdraws Scribe readiness/heartbeat and cluster role fence, and
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
   seams; run focused and real-server regressions before resource changes.
3. Resolve the default 1-GiB server minimum and one shared Bifrost cap from
   the detected process limit; remove idle role memory partitions.
4. Give Forge bounded DataFusion execution and normal spill; retain its
   parallelism and durable retry. Preserve Oracle's queue, per-query ceiling,
   spill, and child-owned cleanup.
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
where the source journeys do not cover current Bifrost. Confirm the unported
boot, routing, or source seam fails. The source peer lane is replayed with
its `mise.toml` entry; the exact integrated selectors are below.

**GREEN.** Adapt TASK-006 startup, public routing, readiness, peer mTLS,
and typed contexts to current Bifrost services. Run server, peer, Oracle,
and Scribe journeys.

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
separate bounds. Forge uses a bounded DataFusion pool and spill; an exhausted
attempt publishes nothing partial and follows durable retry. An estimate
does not reserve future heap use in advance.

**RED.** Add focused concurrent-charge/release cases and a Forge
failure/retry integration case. Show that Forge's current estimated budget
and unbounded pool do not prove the shared cap. Use the named resource and
Forge selectors below.

**GREEN.** Route governed allocations through the shared Bifrost authority.
Use the compaction dependency's existing bounded-pool and spill capability.
Charge transport body ownership to Bifrost rather than server headroom.
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
   and cross-pod journeys satisfy both specs. A Scribe WAL failure is role-local;
   shared-governor poison remains process-terminal.
2. Default 8-GiB detection yields 1-GiB server headroom and a 7-GiB
   governed Bifrost cap. Overrides and invalid boot are proven. Server
   work has no application cap. Idle roles reserve nothing; concurrent
   fallible governed charges respect one total.
3. Forge failure cannot partially publish. Query memory exhaustion fails
   only its requesting query with a distinct public reason before and after
   stream opening, without retry metadata. Infallible and untracked
   allocations are documented honestly, not claimed OOM-proof.
4. Every approved standard and heavy workload has valid measured evidence
   meeting Bifrost REQ-008/AC-010. Invalid or wrong-result rows cannot pass.
5. Focused checks, generated contracts, and the broad gate pass after
   benchmark acceptance. PR #94's CI override is not completion evidence.

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
Forge context in `forge/managed/executor.rs`; do not create a second root.

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
| New `server::owner_inspection::scribe_wal_fault_is_role_local` journey | Combined target with Eval worker: force an ambiguous WAL mutation through the existing Scribe test-fault seam after a durable write; new write gets no ACK, Scribe is unready and withdrawn, Oracle published read and Eval worker remain healthy; restart recovers WAL before Scribe ready. A separate shared-governor poison still shuts down the process. | 1 |
| New `resources::tests::shared_cap_defaults_overrides_and_concurrent_charges` unit | Inject 8-GiB observation, exercise default and operator limits, idle roles, simultaneous Scribe/Oracle/Forge/transport charges, failed charge, release, and re-admission; assert 1-GiB headroom, 7-GiB cap, no partition or precharge, and aggregate invariant. | 2 |
| New `config::tests::server_memory_minimum_rejects_impossible_plan` unit | Parse env/config combinations including impossible minimum/cap and removed unmanaged setting; assert one resolved boot plan or boot error, no alias. | 2 |
| New `forge::managed::executor::tests::rewrite_pool_charges_root_and_releases_on_cancel` unit plus `forge::live_rewrite::failed_memory_attempt_retries_without_partial_publication` journey | Hold Oracle/Scribe charges; run Forge bounded DataFusion growth and spill, force exhaustion/cancel, prove root returns bytes only after work ends; real Forge task retries and publishes one complete snapshot, never a partial one. | 2, 3 |
| New `oracle::capacity::memory_failure_is_query_local_and_typed` journey | Two running queries plus one queued: force one admitted query's fallible DataFusion allocation failure after rows; sibling finishes, failed stream has one typed Failed terminal, queued query runs only after children and memory release; server stays healthy. | 2, 3 |
| New `grpc::query::tests::resource_failure_has_no_retry_hint` unit and `bifrost::query::tests::resource_terminal_rejects_partial_rows` unit | Map pre-stream 503 and post-stream Failed terminal; assert stable code, no `retry-after-ms` for resource failure, no partial client success, and queue-full remains 429/retryable. | 3 |

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
- A benchmark miss is a failed outcome to diagnose, not permission to lower
  the target, remove the workload, or substitute the gate.

## Authority Links

- Approved Bifrost spec revision 12: ../spec.md
- Approved verification contract revision 44 at pinned vcc/task-006 commit
  `0e9c6e98c74361b1f2d18dd857a2aff7690cfdb0` (the local
  [verified-change-contract spec](../../verified-change-contract/spec.md)
  is still revision 38 until replay).
- Continuous Eval source task:
  ../../verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md
- Repository rules: ../../../../AGENTS.md
- Bifrost architecture: ../../../../architecture/bifrost-design.md
- Wyrd protocol: ../../../../architecture/wyrd-design.md
