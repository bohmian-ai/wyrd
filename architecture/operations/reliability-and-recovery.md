# Reliability and Recovery

This document defines how Wyrd sets service objectives, manages capacity,
recovers durable state, and handles production incidents.

## Objective ownership

Every deployment defines an approved objective record before readiness. It
contains:

| Field | Required decision |
|---|---|
| Service owner | Team accountable for the user-visible objective and error budget |
| Scope | Deployment, tenant class, region, surface, and workload class |
| SLI | Exact event population, success condition, exclusions, measurement point, and aggregation |
| SLO | Target and rolling window |
| Error-budget policy | Alert thresholds, release restrictions, and escalation owner |
| RPO | Maximum acceptable loss for each durable data class |
| RTO | Maximum restoration time for each supported failure class |
| Evidence | Dashboard, alert, recovery test, and review cadence |

Wyrd does not supply a universal numeric SLO, RPO, or RTO. A deployment that
has not made and qualified these decisions is not production-ready. Local
defaults may aid development but never become an implicit production promise.

## Required service indicators

At minimum, deployments measure:

- HTTP/gRPC/MCP availability and latency by bounded route family and outcome;
- authentication, permission, policy, and audit availability;
- Scribe admission latency, queue age, fairness, WAL fsync latency, staged-run
  age, persistence lag, object publication, replay, and rejection;
- Oracle interactive and analytical queue age, execution latency, result
  outcome, cancellation, terminal peer failure, memory, exchange, spill,
  audit commit failures, and partial-result prevention;
- Forge demand age, claim age, lease expiry, plan-estimate accuracy, local FIFO
  age, running estimated memory and parallelism, worker loss, attempt outcome,
  rewrite debt, commit conflict, uncertain publication, reconciliation,
  snapshot expiry, and orphan cleanup;
- Postgres saturation, replication health, migration state, transaction
  failures, and RLS/role verification;
- object-store latency, throttling, integrity failure, and capacity;
- audit-staging publication lag and failure outcome; and
- backup age, backup verification, restore duration, and recovery-point age.

Tenant, table, query, object path, task, attempt, and principal identities do
not become metric labels. They belong in redacted structured logs, traces, or
audited diagnostic records.

## Capacity and overload

Capacity planning accounts for steady traffic, rollout surge, dependency
failover, replay, compaction debt, and one declared fault domain unavailable.
The operator records the assumptions and validates them with representative
load evidence.

Overload is bounded and explicit:

- Scribe applies global, tenant, table, shard, memory, disk, backlog, staging,
  merge, and upload admission before accepting work. It seals and drains before
  returning a retryable busy response where the architecture permits.
- Oracle preserves a non-borrowable interactive floor, separates interactive
  and analytical queues, and admits every query against query-owned memory,
  exchange, scratch, deadline, and cancellation resources.
- Forge uses durable demand, per-tenant fairness, fenced leases, and one
  strict FIFO per worker bounded by pending/running parallelism and running
  estimated memory. Compaction debt cannot consume Scribe or Oracle's
  protected resource floor.
- Postgres and external dependencies use bounded pools, timeouts, and
  backpressure. An exhausted dependency does not trigger unbounded retries or
  queue growth.

When a protected floor, durable volume, audit path, or safety invariant is
exhausted, the responsible surface rejects new work. It does not borrow across
tenant or role boundaries, disable an audit, drop accepted data, or return a
partial success.

## Backup contract

Backups cover all authoritative and recovery-critical state:

- Postgres base backup plus continuous WAL sufficient for the deployment RPO;
- object-store versioned data, Iceberg metadata, manifests, data files, and
  configured retention protection;
- encrypted signing, peer, Source, and storage key backups under separate
  access control;
- deployment and redacted configuration fingerprints; and
- node-to-volume identity needed to recover Scribe WAL and staged runs.

Oracle spill is not a backup source. Forge has no local scratch state. Scribe
staged runs and WAL remain required until their authoritative replacement and
retirement fence are proven.

Backups are encrypted, immutable for their retention interval, integrity-
checked, access-audited, and stored outside the primary failure domain. A
successful backup job is not evidence of recoverability; scheduled restore
qualification must recreate an isolated deployment and validate protocol,
tenant, audit, catalog, and query invariants.

## Restore ordering

Restore proceeds under write isolation:

1. Select one mutually consistent recovery point and preserve incident
   evidence before mutation.
2. Restore keys and trust configuration required to verify, but not yet serve,
   the recovered state.
3. Restore Postgres and verify migrations, roles, RLS, sentinels, audit chains,
   task/lease state, and catalog pointers.
4. Restore or select the object-store version set and verify every referenced
   Iceberg metadata, manifest, and data object.
5. Reconcile `vala.audit_staging` with retained `vala.system.audit_log` history.
6. Attach each Scribe volume to its stable node identity; replay WAL and staged
   runs without accepting traffic.
7. Reconcile Scribe publications, Forge attempts and uncertain commits,
   snapshot cleanup cursors, and orphan protection sets.
8. Rebuild disposable indexes and caches from authoritative state.
9. Run tenant-isolation, append/replay, pinned-query, audit, and SDK journeys.
10. Admit traffic by role only after the restored state satisfies the approved
    recovery point and time objectives.

If Postgres and object storage cannot be reconciled to one safe cut, recovery
fails closed. Operators do not advance catalog pointers, delete objects, or
fabricate task completion to make health checks pass.

## Scribe failure boundaries

- Acknowledgement requires WAL append/fsync and the durable batch fence.
- A frozen cohort's WAL retires only after every member is a final-fsynced,
  validated, query-registered staged run.
- Staged runs are authoritative live-tail inputs until publication and pinned-
  reader release make deletion safe.
- Restart dispatches WAL by recorded shard identity and reconstructs dedup,
  cohort, stage, claim, and publication state before admission.
- Object PUT followed by uncertain SQL settlement is reconciled from
  deterministic claim identity and object evidence. It is not blindly retried
  under a new identity.
- A corrupt WAL frame, missing staged member, incompatible schema/layout,
  tenant mismatch, exhausted durable capacity, or unreconciled claim prevents
  the affected role from becoming ready.
- Shutdown stops admission, drains accepted batches through a safe durable
  boundary, settles or records every claim, and releases the dedicated runtime
  only after supervised work has joined.

## Oracle failure boundaries

- Query admission fsyncs the local audit acceptance WAL before any result row
  can be returned. Relay lag blocks readiness or admission according to the
  deployment's bounded audit backlog policy.
- Interactive and analytical execution share one immutable deadline and a
  query-owned cancellation tree. Cancellation joins every descendant and
  releases memory, exchange, spill, peer, and admission resources.
- Every distributed stage is bound to tenant, snapshot digest, fragment digest,
  attempt identity, deadline, destination, and fence before plan decode or IO.
- Analytical selection binds one logical query to one execution attempt. Peer
  or transport loss after selection fails the logical query terminally; the
  server constructs no successor attempt and performs no interactive rerun. A
  caller may submit a new logical query.
- A failed stage, tenant tripwire, exhausted unspillable memory, spill
  exhaustion, protocol mismatch, deadline, or cancellation terminates the
  complete result. Terminal framing prevents a partial stream from being
  interpreted as success.
- Physical planning failure is terminal; Oracle does not build or run a second
  interactive plan. Planning and path selection cannot bypass admission,
  audit, tenant binding, deadline, or payload policy. After Analytical
  selection there is no interactive rerun.
- Cleanup timeout or failure is never reported as a successful release. The
  remaining graph stays observable to the owning supervisor, the node does not
  claim a clean terminal state, and readiness or shutdown evidence surfaces the
  failure.

## Audit history projection and retirement

`vala.audit_staging` is transient write-ahead state. Retained audit history is
the tenant-qualified Bifrost `vala.system.audit_log` table. Projection reads
immutable contiguous tenant ranges by a per-tenant watermark rather than
claiming them, and garbage-collects staged rows once the watermark has advanced
past them; a replayed range is absorbed by Scribe's durable batch-id fence.
The current Scribe and Forge publication path publishes those ranges
idempotently; a legacy direct-Iceberg relay is not a recovery mechanism.

Staging rows retire only after their corresponding events are durably published
to `vala.system.audit_log`. Recovery reads the per-tenant watermark together with
the persisted frozen upper bound: a bound that survived a crash names the exact
range whose publication is uncertain, so every competing or restarted publisher
replays that identical range and derives the identical batch identity. An
ambiguous boundary preserves the staging row and retries the idempotent
publication. Publication is itself an engine transition and appends no audit.

## Forge failure boundaries

- Durable demand, task, lease generation, plan identity, attempt identity,
  output identity, and fence identify every maintenance transition.
- Only the live lease and fence may create effects or acknowledge demand.
  Stale workers cannot publish, settle, or delete.
- The managed compaction core produces bounded outputs and never commits the
  Iceberg catalog. Forge validates each exact handoff; every admitted plan owns
  one fenced `commit_once` operation, and sibling plans may publish
  concurrently under the task attempt's lease and fence.
- A definite non-commit leaves that plan's work as replannable debt. The task
  returns to durable failure scheduling only when no sibling published; any
  known committed or recovered sibling settles the task successfully. An
  ambiguous operation with no known success retains the Running attempt for
  exact reconciliation. When a sibling already succeeded, the task settles and
  the still-Prepared operation remains for table-wide reconciliation.
- Promotion, rewrite, snapshot expiration, expired-object deletion, and
  never-published orphan deletion are separate durable operations with
  independent protection sets and cursors.
- Cancellation after output creation preserves enough durable identity to
  reconcile or safely collect the outputs. Normal completion or cancellation
  releases the plan's local estimated-memory and parallelism reservation.
- Process loss drops the local queue and running reservations. Lease expiry
  returns unfinished durable work to scheduling, fences stale completion,
  resumes cursors, and preserves one authoritative terminal outcome per
  attempt.

## Dependency and regional failure

- Postgres unavailability stops mutations, policy decisions that require
  durable state, Forge transitions, and any audit path that cannot satisfy its
  defined durability boundary. Requests fail with stable retry semantics.
- Object-store unavailability stops publication and queries requiring missing
  objects. Scribe retains accepted data within governed local durability;
  resource exhaustion then stops admission.
- Secret, KMS, JWKS, or policy-provider uncertainty denies the affected
  operation. Cached trust never exceeds the bounds in the security posture.
- Clock health is required for token, lease, deadline, snapshot, and retention
  semantics. Excess skew removes readiness.
- A deployment spanning failure domains declares quorum, routing, data
  placement, failover, and split-brain fencing explicitly. DNS or load-balancer
  failover alone is not a consistency design.

## Incident response

Every deployment assigns an incident commander, operations owner, security
owner, communications owner, and evidence custodian. The response sequence is:

1. Detect and classify the affected tenants, surfaces, data classes, and
   durable transitions.
2. Contain without destroying evidence: remove readiness, stop targeted
   admission, fence workers, revoke credentials, or isolate egress as needed.
3. Preserve logs, traces, audit chains, database WAL, object versions, Scribe
   volumes, task/lease state, and deployment
   fingerprints under controlled access.
4. Determine the last verified safe cut and whether confidentiality,
   integrity, availability, or tenant isolation was affected.
5. Recover through the owning state machine or restore procedure. Do not patch
   durable state outside its authoritative owner.
6. Verify tenant isolation, audit continuity, credential state, replay safety,
   catalog/object consistency, and user journeys before restoring traffic.
7. Record the timeline, decisions, evidence, affected objectives, notification
   obligations, root cause, and corrective controls.

Security incidents additionally rotate affected signing, peer, Source, and
storage credentials; revoke affected credentials and suspend affected
principals so no new token issues; and verify that no retired material remains
accepted once issued tenant tokens reach their five-minute expiry.

The executable provider-neutral procedures are in
[`runbooks.md`](runbooks.md). A deployment maps each procedure's named evidence
and mutation capability to its infrastructure without changing the stop/go
criteria.

## Recovery qualification

The production qualification suite exercises, with real dependencies:

- Postgres point-in-time restore and migration compatibility;
- object-store version recovery and missing/corrupt-object detection;
- audit-staging and retained audit-log reconciliation;
- Scribe crash at each durability boundary, replay, duplicate suppression,
  staged-run recovery, and uncertain publication;
- Oracle leader and peer loss, cancellation, timeout, spill exhaustion,
  terminal post-selection failure with joined cleanup, and terminal-safe
  streaming;
- Forge lease loss, stale completion, commit conflict, ambiguous publication,
  cleanup cursor takeover, and orphan protection;
- credential/JWKS/peer-key rotation and emergency revocation;
- policy and dependency outage fail-closed behavior; and
- Rust, Python, TypeScript, HTTP, gRPC, and MCP journeys for enabled public
  surfaces.

Qualification fails when assertions are skipped, an expected failure is
converted to success, a gate is weakened, or evidence cannot be tied to the
tested artifact and configuration.
