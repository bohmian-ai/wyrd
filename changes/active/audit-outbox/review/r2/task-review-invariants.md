# Audit outbox r2 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Remediation input: `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Prior disposition inputs: `changes/active/audit-outbox/review/r1/verdict.md` and `findings-validation.md`

The repository has no `.codegraph/` directory. I reviewed the complete
base-to-candidate diff, then traced the changed outbox state from staging
through the generic writer, tenant transaction, publication, settlement, and
staging-row retirement. The candidate remained at the stated commit throughout
this review. I made no production or test changes.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: ordinary connection, append, or commit errors remain queued at the front, retry with 50 ms-to-5 s backoff, and do not block other tenants | `wyrd-runtime/src/outbox.rs:321-373` requeues an `Err` result ahead of later items; `:273-319` enforces one write per tenant and bounded cross-tenant dispatch; `AuditSink::write` returns acquisition, append, and commit errors | Generic unit tests cover retry order and tenant independence; recorded `test:bifrost:integration:sql` and the audit failure journey are green | PASS |
| REQ-003a: no live-process loss outside an expired graceful-shutdown deadline | `OutboxWriter::finish` at `wyrd-runtime/src/outbox.rs:327-340` treats a joined sink-task panic/cancellation as terminal loss, decrements pending, and continues the process | No panic/cancellation recovery test exists; the generic tests cover returned errors and deadline abandonment only | **FAIL — INV-R2-002** |
| REQ-007 / AC-007: shutdown closes intake, retries queued work through the deadline, and counts the remainder | `Outbox::shutdown` and `OutboxWriter::run` close the receiver, continue dispatch/retry, then abandon and count pending work only at the deadline; server shutdown drains transports and request trackers before the audit outbox (`app/server.rs:784-878`) | Generic recovery/deadline tests and `pg_audit_outbox::shutdown_reports_events_unwritten_at_the_deadline_as_lost`; recorded SQL lane green | PASS |
| REQ-008 / AC-008: one SQL-free generic owner provides an unbounded queue, batching, bounded tenant concurrency, retry, idle/settle, shutdown, loss accounting, and labelled metrics | `crates/shared/wyrd-runtime/src/outbox.rs` owns `OutboxSink`, `Outbox`, and private `OutboxWriter`; `AuditSink` is the SQL adapter; no SQL dependency entered `wyrd-runtime` | Five focused generic tests cover returned-error retry, tenant independence, 50k queued items, recovery during shutdown, deadline loss, and pending returning to zero | PASS for specified returned-error behavior; the broader accepted-loss invariant fails separately under INV-R2-002 |
| REQ-009 / AC-009: an unknown commit outcome cannot duplicate an audit event | `StagedAuditEvent` retains a UUID across retry and migration `20261003000001_audit_staging_event_id.sql` makes `(data_tenant_id, event_id)` unique only in `vala.audit_staging`; `append_audit_events` checks only that table | `rewriting_a_committed_batch_stages_each_event_once` replays while the first rows are still in staging; it never publishes and retires them before replay | **FAIL — INV-R2-001** |
| INV-002: committed rows remain gap-free and tenant order is preserved | The chain-head lock assigns sequence and hashes only to currently fresh events; failed returned batches are prepended before later items | Two-outbox Postgres chain test and generic retry-order test | PASS for sequence/hash continuity and ordering; exact-once event cardinality fails under INV-R2-001 |
| AC-002: Gate, run start, Card, auth, admin, and Oracle requests survive an injected audit failure; failures count; recovery commits once | Surface tests and `a_gate_write_run_start_and_query_succeed_while_audit_commits_fail` exercise the named families, generic failure counter, and recovery | Recorded server, principals, auth, and Bifrost journey lanes are green | PASS for an ordinary failed transaction followed by recovery; it does not prove the publication-retirement interleaving in INV-R2-001 |
| AC-005 / r1 FIND-5 disposition | The remediation task explicitly defers the canonical capacity run to the integrated branch and forbids substituting another benchmark | No integrated `mise run bench:capacity` artifact is part of this candidate review | PASS for the approved remediation disposition; final capacity acceptance remains deferred |
| r1 FIND-2, 3, 4, 6, 7, 9, and 10 dispositions | The diff moves admin staging ahead of fallible work, updates live prose/contracts, adds Oracle failure/recovery proof, removes redundant tenant predicates and qualified signature names, removes packet IDs from permanent rustdoc, and renames staging collaborators to `stage_*` | Recorded targeted lanes and static searches in the remediation artifact | PASS within the constrained r2 scope |
| r1 FIND-8 TypeScript proof | No replacement contract was invented; the remediation artifact records `ts:install`, builds, N-API check, typecheck, unit tests, and 29/29 integration tests | Recorded TypeScript lane evidence | PASS |
| `check:unwrap-audit` remains a truthful production check | `scripts/check_unwrap_audit.py:31-38` now ignores every Rust file named `tests.rs`, without proving it is owned by a `#[cfg(test)] mod tests;` declaration | Direct `python3 scripts/check_unwrap_audit.py` passes, but a production module named `tests.rs` is now outside the scan by construction | **FAIL — INV-R2-003** |
| `wyrd-runtime` dependency and reverse-dependency regression boundary | The generic owner adds only `metrics`, `tokio-util`, and `tracing`, all required by its specified metrics, cancellation/task tracking, and diagnostics; `Cargo.lock` adds no new third-party package versions; `cargo tree -i wyrd-runtime` confirms the existing broad client/server cone but no SQL/cloud/DataFusion dependency enters the crate | Recorded `check:client-tier` and lint results are green; local `cargo tree` inspection confirms the manifest/lock projection | PASS |
| Non-goals and tenant boundary | Audit content and the single publisher remain; `AuditSink` opens a `TenantConn`; append lookup, chain-head lock/update, settlement, and deletion remain RLS-scoped | Existing two-tenant tests and recorded SQL lane | PASS |

## Producer-to-sink and lifecycle trace

1. A surface calls `Outbox::stage`, which converts `AuditEvent` into one
   `StagedAuditEvent` and assigns its event ID once.
2. `OutboxWriter` moves one tenant backlog into a spawned `AuditSink::write`.
   A returned error restores the same items at the front; a joined task error
   does not.
3. `append_audit_events` locks the tenant chain head, reads matching event IDs
   from `vala.audit_staging`, assigns sequence/hash state only to IDs absent
   there, inserts them, and advances the head in the same transaction.
4. `AuditPublisher` freezes and publishes a sequence range. On durable Scribe
   acknowledgement, `settle_publication` advances the watermark and deletes
   every `vala.audit_staging` row through it.
5. Neither `vala.system.audit_log` nor another durable table retains the event
   ID for append-side reconciliation. Therefore the dedup proof exists only
   for the transient lifetime of a staging row, while the retry identity lives
   in process until the sink returns success.

## Proposed findings

### INV-R2-001 — publication retirement erases the only retry dedup identity

- **Classification:** INCORRECT
- **Violated obligation:** REQ-009 and AC-009 require a write retried after an
  unknown commit outcome to produce no duplicate staged or retained audit
  event. The approved audit doctrine also says each permission decision is
  recorded exactly once.
- **Exact location:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-128` and
  `:489-520`; migration
  `crates/vala/vala-sql/migrations/20261003000001_audit_staging_event_id.sql:12-15`;
  proof gap at `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215`.
- **Evidence:** The append deduplicates solely by selecting event IDs currently
  present in `vala.audit_staging`. Publication settlement deletes those rows
  through the watermark. A commit can succeed in Postgres while its client
  observes an error; before the retained in-process batch retries, the
  independent publisher can publish and retire that sequence range. The retry
  then sees no event ID, assigns the same decision a new sequence/hash, and
  inserts it again. The existing AC-009 test repeats the write before any
  publication or settlement and cannot falsify this path. The production
  publisher sweeps independently every five seconds, the writer backoff can
  reach five seconds, and neither owner serializes against the other.
- **Observable consequence:** One authorization decision can appear twice in
  retained audit history under two valid sequence numbers. The chain remains
  gap-free, so ordinary chain-integrity checks do not expose the duplicate.
- **Required testable correction:** Preserve the event identity in a durable
  dedup authority for at least the entire possible retry/reconciliation
  lifetime, and make the canonical append consult it atomically before
  allocating a new sequence. Add a Postgres/system test that orders: first
  commit succeeds with an unknown client outcome, publication durably retains
  and retires that row, then the same `StagedAuditEvent` retries; assert one
  retained decision total and no second sequence allocation. Because revision
  2 explicitly selected uniqueness only on transient `vala.audit_staging`, the
  durable owner/retention choice is a persistent-data decision that requires
  specification revision rather than an improvised downstream guard.

### INV-R2-002 — a sink task panic is converted into unapproved live-process audit loss

- **Classification:** VIOLATION
- **Violated obligation:** REQ-003a permits loss only at abrupt process stop or
  an expired graceful-shutdown deadline. REQ-003 requires accepted tenant
  batches to remain owned and retryable while the process is alive.
- **Exact location:**
  `crates/shared/wyrd-runtime/src/outbox.rs:321-340`.
- **Evidence:** Each batch and its items move into a spawned task. If that task
  returns `JoinError` (the source documentation specifically names panic), the
  writer owns only the item count, calls `count_lost`, decrements `pending`, and
  continues. It cannot restore the moved items. This is neither abrupt process
  termination nor deadline abandonment.
- **Observable consequence:** A sink panic loses a whole accepted audit batch
  while the server continues serving, and `settle` can report zero pending as
  though the batch completed.
- **Required testable correction:** Keep recoverable ownership of a batch
  across execution failure so a panicked/cancelled tenant write cannot be
  released as successfully settled while the process continues. Add a generic
  sink test that panics once, then succeeds, and prove the same items remain
  pending and commit once ahead of later tenant items. Do not convert this into
  a shared-server abort without separate lifecycle authority.

### INV-R2-003 — the unwrap audit now has a filename-wide production bypass

- **Classification:** REGRESSION / VIOLATION
- **Violated obligation:** AGENTS.md completion rules prohibit broadening a
  check's glob to hide a violation; the check must continue auditing production
  `.unwrap()` and invalid `.expect()` calls while excluding actual test code.
- **Exact location:** `scripts/check_unwrap_audit.py:31-38`.
- **Evidence:** `is_ignored_path` returns true for any path whose basename is
  `tests.rs`. The current four such files happen to be declared by
  `#[cfg(test)] mod tests;`, but the checker never validates that ownership.
  Rust permits a production module/file named `tests.rs`; every prohibited call
  in it would now be silently skipped. The motivating wiremock `.expect(1)` is
  real test code, but its legitimacy does not make the basename a test
  boundary.
- **Observable consequence:** A production `tests.rs` can introduce unchecked
  `.unwrap()` or dynamic/empty `.expect()` calls while
  `mise run check:unwrap-audit` stays green.
- **Required testable correction:** Exclude an out-of-line `tests.rs` only when
  its owning module is demonstrably gated by `#[cfg(test)]` (or use the check's
  narrow explicit test-only mechanism), and add checker fixtures proving both
  a cfg-test `tests.rs` exclusion and a production `tests.rs` finding. Preserve
  the legitimate wiremock call without a basename-wide bypass.

## Verification limits

- I inspected the recorded green Rust, Postgres, server-journey, codegen,
  documentation, client-tier, lint, and TypeScript evidence; I did not rerun
  the environment-owning Postgres or TypeScript lanes.
- `mise run check:unwrap-audit` could not acquire the sandboxed uv cache lock;
  running its exact Python script directly succeeded. That green result does
  not address INV-R2-003 because the defect is the check's exclusion rule.
- `git diff --check` is clean. The candidate commit remained unchanged and the
  worktree had no unrelated modifications before this report.

## Overall result

**FAIL**

