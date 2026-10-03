---
id: TASK-AUDIT-OUTBOX-R1
kind: remediation
status: ready
spec: SPEC-audit-outbox
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-003a, REQ-007, REQ-008, REQ-009, AC-002, AC-006, AC-007, AC-008, AC-009]
remediates: [FIND-AUDIT-OUTBOX-1, FIND-AUDIT-OUTBOX-2, FIND-AUDIT-OUTBOX-3, FIND-AUDIT-OUTBOX-4, FIND-AUDIT-OUTBOX-6, FIND-AUDIT-OUTBOX-7, FIND-AUDIT-OUTBOX-8, FIND-AUDIT-OUTBOX-9, FIND-AUDIT-OUTBOX-10]
route_to: wyrd-implement
---

# Close audit-outbox r1 under spec revision 2

## Authority

- Approved spec: `changes/active/audit-outbox/spec.md` revision 2. It adds
  retry without drop, the generic outbox (REQ-008), and audit event-ID dedup
  (REQ-009).
- Findings: `review/r1/verdict.md` and `review/r1/findings-validation.md`. The
  integrator accepted every finding, with these dispositions.
  - **FIND-1:** resolved by revision 2. Implement REQ-003, REQ-003a, REQ-007,
    REQ-008, and REQ-009 exactly as specified.
  - **FIND-2, 3, 4, 6, 7, 9, 10:** implement as written in
    `findings-validation.md`. For FIND-4, use the generic failure metric
    `outbox_write_failures_total{outbox="audit"}` and also assert that the
    event commits after recovery. For FIND-10, the staging collaborators
    become `stage_*`; the real SQL append keeps its name.
  - **FIND-8:** run the TypeScript lane that covers the touched native binding
    and integration test. The one `mise run gate` runs at integration, not
    here.
  - **FIND-5:** deferred to `mise run bench:capacity` on the integrated branch.
    Do not run or replace it here.

## Outcome

- Audit never drops an event while the process is alive. A failed tenant
  write is retried at the front of its queue with backoff, and other tenants
  keep committing.
- The queue and writer live in one generic `Outbox<S: OutboxSink>` in a shared
  crate with no SQL dependency, and audit is its first sink. The Eval
  run-request outbox will be its second sink in verified-change-contract
  TASK-015, so the generic type must not contain anything specific to audit.
- A retry after an unknown commit outcome cannot duplicate staged audit.
- The queue cap, drop-on-full, drop-on-failure, and preallocation are deleted.

## Proposed implementation (approved with audit-outbox spec revision 2)

Both outboxes write through one function that inserts into one table:

| Outbox | Function the sink calls | Table |
|---|---|---|
| Audit | `append_audit_events` | `vala.audit_staging` |
| Eval run requests | `VerifierRunQueue::enqueue_observation_batch` | `verifier_runs` |

The generic outbox owns the queue and the writer. Each use supplies a small
sink that makes that one call.

**Generic part.** It goes in a shared crate with no SQL dependency (proposed:
`crates/shared/wyrd-runtime/src/outbox.rs`).

```rust
/// One destination an outbox writes to. Two real implementations: audit, Eval run requests.
pub trait OutboxSink: Send + Sync + 'static {
    /// What gets queued (AuditEvent, ObservationRecord).
    type Item: Send + 'static;
    type Error: std::fmt::Display + Send;
    /// Label for metrics and logs: "audit", "eval_run_requests".
    const NAME: &'static str;

    /// Write one tenant's items in one transaction, all or nothing.
    /// A write may be retried, so it must be safe to repeat.
    fn write(&self, tenant: DataTenantId, items: &[Self::Item])
        -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// The shared outbox. Callers only ever see this.
pub struct Outbox<S: OutboxSink> {
    queue: mpsc::UnboundedSender<(DataTenantId, S::Item)>, // no count limit
    pending: Arc<AtomicUsize>,                             // queued + being written
    idle: Arc<Notify>,                                     // fires when pending reaches 0
    stop: CancellationToken,
    writer: TaskTracker,
}

impl<S: OutboxSink> Outbox<S> {
    /// Starts the writer. `concurrency` = most tenants written at once (DB connections it may hold).
    pub fn new(sink: S, concurrency: usize) -> Arc<Self>;
    /// Queue one item. Never blocks, never fails a request.
    pub fn stage(&self, tenant: DataTenantId, item: S::Item);
    /// Items not yet written; exported as the `outbox_pending{outbox}` gauge (the benchmark reads it).
    pub fn pending(&self) -> usize;
    /// Wait until everything queued so far is written (tests).
    pub async fn settle(&self, deadline: Instant) -> usize;
    /// Stop taking items, keep writing and retrying until `deadline`, return how many were lost.
    pub async fn shutdown(&self, deadline: Instant) -> usize;
}

/// Private background task.
struct OutboxWriter<S: OutboxSink> {
    sink: Arc<S>,
    requests: mpsc::UnboundedReceiver<(DataTenantId, S::Item)>,
    waiting: HashMap<DataTenantId, Vec<S::Item>>,         // per-tenant queue, in arrival order
    writing: JoinSet<(DataTenantId, Vec<S::Item>, Result<(), S::Error>)>,
    in_flight: HashSet<DataTenantId>,                     // at most one write per tenant
    retry_at: HashMap<DataTenantId, (Instant, Duration)>, // backoff: 50 ms doubling to 5 s
    concurrency: usize,
    pending: Arc<AtomicUsize>,
    idle: Arc<Notify>,
    stop: CancellationToken,
}
```

The writer loop waits on four things at once: new items, finished writes, the
next retry time, and shutdown.

- **Dispatch.**
  - Pick each tenant that has items waiting, has no write in flight, and is
    not backing off.
  - Stop when `concurrency` writes are running.
  - Take that tenant's whole list and spawn `sink.write(tenant, &items)`.
- **Success.**
  - Subtract the items from `pending` and clear that tenant's backoff.
  - Fire `idle` when `pending` reaches 0.
- **Failure.**
  - Put the items back at the front of that tenant's list, ahead of anything
    that arrived meanwhile, so order is kept.
  - Double the backoff, log the error, and increment
    `outbox_write_failures_total{outbox}`.
  - Other tenants keep going.
- **Shutdown.**
  - Stop taking items and keep running until everything is written or the
    deadline passes.
  - Count the remainder in `outbox_events_lost_total{outbox}`.

No memory is preallocated. A sink may split a batch inside its own
transaction only to fit one statement's parameter limit.

**The two sinks.**

```rust
// crates/vala/vala-sql/src/audit_outbox.rs
pub struct AuditSink { vala: ValaPostgres }

impl OutboxSink for AuditSink {
    type Item = AuditEvent;
    type Error = sqlx::Error;
    const NAME: &'static str = "audit";
    async fn write(&self, tenant: DataTenantId, events: &[AuditEvent]) -> Result<(), sqlx::Error> {
        let mut conn = self.vala.tenant_conn(tenant).await?;
        append_audit_events(&mut conn, events).await?; // INSERT INTO vala.audit_staging; skips event IDs already staged
        conn.commit().await
    }
}
pub type AuditOutbox = Outbox<AuditSink>;              // concurrency = 4

// crates/wyrd/wyrd-server/src/verification/observations.rs
pub struct ObservationRunSink { postgres: WyrdPostgres, runs: VerifierRunQueue }

impl OutboxSink for ObservationRunSink {
    type Item = ObservationRecord;
    type Error = WyrdSqlError;
    const NAME: &'static str = "eval_run_requests";
    async fn write(&self, tenant: DataTenantId, records: &[ObservationRecord]) -> Result<(), WyrdSqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        self.runs.enqueue_observation_batch(&mut conn, records).await?; // INSERT INTO verifier_runs ... ON CONFLICT DO NOTHING
        conn.commit().await
    }
}
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;
```

**Safe to retry.**

- Eval is keyed by (tenant, binding, record), so a repeated insert does
  nothing.
- Audit (audit-outbox REQ-009) is made safe the same way:
  - each `AuditEvent` gets an event ID when it is staged;
  - a migration makes (tenant, event ID) unique on `vala.audit_staging`;
  - `append_audit_events` skips event IDs already staged, without consuming a
    `seq` or breaking the hash chain.

**Deleted.**

- The 16,384 queue cap and drop-on-full.
- `BATCH_EVENTS` preallocation.
- Drop-on-failure in `commit_tenant` and `record_commit_failure`.
- `AuditOutboxWriter`, which moves into the generic writer.
- The hand-written `ObservationRunOutbox` writer from the first TASK-015
  attempt.

**Who builds what.**

- The audit-outbox r1 remediation builds the generic `Outbox`, the
  `OutboxSink` trait, `AuditSink`, and the event-ID migration.
- TASK-015 then adds only `ObservationRunSink` and its wiring, on top of the
  merged audit outbox.


## Constraints

- No caps unless justified and no preallocation. The bounded writer
  concurrency (4 for audit) is justified as its share of the Vala connection
  pool.
- Do not add a second queue, a WAL, a relay, or per-surface metric labels.
- The hash chain stays gap-free and ordered (INV-002). The REQ-009 skip must
  not consume a `seq`.
- Do not weaken, ignore, or delete tests. Tests that encoded the old drop
  behavior (`audit_publication.rs` permanent-loss proof) must be rewritten to
  prove retry and recovery (AC-002).

## Verification

- Focused exact commands (`mise exec -- cargo nextest run --locked -p <crate>
  --lib|--test <target> -E 'test(=...)'`, with the repository Postgres wrapper
  where needed) for AC-002, AC-007, AC-008, and AC-009.
- The owning `mise` lanes for vala-sql, the shared crate, wyrd-server, and the
  Bifrost audit journeys; the TypeScript lane (FIND-8); `mise run
  codegen:check` (AC-006); and `mise run test:principals:integration` (OpenAPI
  served document).
- `mise run fmt`, `mise run lints`, `mise run check:client-tier`, `mise run
  check:unwrap-audit`, and `git diff --check`.
- Record an evidence table in this file.
