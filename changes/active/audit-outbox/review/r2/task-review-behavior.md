# Behavior review — audit outbox revision 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Range: `base..candidate`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 2
- Original tasks: `changes/active/audit-outbox/tasks/*.md`
- Remediation input: `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Result: **FAIL**

The repository has no `.codegraph/` directory. `HEAD` matched the assigned
candidate before this report was written. This review inspected the cumulative
range and treated the r1 findings as closure hypotheses, not as conclusions to
repeat.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or prior disposition | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / INV-004 / AC-001 — one process audit outbox and one production append path | Boot creates one `AuditSink::outbox`; `AppState`, Gate, Oracle, peer audit, auth, and route surfaces share it. `append_audit_events` is crate-private and its only production caller is `AuditSink::write` (`audit_outbox.rs:83-107`, `audit_staging.rs:79-82`). | Source/caller search; r1 and remediation evidence. | PASS |
| REQ-002 — tenant batches, one in-flight write per tenant, bounded cross-tenant concurrency | `OutboxWriter::dispatch` removes each ready tenant's complete backlog, caps the `JoinSet` at the constructor concurrency, and `is_ready` excludes a tenant already in flight (`outbox.rs:273-305`). Audit uses four slots. | Generic tenant-independence test; `pg_audit_outbox::a_contended_tenant_does_not_delay_another_tenants_audit`; two-outbox chain test. | PASS |
| REQ-003 — requests do not wait for or fail on audit; failed sink writes remain queued and retry | `Outbox::stage` is synchronous enqueue-only. A returned sink error requeues the exact batch at the front and backs off from 50 ms to 5 s (`outbox.rs:136-148,342-373`). All audited adapters expose `stage_*`, not a commit result. | Generic retry/tenant tests and Gate/start-run/Oracle recovery journey; isolated family tests recorded in the remediation task. | PASS for ordinary sink errors; task panic loss is a separate REQ-003a violation below. |
| REQ-003a — loss only on abrupt process stop or expired graceful-shutdown deadline | Deadline abandonment is counted, but a spawned sink task panic is explicitly settled as loss while the process and writer continue (`outbox.rs:321-339`). | Deadline tests cover only explicit shutdown; no task-panic recovery test. | **FAIL — BEH-R2-002** |
| REQ-004 / AC-006 — audit-unavailable codes and live refusal contract removed | Stable public variants/mappings are removed; retained `AuditErrorCode::AuditUnavailable` and reserved schema/proto history remain historical decode state. Updated OpenAPI/doc prose describes non-blocking staging. | Recorded `codegen:check`, docs check, and served OpenAPI assertion. | PASS (r1 FIND-3 closed) |
| REQ-005 / INV-001 — decision before effect, audit after decision and before fallible protected work | Shared authorization and all six admin allowance paths stage immediately after authorization and before discovery/connection/write work (`admin/routes.rs:307,383,451,535,600,666`). | Admin failed-discovery and unavailable-connection tests plus surface journeys. | PASS (r1 FIND-2 closed) |
| REQ-006 / AC-004 — publication progress does not contend with append and retirement follows the watermark | Freeze/settle use `vala.audit_publication`; appenders use only the chain head. Settlement advances the watermark and deletes through it in one transaction (`audit_staging.rs:377-438,489-520`). | SQL lock/progress tests and publication journeys recorded green. | PASS |
| REQ-007 / AC-007 — shutdown closes intake, retries to deadline, and counts remainder | `shutdown` cancels intake, waits to the absolute deadline, then abandons and counts the pending total (`outbox.rs:175-190,385-399`). | Generic recovery/deadline tests and Postgres shutdown test. | PASS for deadline behavior; non-deadline task-panic loss is BEH-R2-002. |
| REQ-008 / AC-008 — one SQL-free generic outbox owns queue, pending, grouping, bounded concurrency, retry, idle, shutdown, and labelled metrics | `wyrd-runtime::outbox::{OutboxSink, Outbox, OutboxWriter}` is generic and SQL-free; audit supplies only `AuditSink`. Repository search finds no sibling server outbox implementation. | Five generic unit tests cover ordered retry, tenant independence, 50,000 queued items, recovery during shutdown, deadline loss, and pending zero. | **FAIL — BEH-R2-002** because the generic owner has an additional live-process loss path. |
| REQ-009 / AC-009 — retry after an unknown commit outcome never duplicates audit | A stage-time UUID and `(data_tenant_id,event_id)` unique constraint prevent duplicates only while the original row remains in `vala.audit_staging` (`audit_staging.rs:111-130`; migration `20261003000001`, lines 12-15). The publisher deletes that row after retention (`audit_staging.rs:489-520`), and retained projection does not carry `event_id`. | `rewriting_a_committed_batch_stages_each_event_once` retries before publication; no proof covers publication/retirement before retry. | **FAIL — BEH-R2-001** |
| INV-002 — committed chain stays gap-free and ordered; retry precedes later same-tenant work | The chain head is locked once, fresh events are chained in order, and failed batches are reinserted before later items (`audit_staging.rs:97-209`, `outbox.rs:365-373`). Even the retirement race creates a gap-free chain, but with a second semantic copy of the decision. | Two-replica 1,000-row hash-chain test and generic ordered-retry test. | PASS for sequence/hash ordering; REQ-009 still fails on duplicate decision content. |
| INV-003 — tenant isolation | `AuditSink` opens `tenant_conn(tenant)` for each batch; append selection/update rely on RLS and bind the tenant only for inserted rows. | Multi-tenant SQL tests and boundary evidence. | PASS (r1 FIND-6 closed) |
| AC-002 — Gate, run start, Card, auth grant, admin, and Oracle survive injected audit failures, count them, and commit once after recovery | Gate/run/Oracle share the full public journey; changed Card/auth/admin/gateway/direct-execution tests use the same injected staging failure and recovery pattern. | `a_gate_write_run_start_and_query_succeed_while_audit_commits_fail` explicitly serves the Oracle result during failure and later observes exactly one Oracle decision; task evidence records the other family tests. | PASS (r1 FIND-4 closed) |
| AC-005 / r1 FIND-5 — capacity lane | User disposition defers `mise run bench:capacity` to integration and forbids substituting a narrower run here. | Not run in this task by instruction. | DEFERRED TO INTEGRATION; not a failure in this review. |
| r1 FIND-7 — bare names in changed signatures | Previously cited signatures now use module imports and bare names. | Recorded lints. | PASS |
| r1 FIND-8 — TypeScript proof | No new TS implementation was required; remediation records install, build, testing build, N-API declaration check, typecheck, unit, and integration (29/29). | Recorded TypeScript lanes. | PASS; aggregate gate remains integration-owned per disposition. |
| r1 FIND-9 — no task IDs in permanent test rustdoc | Search of touched audit/outbox code finds no audit-packet `REQ-*`, `AC-*`, or `INV-*` identifiers. The unrelated Oracle comment predates this packet. | Static source search. | PASS |
| r1 FIND-10 — staging collaborators use `stage_*` | Gate, Oracle, and peer contracts/callers use `stage_write_decision`, `stage_read_decision`, and `stage_security_violation`; only the real SQL append and test seeding helpers retain `append_*`. | Source/caller search and recorded compilation/lints. | PASS |
| Regression boundary — unwrap audit still rejects production `unwrap`/invalid `expect` | The candidate skips every file named `tests.rs` without establishing that its parent declaration is `#[cfg(test)]` (`check_unwrap_audit.py:31-38`). The four current files happen to be test-only, but the rule is filename-based and can silently exempt a production module. | `check:unwrap-audit` is recorded green, but there is no checker test for cfg-gated versus production out-of-line modules. | **FAIL — BEH-R2-003** |
| Regression boundary — shared dependency ownership and client tier | `wyrd-runtime` gains `metrics`, `tokio-util`, and `tracing`, all directly used by the generic runtime owner; it remains SQL/cloud/DataFusion/Iceberg/PyO3-free. `vala-sql` keeps SQL in `AuditSink`. Given the approved REQ-008 seam and the task's explicit `wyrd-runtime` placement, no second implementation or server dependency enters a client crate. | Recorded `check:client-tier` and manifest/lock inspection. | PASS |
| Non-goals — no second publisher/WAL/relay, event content and retained authority remain owned as before | `AuditPublisher` remains singular; no new durable queue or audit authority exists; the event ID is staging metadata and not added to the public audit event or retained table. | Full diff and caller search. | PASS, except that omitting the ID from all post-retirement authority is the source of BEH-R2-001. |

## Proposed findings

### BEH-R2-001 — INCORRECT: publication can erase dedup evidence before an unknown-outcome retry

- **Violated obligation:** REQ-009, AC-009, the audit pattern in
  `architecture/references/architecture/patterns.md:260-272`, and
  `architecture/wyrd-security-posture.md:358-390` require a stage-time event ID
  to prevent a retry after an unknown commit outcome from staging or retaining
  the decision twice.
- **Exact location:**
  `crates/vala/vala-sql/src/queries/audit_staging.rs:111-130,449-467,489-520`;
  `crates/vala/vala-sql/src/row_types/audit_staging.rs:5-42`;
  `crates/vala/vala-bifrost-redux/src/tables/audit/projection.rs:76-88,172-235`;
  `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215`.
- **Evidence and reachable path:** `append_audit_events` checks `event_id` only
  against rows currently in `vala.audit_staging`. An audit transaction can
  commit and its client connection can fail before acknowledging that commit,
  causing the outbox to retain the same `StagedAuditEvent` for retry. Before
  its backoff elapses, the independent publisher can freeze, publish, settle,
  and delete the committed row. The retained row/projection contains no event
  ID, and no surviving Postgres uniqueness evidence remains. The retry then
  treats the ID as fresh, allocates a later sequence, and publishes the same
  authorization decision a second time. The AC-009 test repeats the write only
  while the first rows are still staged, so it cannot falsify this path.
- **Observable consequence:** retained `vala.system.audit_log` can contain two
  rows for one permission evaluation. Counts, investigations, and compliance
  evidence overstate activity even though the sequence/hash chain itself is
  gap-free.
- **Required testable correction:** preserve the stage-time identity across
  the complete uncertain-outcome window, including staging retirement, so the
  canonical append can identify an already committed-and-retired event without
  allocating a new sequence. The correction must preserve the approved single
  append path, single publisher, transient staging authority, and retained-log
  ownership; if doing so requires a new durable authority or retained public
  field, route that persistent-data decision through specification revision.
  Add a Postgres/publication test that commits one fixed-ID batch, retires it
  through the real publisher/settlement path, retries that same batch, and
  proves exactly one retained decision and no new chain sequence.

### BEH-R2-002 — VIOLATION: a sink-task panic drops accepted events while the process remains alive

- **Violated obligation:** REQ-003a says an event is lost only on abrupt process
  stop or an expired graceful-shutdown deadline. REQ-008 makes the generic
  outbox the owner of retry and loss accounting.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:321-339`.
- **Evidence:** `OutboxWriter::finish` handles `JoinError` by incrementing the
  loss counter and releasing the batch from `pending`. The item values were
  owned by the panicked spawned task, so they are gone and cannot be requeued;
  the writer loop and server process continue. The source rustdoc explicitly
  describes this as loss. This is neither an abrupt process stop nor shutdown
  deadline exhaustion.
- **Observable consequence:** a panic in any `OutboxSink::write` implementation
  silently creates a counted audit gap while requests and the server continue,
  contrary to the exhaustive accepted-loss contract. It also applies to the
  planned second generic sink.
- **Required testable correction:** keep recoverable ownership of each in-flight
  batch at the generic writer boundary until its task settles, and treat an
  isolated task failure as a failed attempt that requeues the exact items at
  the front with the ordinary backoff/failure metric. Do not convert it to the
  shutdown loss counter. Add a generic sink that panics once and prove the
  writer remains alive, the exact batch commits once ahead of later tenant
  items, and `pending` returns to zero.

### BEH-R2-003 — REGRESSION: the unwrap checker trusts `tests.rs` filenames rather than cfg reachability

- **Violated obligation:** AGENTS.md §12 and `architecture/agent-rules.md`
  prohibit weakening or broadening a gate exclusion to hide a violation. The
  checker promises to audit production Rust and exempt test code.
- **Exact location:** `scripts/check_unwrap_audit.py:31-38`.
- **Evidence:** `is_ignored_path` now returns true for every file named
  `tests.rs`; Rust does not make such a filename test-only. The four current
  matches are legitimately included by `#[cfg(test)] mod tests;`, but the
  checker does not inspect that declaration and has no regression test. A
  production `mod tests;` or another production module mapped to `tests.rs`
  is now completely outside the audit.
- **Observable consequence:** production `.unwrap()` and invalid `.expect()`
  calls can pass `check:unwrap-audit` solely because of a filename, weakening a
  repository safety gate beyond the one legitimate wiremock assertion that
  prompted the edit.
- **Required testable correction:** exempt an out-of-line module only after
  proving its owning declaration is cfg-test-only, or use a narrow documented
  allowlist of the verified current test modules. Add checker fixtures for both
  `#[cfg(test)] mod tests;` (ignored) and production `mod tests;` (audited).

## Verification assessment

- The remediation task records green focused generic, SQL, server, Bifrost
  journey, OpenAPI, TypeScript, codegen, docs, format, lint, client-tier,
  unwrap-audit, and Python-lint lanes. Their named source coverage was checked;
  they were not rerun by this read-only reviewer.
- The required capacity benchmark and aggregate gate are intentionally deferred
  to integration under the user's disposition and are not converted into local
  findings.
- The existing AC-009 test establishes idempotence only before staging
  retirement. No recorded lane exercises the producer-to-publisher race in
  BEH-R2-001.
- `git diff --check` was clean. No production or test source was edited by this
  review.

## Overall result

**FAIL.** The r1 surface, documentation, tenancy, naming, TypeScript, and
ordinary retry findings are substantially closed, and revision 2's generic
retry/shutdown behavior works for returned sink errors. Acceptance is still
incomplete because the exact unknown-commit path named by REQ-009 can duplicate
a decision after publisher retirement, the generic writer has a live-process
loss path outside REQ-003a, and the unwrap gate was broadened by filename rather
than actual cfg-test reachability.
