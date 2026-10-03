# TASK-008 closeout tracker

The goal is to close out TASK-008 and the gateway capture refactor. Every row
below must reach **Done** before the goal is complete. Update this file
whenever a status changes.

Rules for every worktree:

- Implement with `wyrd-implement`.
- Review with `wyrd-task-review`. The reviewer is always Codex `gpt-5.6-sol`
  at medium effort, and each worktree gets its own separate review.
- Accept only valid findings. Valid findings go to a fresh `wyrd-implement`
  agent, and the worktree is reviewed again until the verdict is PASS.
- Merge into `wyrd/verified-change-contract/TASK-008` only after PASS.
- Delete the worktree and its `target/` directory after merging. Keep
  `~/Documents/GitHub/wyrd/target`.

## Workstreams

| # | Work | Spec | Worktree / branch | Latest commit | Status | Next step |
|---|---|---|---|---|---|---|
| 1 | Gateway capture refactor | verified-change-contract rev 54 | merged | `de3dfaca6` | **Done** | — |
| 2 | Benchmark fixes (reviews R1–R8) | verified-change-contract rev 57 | merged into TASK-008 | `b7e135a83` | **Done.** Merged after audit. The drain now reads `outbox_pending{outbox="audit"}`, and the staged backlog compares against `vala.audit_publication`. FIND-17 Postgres proof, capacity bin 20/20, journey:server 31/31 | Full `bench:capacity` run at integration |
| 3 | Forge (redesign to the RisingWave model) | forge-concurrent-planning rev 3 (to draft; needs approval) | `~/Documents/GitHub/wyrd-forge` / `worktree-agent-ac58f45cb5b747685` | `1e5fae2c7` | User direction 2026-10-03: follow RisingWave. **Owned by the user** (taken over 2026-10-03); r3 remediation stopped, uncommitted partial FIND-19 edits left in place. Next: draft spec rev 3 (one active scheduler with standbys; Scribe counts commits in its own transaction; workers claim, plan, run, commit) | Approve rev 3, re-plan, implement, review, merge |
| 4 | Audit outbox (3 tasks) | audit-outbox rev 4 (approved on user direction) | merged into TASK-008 | `a556ac49e` | **Done.** r3 closed FIND-11 (commit-outcome check), 13, 14, 3. The remaining small FIND-12, 7, 15 were fixed in `a556ac49e` (outbox tests 8/8, journey and lints green) and merged on user direction without another review round | `AGENTS.md` §2 audit wording at closeout (human edit) |
| 5 | Verifier runtime under load (TASK-013, 014, 015) | verified-change-contract rev 61 | merged into TASK-008 | `990f922a9` | **Done.** TASK-015 r1 SPEC_REVISION_REQUIRED was resolved by rev 61: an Eval observation runs only bindings where subject matches and owner = the writer's Card; Drift and schedule keep the activity gate. New journey `eval_verification::eval_runs_follow_the_writing_owner`; journey:server 32/32, test:sql green, integration:server 89. Merged on user direction | Integrated lanes running on TASK-008 |
| 6 | Intermittent Drift journey test `verification_runtime::two_bindings_share_one_client_observation` | verified-change-contract | TASK-008 | `0ad7e348d` | **Done.** Cause: the test compared two clocks. The Drift window ended at Postgres time (Colima VM, about 80 ms behind), while the observation's event time came from the host clock, so the observation fell after the window and both verdicts were inconclusive. Fix in the shared test fixture: `make_binding_due` sets `next_run_at = GREATEST(statement_timestamp(), host now)`. An independent diagnostician confirmed the cause. Failed 6/6 before the fix, 10/10 after; journey:server 32/32 | — |

### 2. Benchmark fixes

- Implemented: one 30-minute run deadline; the judge provider wait is
  reported; one `Benchmark` owner type; cancellation docs; a step-kind enum;
  Scribe staged members counted in the backlog; a backlog drained at 60.0 s
  passes; one report table; matched CPU and memory windows; the duplicate-run
  check removed; the `bench:bifrost:query-capacity` benchmark deleted.
- Moved into normal test lanes: two-replica exactly-once claims, flooding
  tenant fairness, the failed LLM-judge verdict, and 100-feature drift landing
  exactly once with flat client bytes.
- Review finding 13 (the full default benchmark run) is done at integration,
  after workstreams 3, 4 and 5 merge.

### 3. Forge concurrent planning

- Every replica plans, using per-table demand claims that are released when
  work completes, fails, or the server shuts down.
- Batches run back to back, and periodic maintenance runs when each table is
  due.
- Worker claims are fair across tenants. All claims last 60 s.
- Gate repairs in `7b56776ff`: the unwrap audit skips out-of-line test
  modules; an SDK test's casts were fixed; one Python file was formatted.
- Open notes from the implementer:
  - A failed demand moves to the back of its tenant's queue.
  - The table publication lease is still 15 minutes, which the spec left
    unchanged.

### 4. Audit outbox

- Every audit decision goes through one batched, non-blocking outbox, like
  Oracle's.
- The audit-unavailable error codes are removed.
- Publication progress no longer lives on the chain-head row.

### 5. Verifier runtime under load (revision 59)

- REQ-077 (rev 60): Eval runs come from a batched run-request outbox like audit's. The ack covers receipt only, flushes are batched per tenant, a failed flush retries and never drops, graceful shutdown flushes, and losing unflushed requests on a hard kill is accepted.
- REQ-086 and REQ-087: results go through the server-internal capture writer.
  No tokens and no Gate are involved, and Gate refuses public writes to the
  result tables. Reads use a tokenless SYSTEM authority.
- REQ-181: there is no 64-tenant limit and no execution count cap. A run
  refused by a full shared resource returns to the queue without consuming an
  attempt.
- REQ-182: Verifier Cards are cached per process, LRU-evicted at 64 MiB.
- REQ-183: each run's result is stored once in Postgres and written from
  those stored bytes every time, so a run is never executed twice.
- REQ-184: leases are renewed with one statement per tenant.
- REQ-185: a run holds a connection only to claim, store its result, settle,
  and renew its lease.
- Drift runs its SQL and scores what comes back. It has no completeness check.
- Proof: AC-044, plus the revised AC-014, AC-023, AC-030 and AC-043.

### Shared outbox machinery (audit-outbox rev 2 REQ-008/009, then TASK-015)

The first TASK-015 attempt hand-wrote an Eval run-request outbox that copied
the audit outbox's queue and writer code. That copy is not allowed. There is
one generic outbox type with separate instances for audit and Eval. They write
to different crates' databases, and one slow database must not delay the
other. Sharing the type costs no throughput, because each instance has its own
queue and writer. If one writer is ever measured as the limit, write tenants
concurrently inside the generic type. Never add a second outbox.

#### Proposed implementation (approved with audit-outbox spec revision 2)

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

## Order of work

1. Now, in parallel: the audit outbox r1 review and fix loop; the benchmark
   R5 fixes then the R6 review (which checks only that the R5 findings are
   closed and nothing regressed); the Forge r2 fixes then the r3 review; and
   verifier runtime TASK-014.
2. Audit outbox r1 remediation under spec rev 2 (approved). It covers retry
   without drop, the generic `Outbox`/`OutboxSink`, `AuditSink`, the event-ID
   migration, and the nine other r1 findings. Then the r2 review until PASS,
   then merge into `TASK-008` first. The benchmark and the verifier runtime
   both depend on it.
3. Merge Forge any time after PASS. It does not overlap with the others.
4. Merge the benchmark after the audit outbox. While merging, point its
   pending-audit metric at the new audit outbox and rerun its focused tests.
5. Verifier runtime, last:
   - merge the current `TASK-008` into its worktree;
   - run TASK-015 (only `ObservationRunSink` and its wiring over the merged
     generic outbox, plus deleting the duplicate writer);
   - its own Codex review until PASS;
   - merge.
6. Closeout steps below, on the fully merged branch.

## Integration and closeout (after workstreams 2–5 pass review)

| Step | Status |
|---|---|
| Merge the approved branches into `wyrd/verified-change-contract/TASK-008` | Pending |
| `mise run bench:capacity` (default run) with results recorded (review finding 13) | Pending |
| `mise run bench:capacity -- --profile` | Pending |
| Scenario 7: optimizations with before/after numbers, or a recorded justification for none | Pending |
| Replace every `GATE_PENDING` in the CLOSE-01 matrix | Pending |
| `mise run gate`, run once | Pending |
| `git diff --check` | Pending |
| Evidence tables in `tasks/task-008-closeout.md` | Pending |
| Commits ending with the session trailer | Pending |
| Delete the agent worktrees and their `target/` directories; keep `wyrd/target` | Pending |
