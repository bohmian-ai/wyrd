# TASK-004 validated remediation handoff

User-supplied review verdict: **REMEDIATE**.
Reviewed range: `027fa942a16626fef2c3a0788bca7c1cf05ee7e8..4f32eefe22e7d55d71bf1465ffaa6b9eec234317`.
Spec: approved revision 6. The reviewer changed no files, reran the HTTP journey
(7/7 passed), and supplied validation of four concrete remaining defects.
The unrelated workspace Clippy warning is not a task finding.

| Finding | Validated defect / required closure | Remediation |
| --- | --- | --- |
| FIND-TASK-004-1 | Draft-owned metadata and save history; validated closed by R1. | Preserve closure; do not reopen. |
| FIND-TASK-004-2, MAJOR | Review fits roughly one thread rather than two plus Submit review; source diff begins near 600px rather than 220px. Restore CRWM-02/03 regions and four paired 390 × 844 theme comparisons. | [R2](../tasks/TASK-004-R2-narrow-workflow-layout.md) |
| FIND-TASK-004-3, MODERATE | Seeded ledger draft inherits checkout fields. Prove unchanged resume/save retains record-owned contents through HTTP. | [R3](../tasks/TASK-004-R3-seeded-draft-roundtrip.md) |
| FIND-TASK-004-4, MODERATE | Equal-number deletion/addition rows share DOM and discussion anchors. Use validated, stable old/new coordinates through all consumers. | [R4](../tasks/TASK-004-R4-distinct-diff-anchors.md) |
| FIND-TASK-004-5, MODERATE | Every verifier points to nonexistent card_verifier_01. Render identity/version as plain text; do not introduce Verifier Cards. | [R5](../tasks/TASK-004-R5-verifier-identity-text.md) |

Source validation during planning confirmed the draft spread, ambiguous line-only
Anchor and equal-number model rows, fabricated link, and recorded failed mobile
comparisons. Findings remain within approved behavior; no spec revision is needed.
Existing [self-review](TASK-004-review.md) and failed screenshots are preserved.

Each task owns a separate observable outcome and evidence. All consume the
integrated TASK-004 candidate; there are no new inter-task behavioral dependencies.
R2 and R4 touch shared presentation files: reconcile their edits and refresh final
captures on the integrated candidate, without inventing a scheduling dependency.
R3 uses the completed R1 save behavior already present in the candidate.

Planning coverage: every open finding maps to exactly one task, with ordered RED /
GREEN scenarios, existing exact test targets, negative/consumer closure, ownership,
authority, and verification limits. The four packets are proposed, not independently
readiness-approved. Next: wyrd-task-readiness, then wyrd-implement with wyrd-ui.
Final wyrd-task-review must use the original base through the cumulative candidate,
including original TASK-004, R1, R2–R5, all findings and integrated evidence.

This handoff plans remediation only; it neither implements nor approves the task.


## Execution handoff superseded

R2–R5 have now passed readiness and been implemented and verified. See each task's
execution evidence and the new R2 captures; the earlier planning-only statement
above records the historical handoff. The original failed review remains intact.
