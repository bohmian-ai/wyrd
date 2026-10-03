---
id: TASK-AUDIT-OUTBOX-R2-BOUNDED
kind: remediation
status: review
spec: SPEC-audit-outbox
spec_revision: 3
requirements: [REQ-003, REQ-003a, REQ-007, REQ-008, REQ-009, AC-008, AC-009]
remediates: [FIND-AUDIT-OUTBOX-11, FIND-AUDIT-OUTBOX-12, FIND-AUDIT-OUTBOX-13, FIND-AUDIT-OUTBOX-14, FIND-AUDIT-OUTBOX-7, FIND-AUDIT-OUTBOX-3]
route_to: wyrd-implement
---

# Close the bounded audit-outbox r2 findings

## Authority

- Approved spec: `changes/active/audit-outbox/spec.md` revision 3 (approved
  in commit d61114979; it rewrites REQ-009 and AC-009). Do not edit `spec.md`.
- Findings: `review/r2/verdict.md` and `review/r2/findings-validation.md`. The
  integrator accepted FIND-12, 13, 14, 7, and 3 exactly as written there.
  Implement each one's "Ponytail correction" and prove it with its "Focused
  closure proof". The coordinator added FIND-11 to this task's scope after
  revision 3 was approved.
- Out of scope: FIND-5 / `mise run bench:capacity`, and `mise run gate` (both
  run at integration).

## Findings to close

1. **FIND-12:** if a sink task panics, its batch is lost. Catch the panic
   inside the child task while that task still holds the items, return them,
   and send them through the existing front-of-queue retry with backoff and
   the write-failure metric. Do not add a second queue, require cloning, or
   abort the process.
2. **FIND-13:** shutdown must be a one-way fence owned by the handle. When
   shutdown is called, close the staging sender before waiting. Items staged
   before that point still drain. Later `stage` calls are refused and counted,
   and never enter the queue. If the deadline abandons work, return the exact
   loss count, and clear that count from the pending atomic and from
   `outbox_pending{outbox}`.
3. **FIND-14:** delete the rule in `scripts/check_unwrap_audit.py` that skips
   every file named `tests.rs`. Replace it with an explicit allowlist of the
   four current `#[cfg(test)] mod tests;` files, each entry naming the file
   that declares it. Add checker fixtures with identical bodies: the
   allowlisted file is skipped, and a production `tests.rs` is scanned and
   rejected.
4. **FIND-7:** import `MutexGuard` and `Uuid` at the top of the module, and
   use bare names in the declarations listed in the finding.
5. **FIND-3:** replace the phrase "Oracle audit commit failure" in
   `architecture/wyrd-security-posture.md` with the shared audit-outbox
   wording that the surrounding text already uses.
6. **FIND-11 (revision 3):** carry `event_id` into every retained
   `vala.system.audit_log` row through the existing single publisher. Keep
   staging unique per (tenant, event ID). Accept that a retry after retirement
   may add one duplicate retained row. Collapse rows that share
   (tenant, event ID) in audit reads that count or list decisions. Add no
   second audit table, ledger, WAL, relay, retirement delay, or publisher.

## Verification

- Rerun the focused outbox and audit tests through their exact
  `mise exec -- cargo nextest run` commands, including the Postgres-backed
  `pg_audit_outbox` target through its repository setup wrapper or owning
  `mise` task.
- Run the new FIND-12 and FIND-13 tests the same way.
- Also run: the checker's own tests, `mise run check:unwrap-audit`,
  `mise run fmt`, `mise run lints`, `mise run docs:check`, and
  `git diff --check`.
- Append the evidence table that `wyrd-implement` requires to this file.

## Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-11: every retained row carries `event_id`, written by the existing single publisher | `vala-bifrost-redux/src/tables/audit/{audit_log,projection,mod}.rs` (15th content column `event_id`); `vala-sql` staging queries and `AuditStagingRow` select `event_id` | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^tables::audit::/)'`: 7/7; `mise run test:bifrost:integration:sql`: 119/119 | PASS |
| FIND-11: staging stays unique per (tenant, event ID); a retry after retirement adds at most one retained duplicate; reads collapse by event ID | `append_audit_events` unchanged (`ON CONFLICT` on the unique key); `wyrd-testing/src/server.rs` `retained_audit_records` / `retained_audit_rows` collapse by `event_id`; no new table, ledger, WAL, relay, delay or publisher | journey `audit_publication::unknown_outcome_retries_retain_each_decision_at_most_twice_and_read_once` (production `AuditPublisher`; phase 1: retry while still staged gives 1 retained row; phase 2: retry after retirement gives 2 raw rows, both ordered before the later event, collapsed count and listing = 1, outbox drains with 0 lost) through `scripts/postgres/with-test-postgres.sh -- ... cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::unknown_outcome_retries_retain_each_decision_at_most_twice_and_read_once)'`: PASS; `mise run test:bifrost:journey:server`: 31/31 | PASS |
| FIND-12: a sink panic is retried at the front of the queue with backoff and the write-failure metric, with no loss | `wyrd-runtime/src/outbox.rs` `contain_panic` + `catch_unwind` inside the dispatch task, which keeps ownership of the items | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::a_panicking_write_is_retried_once_in_order_without_loss)'`: PASS | PASS |
| FIND-13: shutdown closes the staging sender before waiting; later stages are refused and counted; deadline loss is exact and cleared from the pending count and `outbox_pending` | `Outbox::shutdown` takes the sender under the `RwLock` write lock; `stage` checks under the read lock | `-E 'test(=outbox::tests::shutdown_refuses_items_staged_after_it_begins)'` and `-E 'test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'` (lost counter 3, `pending()` 0, gauge 0): PASS; `-E 'test(/^outbox::/)'`: 7/7; `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-sql --test pg_audit_outbox`: 4/4 | PASS |
| FIND-14: no blanket skip of files named `tests.rs`; an explicit allowlist of four entries, each naming its declaring file | `scripts/check_unwrap_audit.py` `CFG_TEST_MODULES`; fixtures in `scripts/test_check_unwrap_audit.py` | `mise exec -- python3 scripts/test_check_unwrap_audit.py`: passed (allowlisted file skipped, production `tests.rs` with an identical body rejected); `mise run check:unwrap-audit`: PASS | PASS |
| FIND-7: `MutexGuard` and `Uuid` imported at the top of the module and used by bare name | `outbox.rs` test module; `vala-sql/src/queries/audit_staging.rs` non-test declarations | `mise run lints`: PASS | PASS |
| FIND-3: the "Oracle audit commit failure" wording is replaced with the shared audit-outbox wording | `architecture/wyrd-security-posture.md` | `mise run docs:check`: PASS | PASS |

Lanes and checks: `mise run test:wyrd` 2329/2329; focused audit tests (`vala-sql --lib /audit/` 3/3, `wyrd-auth --lib` 119/119, `wyrd-server oracle::peer_audit::` 2/2, `vala-bifrost-redux --lib` 810/810); `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run check:client-tier`, `mise run check:unwrap-audit`, ruff check and format on `scripts/test_check_unwrap_audit.py`, `git diff --check`: all pass.

Diagnosis, recorded for the journey's first run:
- **Symptom:** the new journey failed with `Audit("invalid or unsupported query SQL ...")`.
- **Evidence:** rerun with `WYRD_LOG` tracing showed no refusal line from the server. A scratch DataFusion check (since reverted) reproduced a Plan error for `SELECT event_id, CAST(seq AS VARCHAR) ... ORDER BY seq`.
- **Cause:** the cast projection's name clashed with the `ORDER BY seq` column, and `CAST` also yields Utf8View.
- **Fix site:** the new test's own projection, now `arrow_cast(seq, 'Utf8') AS seq_text`. No production code or existing assertion changed.

Non-goals: FIND-5 / `mise run bench:capacity` and `mise run gate` were not run (both run at integration). `spec.md` is untouched. The checker fixtures are not wired into a mise task, because AGENTS.md says not to add a check that verifies another check.

Material limits:
- The retained `audit_log` schema fingerprint changes with no compatibility path, following repository precedent for unshipped schemas.
- Generic-SQL readers of `vala.system.audit_log` must collapse rows by `event_id` themselves.
- In journey phase 1, the server publisher could settle a row before the test fences it. That race fails loudly, the same accepted pattern as `frozen_audit_range_replays_once_while_its_tail_waits`.
- The `JoinError` loss branch remains only for panics that escape containment.
