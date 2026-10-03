---
id: TASK-AUDIT-OUTBOX-R2-BOUNDED
kind: remediation
status: ready
spec: SPEC-audit-outbox
spec_revision: 2
requirements: [REQ-003, REQ-003a, REQ-007, REQ-008, AC-008]
remediates: [FIND-AUDIT-OUTBOX-12, FIND-AUDIT-OUTBOX-13, FIND-AUDIT-OUTBOX-14, FIND-AUDIT-OUTBOX-7, FIND-AUDIT-OUTBOX-3]
route_to: wyrd-implement
---

# Close the bounded audit-outbox r2 findings

## Authority

- Approved spec: `changes/active/audit-outbox/spec.md`, revision 2 is the
  approved authority. The working copy may show a draft revision 3 that only
  rewrites REQ-009 and AC-009. It is not approved. Do not implement it, and do
  not edit `spec.md`.
- Findings: `review/r2/verdict.md` and `review/r2/findings-validation.md`. The
  integrator accepted FIND-12, 13, 14, 7, and 3 exactly as written there.
  Implement each one's "Ponytail correction" and prove it with its "Focused
  closure proof".
- Out of scope: FIND-11 (it waits for a human decision on spec revision 3),
  FIND-5 / `mise run bench:capacity`, and `mise run gate` (both run at
  integration).

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
