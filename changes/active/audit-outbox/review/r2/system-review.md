# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Original tasks: `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Remediation authority: `review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Prior validated findings: `review/r1/findings-validation.md` and
  `review/r1/verdict.md`

The repository has no `.codegraph/` index. `HEAD` matched the candidate before
this report was written. The complete base-to-candidate diff, the current
runtime owners and callers, the revision-2 task evidence, and the applicable
architecture and operations documents were inspected. No production or test
source was changed by this review.

## Deployed path and affected capabilities

`compose_bifrost` constructs one process audit outbox with
`AuditSink::outbox`, and `AppState` shares that handle with Gate, Oracle, peer
security, `wyrd-auth`, gateway, verification, Card, principal, admin, and
platform request paths (`crates/wyrd/wyrd-server/src/boot/mod.rs:1020-1026`;
`crates/wyrd/wyrd-server/src/state.rs:2209-2214`). Each request stages without
waiting. The generic writer groups by tenant, permits one in-flight write per
tenant, bounds cross-tenant writes by four connections for audit, and returns a
failed batch to the front of that tenant's queue after 50 ms-to-5 s
exponential backoff (`crates/shared/wyrd-runtime/src/outbox.rs:230-374`;
`crates/vala/vala-sql/src/audit_outbox.rs:27-35,83-108`). A failing tenant
therefore does not hold a connection during backoff or prevent healthy tenants
from using the remaining writer slots.

`AuditSink::write` opens a tenant-bound transaction, calls the sole production
`append_audit_events`, and commits. Each staged decision receives a UUID when
it enters the outbox. The append locks the tenant chain head, looks for those
UUIDs in the surviving `vala.audit_staging` rows, chains only UUIDs it does not
find, inserts them, and advances the head in the same transaction
(`crates/vala/vala-sql/src/audit_outbox.rs:37-57,88-108`;
`crates/vala/vala-sql/src/queries/audit_staging.rs:79-209`). This preserves a
gap-free committed chain across replicas and correctly absorbs an immediate
retry while the original staging row still exists.

In Scribe-capable replicas, `AuditPublisher` runs concurrently with the outbox.
Every five seconds it freezes or reuses a tenant range, durably appends that
range through Scribe, then advances the watermark and deletes every staging row
through it (`crates/wyrd/wyrd-server/src/app/server.rs:596-609`;
`crates/wyrd/wyrd-server/src/audit/publication.rs:37-54,259-285`;
`crates/vala/vala-sql/src/queries/audit_staging.rs:350-520`). Another publisher
or a restarted publisher reuses the frozen bound, so publisher-side uncertain
replay remains idempotent. The affected user-facing capabilities are every
audited authorization surface named above and the retained audit history those
surfaces produce. The generic Eval sink is not present in this candidate, so
its later use of the generic machinery is not runtime scope here.

## Failure and recovery assessment

| Scenario | Deployed behavior and recovery | Assessment |
|---|---|---|
| Definite Postgres acquire, append, or commit failure | The batch returns to the front of its tenant queue, later same-tenant events remain behind it, failures are counted per attempt, and retries hold no connection during backoff. Healthy tenants remain dispatchable. | Meets REQ-003, REQ-008, AC-008, and the ordering portion of INV-002. Unit and Postgres-backed recovery tests cover definite rollback/failure. |
| Commit outcome unknown | The original transaction may be committed even though `commit()` returns an error, so the writer waits and retries the same event IDs. If the original staging rows still exist, the retry skips them. | Unsafe when publication retires those rows before retry; see `SYS-R2-001`. |
| Publisher or Scribe outage/cancellation | The frozen upper bound and staging rows survive. A competing or restarted publisher reuses the same range and Scribe batch identity; settlement happens only after durable append. | Recovery is sound for publisher-side ambiguity and publication concurrency. Existing journey coverage exercises competing publishers, stale settlement, and replay. |
| Process crash or pod kill | Process-local queued decisions can be lost. Committed staging, publication watermark, and a frozen publication bound survive in Postgres and are recoverable by another replica or restart. | This is the accepted REQ-003a loss boundary. It does not authorize the live-process duplicate in `SYS-R2-001`. |
| Rolling replacement | The retiring process stops transport work and its local publisher, then drains the audit outbox to the shared deadline. Other replicas may continue publishing the same tenant during that drain. | Ordinary queued work drains or is counted lost at the deadline. A publisher on another replica can still trigger `SYS-R2-001`. |
| Graceful shutdown and cancellation | Intake closes after accepted request work drains. The outbox continues retrying until the absolute process deadline; at expiry it cancels in-flight writes, counts the pending remainder lost, and returns. | Meets REQ-007/REQ-003a and AC-007 for the tested healthy-recovery and persistent-failure cases. The single-process publisher has already stopped, but a peer publisher remains concurrent. |
| Prolonged database outage | Requests continue and the unbounded process queue grows; four is only the connection-concurrency bound. Recovery drains per tenant with backoff. | The availability and unbounded-memory tradeoff is explicit in revision 2, so it is not a finding. Operators must use pending/failure metrics and avoid terminating a pending replica during the outage. |
| Writer task panic | A panicked sink task is logged, counted lost, and removed from `pending`; the process stays up. | No production panic path was found in `AuditSink::write`. The generic test suite does not inject a sink panic, so this remains a proof limit rather than a reachable task finding. |

## Requirement and proof assessment

| Obligation | System evidence | Result |
|---|---|---|
| REQ-003 / REQ-003a | Front-of-queue retry, bounded cross-tenant writes, no count cap, shutdown-deadline loss accounting; request paths stage synchronously without awaiting the sink. | PASS for definite dependency failure and accepted shutdown/crash loss; exact-once recovery is qualified by `SYS-R2-001`. |
| REQ-007 / AC-007 | `Outbox::shutdown` closes intake, retries until its deadline, then abandons and counts the pending remainder. Unit tests prove recovery before deadline; `pg_audit_outbox::shutdown_reports_events_unwritten_at_the_deadline_as_lost` proves persistent failure. | PASS. |
| REQ-008 / AC-008 | One SQL-free generic owner supplies queueing, pending metrics, per-tenant ordering, concurrency, retry, idle signalling, and shutdown. Focused tests cover retry ordering, tenant independence, a 50,000-item queue, healthy recovery during shutdown, deadline loss, and pending returning to zero. | PASS. |
| REQ-009 / AC-009 | UUID uniqueness and lookup exist only in transient `vala.audit_staging`; the focused test repeats the batch before any publisher retires it. | **FAIL — `SYS-R2-001`.** |
| INV-002 | Every committed append still serializes on the chain head and allocates a gap-free sequence. | PASS narrowly for sequence/hash-chain continuity. `SYS-R2-001` duplicates a logical decision at a later valid sequence rather than creating a chain gap. |
| AC-002 | Named surfaces have failure/recovery coverage and requests remain successful. The Gate/run/Oracle journey proves eventual commit after a definitely rejected staging transaction. | FAIL as complete exactly-once recovery evidence because no surface or integration test exercises commit ambiguity concurrent with retirement; see `SYS-R2-001`. |
| AC-005 | The task packet explicitly records that `mise run bench:capacity` was not run, and this candidate still has no task by that name or preserved passing two-replica scale-out artifact. | **FAIL — `SYS-R2-002`.** |

## Material findings

### SYS-R2-001 — Retirement deletes the only dedup identity before an ambiguous commit retry

- **Classification:** INCORRECT.
- **Violated obligation:** REQ-009 and AC-009 require a write retried after an
  unknown commit outcome to produce no duplicate staged rows; AC-002 requires
  recovery to commit the event exactly once. The operations contract also says
  recovery commits queued decisions exactly once and a retry never stages a
  decision twice.
- **Exact location:** dedup lookup in
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-130`; retry scheduling
  in `crates/shared/wyrd-runtime/src/outbox.rs:342-373`; retirement in
  `crates/vala/vala-sql/src/queries/audit_staging.rs:489-520`. The insufficient
  proof is
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215`.
- **Reachable failure path:** (1) Postgres commits an `AuditSink::write`
  transaction, but the connection fails before the client observes the commit;
  (2) the outbox receives `Err`, retains the original event IDs, and waits for
  backoff; (3) the local publisher or another replica freezes, publishes, and
  settles the committed range, deleting the original rows; (4) the retry's
  `already_staged` query finds no event IDs, allocates new sequence numbers from
  the advanced chain head, and inserts the same authorization decisions again;
  (5) a later publisher range retains those duplicate decisions under a new
  batch identity. The five-second publisher cadence overlaps the retry window,
  especially after repeated failures reach the five-second maximum backoff,
  and another replica is not stopped by this process's shutdown sequence.
- **Observable system consequence:** retained audit history can contain the
  same authorization decision twice with two valid, consecutive chain entries.
  The hash chain remains gap-free, so chain validation does not expose the
  semantic duplication. Audit consumers, investigations, and policy evidence
  can therefore count or attribute one decision twice.
- **Why current proof is insufficient:**
  `rewriting_a_committed_batch_stages_each_event_once` performs the second
  write while the first rows remain in `vala.audit_staging`; it neither runs
  the publisher nor deletes through the watermark. Definite trigger failures
  used by the surface journeys roll back and never exercise an unknown commit
  outcome.
- **Required testable correction:** make an audit event's dedup identity remain
  authoritative across staging publication and garbage collection, without
  adding a second audit writer or weakening publisher settlement, chain
  ordering, tenant isolation, or non-blocking request behavior. The correction
  belongs at the canonical append/retirement persistence boundary, not as a
  guard on individual request surfaces. Add a Postgres-backed recovery test
  that commits a known event-ID batch, simulates the sink observing an unknown
  outcome, durably publishes and retires that original range, retries the same
  batch with a later event queued behind it, and proves one retained decision
  per event ID, no restaged duplicate, a gap-free chain, and eventual pending
  zero. If preserving the identity requires a new durable table/contract or a
  change to the mandated same-transaction retirement model, the orchestrator
  must route that persistent-data decision through specification/architecture
  approval rather than choose it inside remediation.

### SYS-R2-002 — The required capacity and two-replica scale-out proof remains absent

- **Classification:** MISSING.
- **Violated obligation:** AC-005 requires the integrated canonical capacity
  run to meet every judged saturation SLO and pass the two-replica scale-out
  step.
- **Exact location:** `changes/active/audit-outbox/spec.md:197-200` and the
  explicit omission recorded in
  `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md:265`.
  `mise.toml:511` defines `bench:verification:capacity`, not the required
  integrated `bench:capacity` lane.
- **Evidence:** the remediation records correctness, SQL, server, Bifrost,
  TypeScript, codegen, docs, format, lint, and boundary results, but expressly
  defers FIND-AUDIT-OUTBOX-5. No passing canonical artifact was found in the
  candidate. The earlier recorded two-replica capacity result is the regression
  that motivated this change, not closure evidence.
- **Observable system consequence:** the change can be functionally correct in
  focused tests while still failing its primary deployed objective: restoring
  sustained throughput and two-replica scale-out under audit contention.
- **Required testable correction:** from the integration state that owns the
  canonical `bench:capacity` task, run the exact judged one-replica and
  two-replica capacity sequence against this cumulative candidate and preserve
  a green artifact for every saturation and scale-out verdict. A smoke run,
  predecessor task, or audit-only microbenchmark is not equivalent proof.

## Verification limits

- This review did not rerun the expensive Postgres, server-journey, aggregate,
  or capacity lanes. It inspected their recorded results and the exact source
  paths those results claim to cover.
- The supplied green SQL test proves idempotency only while the original event
  ID remains in staging. There is no fault hook that makes `commit()` return an
  unknown outcome after server-side commit, so current tests cannot prove the
  cross-component recovery path.
- The generic outbox unit sink proves retry mechanics but cannot prove the
  audit sink's durable idempotency boundary or its interaction with a separate
  publisher replica.
- No passing AC-005 artifact is available in the immutable candidate.
- Unbounded in-memory growth during a prolonged dependency outage and abrupt
  process-loss of queued events are explicit revision-2 decisions, not defects
  reopened by this review.

## Overall result

**FAIL**

The remediation closes the live-process drop defect, preserves tenant-local
ordering, keeps unrelated tenants and request surfaces available during a
database outage, and gives graceful shutdown the required retry/deadline
behavior. It does not make ambiguous audit commits idempotent after the
publisher has retired the only dedup record, and the required deployed
capacity/scale-out acceptance proof remains missing.
