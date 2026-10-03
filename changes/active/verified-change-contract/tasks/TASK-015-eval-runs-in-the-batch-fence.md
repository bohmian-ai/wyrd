---
id: TASK-015
kind: implementation
status: review
spec: SPEC-verified-change-contract
spec_revision: 60
requirements: [REQ-077, REQ-108, AC-014]
depends_on: [audit-outbox rev 2 merged into TASK-008 (generic Outbox, OutboxSink, AuditSink)]
---

# Create Eval runs through a batched run-request outbox

## Outcome and Value

Scribe acknowledges an Eval observation batch on its own durable boundary and
the client never waits for run creation. After that acknowledgement the
server places one run request per committed record in an in-process outbox.
A background flusher writes queued requests in one multi-row insert per
tenant, one `verifier_runs` row per matching active `observations_ready`
binding, keyed by tenant, binding, and record, so a repeated request inserts
nothing. The outbox has no count limit and never drops a request because
Postgres is slow or down: a failed flush keeps its batch and retries with
backoff. Graceful shutdown flushes it; a hard kill loses only unflushed
requests, and every loss the process can observe is counted and logged
(REQ-077, AC-014). Bindings whose owner is not runtime-active create no run
(REQ-108).

(The file name is historical; revision 59's batch-fence design was replaced
by revision 60's outbox.)

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-sql` `queries/verifier_runs.rs` owns the frame-free batch insert: one
  lock statement over the batch's subjects' bindings, per-binding resolution
  and activity, then one multi-row `INSERT ... SELECT FROM unnest(...)`
  with ordinals assigned in SQL and `ON CONFLICT DO NOTHING`.
- `wyrd-server` `verification/observations.rs` owns the outbox: unbounded
  queue, pending count, one writer task, per-tenant grouping, retained failed
  batches with exponential backoff, and `shutdown(deadline)` reporting
  unflushed requests. `app/server.rs` drains it at shutdown.
- Prohibited: a count limit, a drop on full or on failure, a Scribe or
  `vala-sql` write to `verifier_runs`, a cross-crate transaction, a client wait
  on run creation.

## Reuse the generic outbox (required)

The first attempt at this task hand-wrote `ObservationRunOutbox`, with its own
queue, pending count, stop token, writer, backoff, and per-tenant grouping. It
duplicated the audit outbox and is not allowed. This task runs only after the
audit outbox (audit-outbox spec revision 2, REQ-008 and REQ-009) has merged
into `wyrd/verified-change-contract/TASK-008`, and after that branch has been
merged into this worktree. The task then does the following.

- Adds `ObservationRunSink` (below) over the merged generic `Outbox` and
  `OutboxSink`. It writes through the multi-row
  `VerifierRunQueue::enqueue_observation_batch` (one `INSERT INTO verifier_runs
  ... ON CONFLICT DO NOTHING` per tenant batch).
- Sets `pub type ObservationRunOutbox = Outbox<ObservationRunSink>` and wires
  it where the hand-written outbox was wired (state, boot, server shutdown).
- Deletes the hand-written queue and writer.
- Keeps the parts that are not duplicates: deriving one request per record
  from an acknowledged Eval frame, the multi-row insert SQL, removing the 256
  cap, and the tests.
- Uses the generic metrics, labelled `outbox="eval_run_requests"`
  (`outbox_write_failures_total`, `outbox_events_lost_total`,
  `outbox_pending`). These replace
  `verification_observation_enqueue_failures_total` everywhere it is
  referenced.
- Does not change the generic type except for a defect that both sinks share.
  Any such change is reported in the evidence.

### Proposed implementation (approved with audit-outbox spec revision 2)

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

## Ordered Implementation Scenarios

### Scenario 1 — One insert per tenant, idempotent

**Behavior.** A batch of records for several subjects and bindings inserts one
run per (active binding, record) in one insert statement; resubmitting the
same batch inserts nothing and consumes no ordinal.

**RED.** `pg_verifier_runs` test.
`scripts/postgres/with-test-postgres.sh mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=observation_batches_insert_once_per_binding_and_record)'`

**GREEN.** `VerifierRunQueue::enqueue_observation_batch`.

**REFACTOR.** Delete the per-row `enqueue_observations` path.

### Scenario 2 — Outbox retains, retries, and flushes at shutdown

**Behavior.** With Postgres unavailable the outbox keeps every request and
retries; when it returns, exactly one run per matching binding exists;
graceful shutdown flushes queued requests; a request still unflushed at the
deadline is counted and logged.

**RED.** Runtime test in `pg_verification_runtime.rs` forcing a flush outage
(closed pool fault) and then recovery, and a shutdown flush.

**GREEN.** Outbox owner and writer.

**REFACTOR.** Remove `PENDING_LIMIT`, the semaphore, and the per-frame task.

## Acceptance Criteria

AC-014's outbox bullets pass; the existing continuous Eval journey stays
green.

## Verification and Evidence

Exact focused commands above; `mise run fmt`, `mise run lints`,
`mise run test:sql`, `mise run test:bifrost:integration:server`.

## Authority Links

- [Approved spec revision 60](../spec.md): REQ-077, REQ-108, AC-014.
- `crates/wyrd/wyrd-server/src/oracle/query_audit.rs` (outbox shape).

## Implementation Evidence

Generic outbox (`crates/shared/wyrd-runtime/src/outbox.rs`) unchanged. The
"Safe to retry" audit note above is stale: audit resolves ambiguous commits
through `pg_xact_status` (audit-outbox spec revision 4); Eval relies only on
its `(tenant, binding, record)` key with `ON CONFLICT DO NOTHING`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One multi-row insert per tenant batch, idempotent, no ordinal consumed by a repeat (Scenario 1) | `VerifierRunQueue::enqueue_observation_batch` (`wyrd-sql/src/queries/verifier_runs.rs`); per-row `enqueue_observations` absent | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E 'test(=observation_batches_insert_once_per_binding_and_record)'` | PASS |
| `ObservationRunSink` is one `OutboxSink`; `ObservationRunOutbox = Outbox<ObservationRunSink>`; hand-written queue and writer deleted | `wyrd-server/src/verification/observations.rs` | `cargo check --workspace --all-targets --all-features`; `mise run lints` | PASS |
| Wired where the hand-written outbox was (state, boot, shutdown) | `boot/mod.rs` (`ObservationRunSink::outbox`, `ObservationEnqueue` Gate hook); `state.rs` and `app/server.rs` unchanged over the alias | `mise run test:bifrost:journey:server` | PASS |
| Outage retains and retries, recovery creates one run per record, repeat adds none, graceful shutdown flushes, deadline loss reported (Scenario 2, AC-014) | generic outbox over the sink | `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=observation_outbox_retains_through_an_outage_and_flushes_at_shutdown)'` | PASS |
| Generic metrics `outbox="eval_run_requests"` replace `verification_observation_enqueue_failures_total` | `ObservationRunSink::NAME`; undecodable frame counts `outbox_events_lost_total{outbox="eval_run_requests"}` | `grep -r verification_observation_enqueue crates` → none | PASS |
| No count limit, no drop on failure, no Scribe/`vala-sql` write to `verifier_runs`, no client wait | generic outbox; Gate stages synchronously after the acknowledgement | journey + integration lanes | PASS |
| Continuous Eval journeys green under revision 60 | `eval_verification.rs` updated (below) | `mise run test:bifrost:journey:server`: 31/31 passed | PASS |

Other lanes: `mise run test:sql` (185 + 6 + 118 + 2 passed),
`mise run test:bifrost:integration:server` (89 passed), `mise run fmt`,
`mise run lints`, `git diff --check`: all clean.

### Diagnosis: two Eval journeys failing on the outbox

- **Symptom:** `continuous_eval_runs_the_terminal_matrix` ("only 0 of 28 runs
  were enqueued") and `sealed_replay_on_a_later_day_activates_once` ("only 1
  of 2 activations reached the queue"), in both the WIP and the generic outbox.
- **Evidence:** with `WYRD_LOG=info,wyrd_runtime=debug`, the trace shows
  `outbox write failed; retrying outbox="eval_run_requests" ... items=1`, then
  `items=8` every 5 s, and at shutdown `lost=8`. The sealed-replay barrier
  counted `pg_blocking_pids` backends.
- **Cause:** both tests encoded the pre-revision-60 per-frame fail-open
  enqueue. The terminal matrix refused one record for good and asserted it
  never got a run, which is a drop that REQ-077 forbids. The retained
  refused record shares its tenant's batch, so later records stayed blocked.
  The sealed-replay barrier expected one Postgres transaction per frame, but
  the outbox allows one write in flight per tenant, so the sentinel waits in
  memory.
- **Fix site:** the tests. An independent read-only diagnostician confirmed
  that production code behaves as REQ-077 requires.
  - The terminal matrix drops its refused pre-phase and its "never a run"
    assertion. `integrated_enqueue_outage_preserves_ack_and_recovers` and the
    `pg_verification_runtime` outbox test cover outage and recovery.
  - The sealed replay checks `observation_runs().pending() == 2` after the
    sentinel's acknowledgement, which is deterministic because Gate stages
    before it responds.
  - The obsolete `blocked_activations` helper is removed.

Also fixed: a Step 1 merge-resolution leftover in
`sdks/wyrd-sdk-rust/tests/observe_run.rs` (`u32::try_from` on `u16`), caught
by Clippy.

Risk (spec gap, not changed): a permanently failing item, such as a
constraint violation, blocks its tenant's backlog in either outbox until
shutdown. REQ-077 covers only a slow or unavailable Postgres.

Non-goals kept: no generic outbox change, no Scribe or `vala-sql` writes to
`verifier_runs`, and no unrelated files.
