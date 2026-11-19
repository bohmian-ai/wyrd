# TASK-005 review r1 — verdict

**Verdict: FIX_REQUIRED** — FIND-005-1.

- Subject: base `b262696af`, candidate `79a7f985e`; spec revision 8 (approved);
  task `tasks/005-pull-request-extraction.md`. Round: first review, full range.
- Specialist applicability: `applicability.md`, independently validated in
  `findings-validation.md`.
- Reviewers and reports: behavior-rev (`task-review-behavior.md`, FAIL: B1
  blocking, N1/D1 non-blocking); domain-rev data layer
  (`domain-review-data.md`, PASS); ponytail-rev (`findings-validation.md`).
- Follow-up: not needed; no conflicting claims or untraced common cause.
- Acceptance: every behavioral obligation X1–X10, X12, X13 passes on source and
  re-run tests (extraction journeys 20/20, SQL 40, code-review cache 8). X11 fails
  only on the reference doc (FIND-005-1).
- Validated ledger: FIND-005-1 (INCORRECT, docs). N1 and D1 rejected as not material.
- Verification limits: `BASELINE_SQL` query plan not measured at realistic history
  sizes (shared with existing queries); in-place drop of the old completion
  function assumes no mixed-version replicas (no image published).
- Remediation: `005-R1-reference-output-shape.md`.
