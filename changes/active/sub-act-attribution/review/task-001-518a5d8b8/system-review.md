# System resilience review

Result: **FAIL**

## Immutable subject and scope

Reviewed the cumulative change from `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`
through `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`, against approved
`changes/active/sub-act-attribution/spec.md` r4 and TASK-001. HEAD still names
the candidate. This report is an independent system review, not a task-wide
acceptance or standards verdict. No implementation files were changed.

Used CodeGraph before source navigation. Inspected the cumulative changed-path
inventory and runtime diffs, then expanded the queue, acceptance, input-read,
gateway, Scribe outbox, peer, and telemetry paths through surrounding source.
Authorities: AGENTS.md, agent-rules, spec-driven-development, maintainer-style,
testing-workflows, reference router, and applicable runtime/identity/recovery
sections of wyrd-design, bifrost-design and security-posture, plus
analytical-operations-reliability and telemetry-observations references.

## Deployment and changed paths

The serving process owns HTTP/gRPC/MCP authentication and lifecycle. Vala's
Scribe, Oracle and Forge remain capability owners within that process;
Postgres is the durable tenant-scoped queue/catalog and object storage is the
analytical store. A process without Scribe uses the existing authenticated
peer plane. This change does not introduce a new deployed service or recovery
store.

| Changed deployed path | Source and producer-to-consumer evidence | Availability and recovery boundary |
|---|---|---|
| Direct verification → judge → gateway | `components/verification/service.rs` passes the verified Caller into `verification/direct.rs`; judge preparation retains its effective Principal and history and changes execution actor through Caller; `components/gateway/workflow.rs` invokes the existing GatewayInvocation; `invocation.rs` captures pair independently of the permission-bearing Principal; `ledger.rs` charges that Principal. | A permission/model/dependency failure is an operation failure. No new process exit or shared-service shutdown is introduced. Provider cancellation remains the existing gateway cancellation path. |
| Manual deferred verification | `components/verification/service.rs` captures AcceptedAuthority; `queries/verifier_runs.rs::enqueue_manual` writes pair and serialized authority atomically with the run. `CLAIM_RUN_SQL` returns the stored document; `ClaimedRunRow::into_claimed` carries it unchanged. Eval consumes `run.authority`, not an owner/subject re-query. | A worker replacement has no dependence on its predecessor's in-memory Caller. Grant changes affect later acceptance. Existing queue transitions do not update authority or attribution. |
| Scheduled acceptance | `verification/mod.rs` composes BackgroundAuthority into the existing scheduler. `scheduler.rs::schedule_tenant` calls claim_due, authority resolution, schedule_due, then commits one transaction. | SQL failure rolls back the occurrence and cursor; the scheduler logs and continues other tenants, retrying on its poll interval. Cancellation before commit selection rolls back; selected commit is awaited. Missing owner/authority skips one occurrence with AuthorityUnavailable telemetry; it does not crash the scheduler. |
| Observation acceptance | `boot/mod.rs` supplies BackgroundAuthority to ObservationRunSink. Acknowledged Eval rows stage ObservationRecord; the sink locks matching bindings, resolves owner authority, and calls enqueue_observation_runs in the same tenant transaction. | PostgreSQL failure returns to the existing outbox retry, retaining the slice. An unknown commit is safe to repeat because the binding/record identity deduplicates. Missing owner currently has the silent-loss defect below. |
| Queued execution, interruption and recovery | `verification/runner.rs` still uses ClaimLoop and LeaseRenewal. Its engines receive the immutable ClaimedRun; Eval uses Caller::accepted for judge calls; Drift/Eval input reads use the existing SystemReadAuthority, with served subject added only for attribution. | Shutdown releases unfinished work with refunded attempt; lost lease cancels work and prevents settlement. SQL settlement failure backs off while lease holds. Process loss leaves the lease reclaimable. The snapshot travels with that same durable run on every claim. |
| Results/audit/capture/server spans → Scribe | `scribe_outbox.rs` groups by destination, pair, request and schema. `identify` includes both IDs in the content-derived batch identity. `ScribeBatch::into_frame` is shared by local and peer writes. `grpc/capture_peer.rs::batch` transports the same pair over the existing mTLS-only capture endpoint. | Retry submits the identical slice and Scribe batch identity, including after lost ACK. Terminal refusal is logged/counted and consumed; retryable refusal uses the existing outbox. Audit and result staging never waits on persistence. Abrupt process loss or expired shutdown can lose unacknowledged writes, as explicitly approved. |
| Managed ingest and retained recovery | Gate derives the pair from AuthContext on both canonical and native ingress. Scribe ingress transfers that pair into its admitted rows; execution_lanes stamps Arrow rows before durable persistence. Persistence/replay production logic is otherwise unchanged. | Recovery replays the already stamped row bytes; no actor lookup is introduced during WAL replay or publication. Schema changes intentionally require fresh defining schemas, not a rolling old/new schema compatibility protocol (approved unshipped change). |
| Tenant server-span export | `wyrd-telemetry/export.rs::rebind` changes only the emitting operation's live route slot, preserving the existing sink. Gateway judge work binds its own operation route; other request spans retain their route. | Export has no authorization role. ServerSpanSink retains its nonblocking permit/drop behavior, and ended spans remove their live registry entries. No new replay or export durability guarantee is claimed. |
| Operator dispatch and hosted Workflow | Operator runtime changes are projection names only; run-linked leases/retries and permission rechecks remain the existing owner. Workflow host changes supply captured caller pair and Workflow Card to the existing gateway caller. | Operator delivery recovery is not replaced. Workflow remains the expressly approved process-local captured-authority lifecycle; this change adds no durable Workflow or cross-replica recovery machinery. |

## Failure-path assessment

* **Postgres outage during acceptance:** authority lookup and enqueue share the
  accepting transaction. Database errors propagate rather than falling back to
  observation writer, SYSTEM authority, or subject-only grants. Scheduler
  failures are isolated to a tenant pass; observation failures retain their
  outbox slice for retry. Reads cannot silently create an accepted run.
* **Owner becomes inactive or SYSTEM cannot resolve:** BackgroundAuthority
  refuses acceptance and logs. Scheduled refusal is also represented by
  ScheduleSkip::AuthorityUnavailable. No widened run is queued. Observation
  absence of the owner row is a separate unreported refusal (SYS-001).
* **Worker shutdown, lease loss, or restart:** lease tokens, queue due times,
  retry counters and accepted authority are durable Postgres state. The new
  fields do not depend on process-local cache, a bearer refresh, or a newly
  recovered Principal. Claim/renew/settle still belong to the existing queue
  and runner; cancellation cannot rewrite the accepted document.
* **Gateway/provider unavailable or invocation denied:** protected calls
  still use the captured effective Principal. The attribution actor grants
  nothing. Existing bounded attempt/retry behavior remains; refusal cannot
  authorize a provider request using an ambient client.
* **Scribe/peer timeout or lost ACK:** ScribeRefusal maps timeout/cancellation
  and unavailability to retryable refusal. Pair-aware frame grouping and batch
  hashing keep the identity stable; local/peer conversion uses the same frame
  owner. Unrelated requests continue because staging remains nonblocking.
* **Crash after staging result and before/after settlement commit:** the
  existing best-effort result contract remains. Staging is inside the fenced
  settlement path, and a write already staged before an uncertain SQL commit
  can outlive the lease. This is existing behavior and not a newly introduced
  exactly-once guarantee. Approved architecture explicitly permits loss of
  unacknowledged result/audit writes; requiring an additional durable outbox
  would violate scope.

## Material finding

### SYS-001 — Missing observation-binding owner is consumed without an observable failure

Classification: **MISSING**. Violates REQ-008, AC-017 and Scenario 2's
missing-owner refusal behavior.

Changed location: `crates/wyrd/wyrd-server/src/verification/observations.rs:117–118`.
Producer: `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:132–144`,
LOCK_OBSERVATION_SUBJECTS_SQL; projection in lock_observation_bindings.

The binding query intentionally returns an optional owner principal from the
tenant's machine-principal relation. An otherwise acceptable binding whose
owner has no corresponding principal still enters the returned bindings.
accepts_records checks Verifier/subject/implementation readiness, not owner
principal existence. ObservationRunSink then hits `let Some(owner) ... else
{ continue; }`. Unlike failures in BackgroundAuthority, this path emits no
diagnostic or refusal outcome. enqueue_observation_runs receives no entry for
that binding, and the sink commits and returns success. The generic outbox
therefore consumes the request without reporting loss or a binding failure.

Observable consequence: an acknowledged observation can produce no required
binding run, and the operator has no missing-owner acceptance failure to
distinguish this from an observation with no matching binding. Restoring the
owner later does not re-enqueue the already consumed request. The operation
correctly refuses unsafe execution, but fails the explicitly required
observability at that refusal boundary.

Sibling paths inspected: the scheduler passes absent authority into
schedule_due and emits `authority_unavailable`; BackgroundAuthority logs
missing SYSTEM, missing configured issuer and rejected owner recovery. The
only production caller of lock_observation_bindings is ObservationRunSink;
the other caller is the existing SQL integration helper. No separate runtime
failure owner handles the `None` branch.

Smallest correction: keep the refusal local to the binding acceptance in the
existing ObservationRunSink and make the missing-owner refusal observable
using existing structured tracing, naming tenant, binding and owner Card.
Preserve successful sibling binding inserts and consume this nonretryable
identity refusal; do not turn it into a retry of the entire observation batch,
a shared-process failure, a new audit decision, or a new failure store. No new
API, identity fallback or outbox is required.

Focused closure proof: extend the existing background-owner journey or
ObservationRunSink integration coverage with an otherwise valid binding whose
owner principal is absent. Verify that its run is not created, that the
existing diagnostics capture the binding-specific missing-owner refusal,
and that a healthy sibling binding still enqueues. Existing inactive-owner
coverage alone cannot enter this branch because its principal ID is present.

## Verification evidence and limits

The task records passing V1–V11 and owner/static lanes. Source inspection
supports those claims' relevance, not independent execution. In particular:

* `queued_subject_authority_survives_restart` creates fresh runtime owners
  across a retry and verifies the database document stays unchanged after
  grant edits. It uses scripted Drift outcomes; it does not kill a process,
  force an expired lease, expire an initiating token, or invoke a real judge
  after that reload. It is useful durable snapshot proof with those limits.
* `background_owner_subject_system_actor` drives a real observation/Eval/judge
  path, checks owner/SYSTEM, denies the underprivileged owner, and checks that
  a suspended owner produces no run. It does not cover a missing owner row or
  assert an observable failure for that case.
* Existing outbox tests such as a_retry_resubmits_identical_frames and
  a_terminal_rejection_never_blocks_later_writes exercise the unchanged
  reliability owner with the new pair-aware representation. The peer adapter
  retains the shared Scribe status mapping and pair-preservation checks.

No runtime Cargo, crash, live-cloud, or deployment tests were rerun for this
review. The material remaining risk is SYS-001's silent observation-run
acceptance failure; no additional durability mechanism is recommended.
