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

## Implementation Evidence

Result: IMPLEMENTED.

### Acceptance and finding matrix

| Obligation | Implementation | Proof | Result |
|---|---|---|---|
| REQ-008 generic outbox (FIND-1) | `crates/shared/wyrd-runtime/src/outbox.rs`: `OutboxSink`, `Outbox<S>`, private `OutboxWriter<S>`. Unbounded queue, per-tenant grouping, at most one write in flight per tenant, `concurrency` slots, front-of-queue retry with backoff from 50 ms doubling to 5 s, shutdown flush to the deadline and loss counting, plus `outbox_pending`, `outbox_write_failures_total`, and `outbox_events_lost_total` labelled `{outbox}`. Nothing audit-specific; no SQL dependency. | `wyrd-runtime` `outbox::tests::*` (5): ordered single retry, tenant independence, a 50k-item queue with no limit, shutdown flushes items that recover, shutdown counts items left at the deadline as lost | PASS |
| REQ-003, REQ-003a: no cap, no drop, no preallocation | `vala-sql/src/audit_outbox.rs` reduced to `AuditSink` plus `type AuditOutbox = Outbox<AuditSink>` (concurrency 4). `QUEUE_EVENTS`, `BATCH_EVENTS`, the writer, and `record_commit_failure` deleted. | same as above, plus the AC-002 lanes below | PASS |
| REQ-009 / AC-009: event-id dedup | Migration `20261003000001_audit_staging_event_id.sql` adds `UNIQUE (data_tenant_id, event_id)`. `StagedAuditEvent` assigns a v7 id when staged. `append_audit_events` skips ids already staged under the chain-head lock without consuming a `seq`. | `vala-sql` `pg_audit_outbox::rewriting_a_committed_batch_stages_each_event_once` (3 rows, gap-free chain) | PASS |
| AC-007: shutdown | Shutdown keeps retrying until the deadline, then abandons and counts. | `pg_audit_outbox::shutdown_reports_events_unwritten_at_the_deadline_as_lost` and `two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown`; unit shutdown tests | PASS |
| AC-008: tenant independence | Per-tenant in-flight set; retry never blocks other tenants. | `pg_audit_outbox::a_contended_tenant_does_not_delay_another_tenants_audit`; unit `a_failing_tenant_does_not_delay_another_tenant` | PASS |
| AC-002: retry and recovery on every surface family | The Gate, verification, and Oracle journey was rewritten to: trigger failure, request succeeds, failure counted, nothing staged, drop trigger, decision commits exactly once. Auth, admin, gateway administration and invocation, card registration, completion and delete, CLI, platform, OpenAPI exchange, and direct execution tests all follow the same pattern. | `wyrd-testing::server audit_publication::a_gate_write_run_start_and_query_succeed_while_audit_commits_fail`; `wyrd-server` gateway `gateway_invocation_dispatches_without_waiting_for_the_audit_append` and `gateway_failed_operations_keep_one_allowed_decision_and_never_wait_on_audit`; `pg_verification_routes::direct_execution_does_not_wait_on_audit`; `pg_card_registration_route::*_succeeds_when_its_decision_audit_fails`; `wyrd-auth` `a_refused_exchange_audit_still_issues_the_token`, `an_unrecordable_decision_still_authorizes`, `an_unrecordable_grant_still_returns_the_session` | PASS |
| FIND-4: Oracle failure proof | The AC-002 journey runs a real Oracle query while staging fails. It asserts the row is served, the admin's read decision is not staged, and after recovery exactly one `bifrost.query.read_decision` from that admin is retained. | journey above | PASS |
| FIND-2: admin allowance staged before fallible work | All six admin handlers stage right after authorization. The old test, which asserted that a failed discovery leaves no allowance, was rewritten to the approved behavior. | `admin::routes::pg_tests::a_failed_discovery_keeps_its_allowance_and_creates_no_issuer`, `a_binding_create_without_a_connection_stages_its_allowance` | PASS |
| FIND-6: no manual tenant predicates | The chain-head `SELECT ... FOR UPDATE` and `UPDATE` rely on `TenantConn` RLS. | `mise run test:bifrost:integration:sql` | PASS |
| FIND-7: bare type names | `state.rs` (`OracleRuntimeInspection`), server `auth/callback.rs` (`Value`), `oracle/analytical.rs` test authority (peer types, `PeerContext`) | `mise run lints` | PASS |
| FIND-9: no packet ids in permanent rustdoc | `pg_audit_outbox.rs` docs | grep for `AC-|REQ-|INV-` in touched code: empty | PASS |
| FIND-10: `stage_*` names | `GateAudit::stage_write_decision`, `OracleAudit::stage_read_decision` / `stage_security_violation`, `PeerSecurityAudit::stage_*`, plus their impls, doubles, and callers. `append_audit_events` keeps its name. | `cargo check --workspace --all-features --tests`; lints | PASS |
| FIND-3: live prose and contract | Security posture, operations README / reliability / runbooks, patterns, architecture-constraints, rust-core, vala-architecture, bifrost-design (`outbox_write_failures_total{outbox="audit"}`), docs `bifrost/architecture.svx`, `AuditEvent` rustdoc (both schemas regenerated), auth, server, and client rustdoc, and platform / gateway / auth utoipa descriptions. New served-document assertion. | `pg_openapi_contract::no_operation_documents_an_audit_caused_refusal`; `mise run codegen:check`; `mise run docs:check` | PASS |
| FIND-8: TypeScript lane | n/a | `ts:install`, `ts:build`, `ts:build:testing`, `ts:napi:check`, `ts:typecheck`, `ts:test:unit`, and `ts:test:integration` (29/29) | PASS |

### Lanes

| Command | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::/)'` | 5/5 PASS |
| `mise run test:bifrost:integration:sql` (vala-sql, includes `pg_audit_outbox` and `pg_migration`) | 119/119 PASS |
| `mise run test:principals:integration` (includes `pg_openapi_contract` and admin routes) | PASS |
| `mise run test:wyrd` | 2327/2329 on the first run. The 2 failures were the gateway-invocation and direct-execution audit tests, still written for drop semantics; both were rewritten and then passed focused (below) |
| `scripts/postgres/with-test-postgres.sh -- … cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_invocation_dispatches_without_waiting_for_the_audit_append)'` | PASS |
| `scripts/postgres/with-test-postgres.sh -- … cargo nextest run --locked -p wyrd-server --test pg_verification_routes -E 'test(=direct_execution_does_not_wait_on_audit)'` | PASS |
| `mise run test:bifrost:journey:server` | 30/30 PASS |
| `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^gate::/) \| test(/^oracle::peer/)'` | 39/39 PASS |
| TypeScript lanes listed above | PASS |
| `mise run codegen:check`, `mise run docs:check` | PASS |
| `mise run fmt`, `mise run lints`, `mise run check:client-tier`, `mise run check:unwrap-audit`, `mise run py:lints`, `git diff --check` | PASS |

`check:unwrap-audit` was red on `crates/shared/wyrd-client/src/storage/upload/tests.rs:138`, a wiremock `.expect(1)` call-count assertion. The checker treated the out-of-line `#[cfg(test)] mod tests;` body `tests.rs` as production code. `scripts/check_unwrap_audit.py` now classifies a file named `tests.rs` as test code, which is the property the check already intends.

Not run here: `mise run bench:capacity` (FIND-5, integration) and `mise run gate` (integration).

### Residual risks

- REQ-009 dedup checks ids still present in `vala.audit_staging`. If the publisher retires a row after a commit with an unknown outcome but before that batch's retry, the retry could stage it again. The window is one retry backoff (at most 5 s) against publication latency.
- `wyrd-runtime` now depends on `metrics`, `tokio-util`, and `tracing` unconditionally. `check:client-tier` passes, but `metrics` is newly in the client cone.
- `AGENTS.md` §2 still says "surfaces not yet converted still append in the deciding transaction" and "a failed commit is logged and counted". Both are now stale. Agent configuration is left for a human edit.
