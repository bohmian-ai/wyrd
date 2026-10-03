# Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Remediation task:
  `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Review scope: r1 disposition closure, revision-2 obligations, and regressions
  in `base..candidate`

The candidate remained at the stated commit throughout this review. The
repository has no `.codegraph/` index, so navigation used the cumulative Git
diff, direct owner-module reads, and caller/test searches.

## Changed-surface coverage

| Surface | Owner and material symbols reviewed | Caller, consumer, and proof coverage | Result |
|---|---|---|---|
| Generic shared outbox | `wyrd-runtime::outbox::{OutboxSink, Outbox, OutboxWriter}`, stage, dispatch, retry, settle, shutdown, loss and pending metrics; new `metrics`, `tokio-util`, and `tracing` dependencies | Audit sink, server composition and shutdown, generic in-memory tests, SQL tests, task evidence for the future Eval sink | **FAIL** — shutdown does not close admission at the handle boundary and abandoned items remain pending (`MAINT-R2-02`, `MAINT-R2-03`) |
| Audit sink and idempotent append | `AuditSink`, `StagedAuditEvent`, `append_audit_events`, `AuditRows`, the event-ID migration and staging row shape | `AuditOutbox` call sites, `AuditPublisher` freeze/list/settle path, `pg_audit_outbox`, publication journeys, retained-history docs | **FAIL** — the idempotency fence disappears when publication retires staging (`MAINT-R2-01`) |
| R1 authorization-ordering remediation | Six tenant-admin handlers and their changed failure tests | Authorization helpers, discovery/projection/connection failure paths, outbox settlement assertions | PASS — the known allowance is staged before the changed handlers' fallible effect work |
| R1 collaborator naming remediation | `GateAudit`, `OracleAudit`, `PeerSecurityAudit`, production adapters and test doubles | Gate authorization, Oracle admission/security, peer authority and server composition | PASS — staging-only operations consistently use `stage_*`; the SQL owner alone retains `append_audit_events` |
| R1 tenancy/type/doc cleanup | Tenant-bound chain-head and event-ID queries; changed bare imports; task-ID removal; Rust/client/Utoipa prose | RLS-owned SQL paths, affected callers, served OpenAPI assertion, source-generated audit schemas | PASS except for the new retry guarantee in `MAINT-R2-01`; generated schema copies match their source |
| Failure injection and journey proof | `PgFixture::{fail_audit_staging, restore_audit_staging}`, server failure metrics, Gate/run-start/Oracle recovery journey, auth/admin/gateway/Card tests | Named focused and owning lanes recorded in the remediation task | PASS for the r1 family-coverage disposition; the tests do not simulate acknowledgement loss followed by publication retirement (`MAINT-R2-01`) |
| Test and static-check infrastructure | Generic outbox unit tests, `pg_audit_outbox`, server journey helpers, `scripts/check_unwrap_audit.py` | `check:unwrap-audit` call site and the out-of-line upload test module that motivated the change | **FAIL** — every file named `tests.rs` is now exempt without proving it is test-gated (`MAINT-R2-04`) |
| Architecture and operator documentation | Bifrost, security, operations, reference, and docs-site audit text | Compared with the generic owner, SQL append, publication retirement, metrics, and generated contracts | PASS for removal of the r1 transactional/WAL/fail-closed prose; **FAIL** for the unqualified no-duplicate promise in `MAINT-R2-01` |

## Material findings

### MAINT-R2-01 — Retry idempotency expires when the publisher retires staging

- **Changed location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:111-128`
  detects an event ID only in `vala.audit_staging`; the new constraint in
  `crates/vala/vala-sql/migrations/20261003000001_audit_staging_event_id.sql:12-15`
  is likewise scoped to that transient table. Existing publication settlement
  deletes the row at
  `crates/vala/vala-sql/src/queries/audit_staging.rs:489-520`. The new proof at
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215` replays only while the
  original staging rows still exist.
- **Governing rule / guide principle:** revision-2 REQ-009 and AC-009 require a
  write retried after an unknown successful commit to avoid a duplicate and
  preserve the gap-free chain. Maintainer Style requires durable and retry
  behavior to be visible and accurately proved. The module rustdoc, append
  rustdoc, migration comment, and docs-site text currently promise that a retry
  cannot stage a decision twice.
- **Concrete maintenance cost:** after a commit succeeds but its acknowledgement
  is lost, the outbox waits before retrying. If `AuditPublisher` publishes and
  garbage-collects that row during the wait, the retry's lookup sees no event
  ID, allocates a new sequence, and appends the same authorization decision a
  second time. A maintainer reading the API and its green test is told this
  state is impossible even though the task's own residual-risk note identifies
  the window. Retained history can therefore contain a duplicated and
  misrepresented decision.
- **Smallest testable correction:** make the event-ID fence remain observable
  for the complete interval in which the same outbox item can retry; it cannot
  rely solely on a row that the independent publisher is required to delete.
  Add a Postgres/system test that commits a batch, simulates the lost
  acknowledgement, publishes and settles that exact staging range, then retries
  the original `StagedAuditEvent` and proves no second sequence or retained
  event appears. If retaining that fence conflicts with the approved
  no-second-table, transient-staging, or retained-event contracts, route the
  correction through the owning persistent-data decision rather than keeping
  the current unconditional documentation claim.

### MAINT-R2-02 — Deadline-abandoned items remain permanently reported as pending

- **Changed location:** `crates/shared/wyrd-runtime/src/outbox.rs:175-190` returns
  the shared pending count as the shutdown loss result, while
  `OutboxWriter::abandon_remaining` at lines 385-399 counts loss without
  releasing the items or decrementing `outbox_pending`. The deadline test at
  lines 583-605 asserts only the returned loss count and never checks the
  post-shutdown pending state or gauge.
- **Governing rule / guide principle:** revision-2 REQ-008 defines `pending` and
  `outbox_pending{outbox}` as items not yet written and requires pending to
  return to zero; REQ-003a/REQ-007 classify deadline remainder as lost. The
  struct-centered guide requires one owner to keep lifecycle state and its
  public meaning coherent.
- **Concrete maintenance cost:** after shutdown has definitively abandoned and
  counted an item as lost, `Outbox::pending()` and the exported gauge still say
  it is pending even though the writer task no longer exists and no retry can
  occur. Tests, shutdown diagnostics, and operators cannot distinguish live
  backlog from terminal loss, and future sink users can wait on work the owner
  has already discarded.
- **Smallest testable correction:** when the writer has joined after deadline
  abandonment, return the captured loss count while atomically clearing the
  pending total and decrementing its gauge. Extend the generic deadline test to
  assert `pending() == 0` and a zero pending gauge after asserting the exact
  loss count.

### MAINT-R2-03 — `shutdown` does not stop staging when shutdown begins

- **Changed location:** `Outbox::shutdown` at
  `crates/shared/wyrd-runtime/src/outbox.rs:175-189` only cancels a token owned
  by the background writer. `Outbox::stage` at lines 136-147 retains no closed
  state and can keep sending until the receiver eventually processes the token.
  In the biased writer select at lines 244-262, ready write completions and
  `recv_many` are both ordered ahead of the stop branch.
- **Governing rule / guide principle:** revision-2 REQ-007 says graceful
  shutdown stops accepting new events and drains what was already queued. The
  owning-handle principle requires that acceptance state be discoverable and
  enforced by the public owner, rather than emerge later from scheduling inside
  its private task.
- **Concrete maintenance cost:** a caller can invoke `stage` after shutdown has
  begun and still have the item accepted, so the drain set is not fixed at the
  lifecycle boundary. A continuously ready producer can keep the receive branch
  ahead of the stop branch until the deadline. The existing test stages only
  after `shutdown` has returned, leaving the race invisible and making future
  changes to select ordering unexpectedly alter shutdown behavior.
- **Smallest testable correction:** close admission synchronously on the
  `Outbox` handle at the start of `shutdown`, before waiting for the writer, and
  have `stage` count/log any later item as lost without enqueueing it. Add a
  concurrent test that holds the writer in flight, starts shutdown, stages a
  new item before shutdown returns, and proves only the pre-shutdown set is
  drained while the later item is counted lost.

### MAINT-R2-04 — The unwrap checker now trusts a basename instead of test gating

- **Changed location:** `scripts/check_unwrap_audit.py:31-38` treats every Rust
  file named `tests.rs` as test code and skips the entire file at lines 227-233.
  The motivating file is currently test-gated by
  `crates/shared/wyrd-client/src/storage/upload/mod.rs:25-26`, but the checker
  never verifies that declaration.
- **Governing rule / guide principle:** `AGENTS.md` forbids broadening a
  boundary glob to hide a real violation and permits a check's sanctioned
  per-file mechanism only for legitimate test-only code. The repository's
  adding-and-retiring-checks rule requires a check to keep protecting the
  reachable property it names.
- **Concrete maintenance cost:** an ordinary production module can be named
  `tests.rs` and imported without `#[cfg(test)]`; every `.unwrap()` and invalid
  `.expect(...)` in it now bypasses the production audit. The docstring turns a
  current convention into a false repository-wide invariant, so maintainers
  cannot tell from the checker why a file is actually safe to omit.
- **Smallest testable correction:** replace the basename exemption with
  evidence that the specific out-of-line module is `#[cfg(test)]`-gated (or a
  narrow, documented per-file allowlist for the four currently verified
  modules). Add checker tests for both a gated out-of-line `tests.rs` and an
  identically named production module; only the former may be ignored.

## Uncertainties and calibration notes

- `OutboxWriter::finish` counts and permanently releases items when a sink task
  panics (`outbox.rs:321-340`), although revision-2 prose limits loss to process
  termination and the shutdown deadline. The production `AuditSink` has no
  expected panic path, so this is recorded as an uncertainty rather than a
  separate finding; the structured validation pass should decide whether a
  child-task panic is an in-scope reachable loss boundary.
- `Outbox::settle` says it waits for "everything staged so far," but it actually
  waits for the global pending count to reach zero, including later concurrent
  stages. Current uses are test/observation helpers with controlled producers,
  so the wording mismatch has no demonstrated production consequence in this
  candidate.
- The generic owner's type and module placement are otherwise cohesive. It owns
  the queue, retry state, concurrency, metrics, and lifecycle; the SQL-specific
  behavior is correctly isolated in `AuditSink`. The added shared dependencies
  are a real client-cone cost, but REQ-008 explicitly places the reusable owner
  in a shared crate and requires these capabilities, so that cost is not a
  finding.
- The r1 bare-type, stage-name, task-ID, live-documentation, admin-ordering,
  Oracle-family proof, TypeScript-evidence, and TenantConn-filter dispositions
  were inspected and are closed apart from the new retry guarantee described
  in `MAINT-R2-01`.

## Verification notes

- Reviewed every file in the cumulative `base..candidate` list, grouping
  mechanical route/test/doc edits under their owning symbol and tracing the
  materially changed generic outbox, audit sink, SQL append, publisher
  retirement, server lifecycle, failure fixtures, and checker through callers
  and tests.
- Reviewed the task-recorded focused, SQL, server, Bifrost, TypeScript, codegen,
  docs, format, lint, client-tier, and unwrap-check evidence. I did not rerun
  the long Postgres, journey, capacity, or aggregate lanes; the findings are
  source-level lifecycle/proof defects that those recorded healthy-path runs do
  not exercise.
- Independently ran `git diff --check base..candidate`; it passed.
- Candidate identity was rechecked before writing this report and remained
  `5a5542cbb965af99e92a3983a2ddf586412cea73`.

## Overall result

**FAIL**

The generic owner and its audit specialization are easy to locate and follow,
and the bounded r1 naming/documentation/order corrections are largely closed.
The candidate is not maintainer-complete while unknown-commit idempotency can
expire before retry, shutdown leaves terminal loss represented as live pending
work and continues accepting items after shutdown begins, and the unwrap audit
uses a basename-wide production-code exemption.
