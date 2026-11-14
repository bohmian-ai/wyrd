---
task: TASK-009-R1
source_review: 009-reports-github-delivery-r1-4d001e6d9
approved_spec_revision: 11
---

# TASK-009-R1 — Report delivery corrections

Remediate the round-1 review of TASK-009 (candidate 4d001e6d9). The contract is
[task 009](../../tasks/009-reports-and-github-delivery.md), spec revision 11 and
the validated findings in [findings-validation.md](findings-validation.md).

In scope: FIND-009-1, -3, -4, -5, -7, -8 and -10.

Deferred by the user as accepted limits in spec revision 11, with no work here:
FIND-009-2 (extraction-shaped decoy), FIND-009-6 (recheck after lookup),
FIND-009-9 (recovery shutdown under cooldown).

Write set: `crates/wyrd/wyrd-server/src/components/github/{delivery,presentation,app}.rs`,
`crates/wyrd/wyrd-server/src/verification/inputs.rs`,
`crates/wyrd/wyrd-server/src/components/verification/service.rs` and
`service/inspection.rs`, `crates/wyrd-spec/src/verification/report.rs` (docs
only), their tests and `pg_github_delivery.rs`, plus the user guide.

## Obligations

| ID | Promise | Contract source | Planned proof |
|---|---|---|---|
| FIND-009-1 | A report renders without a retained extraction: summary, unlocated findings, invalid/ambiguous reasons; supplied anchors refused `anchor_source_unavailable`; verdict unchanged | REQ-012/013, RENDER | presentation units: one/malformed/multiple without extraction |
| FIND-009-3 | An anchor validates against either retained side of a changed file (base under `previous_path` on rename) or a selected context span; only head-side hunk locations are inline; unretained ranges still refuse | RENDER, wyrd-design report delivery | presentation units: base evidence, context evidence, rename side, out-of-span refusal |
| FIND-009-4 | Every valid rendered report gets one `COMMENT` review, also with zero inline comments; retry/restart finds it by marker | REQ-013, wyrd-design | presentation unit plus existing PG empty-report journey updated |
| FIND-009-5 | Cooldown honors the full `Retry-After` and the primary rate-limit reset; attempts stay bounded | REQ-014 | app units on the classifier |
| FIND-009-7 | A waiting delivery never updates an existing Check; terminal delivery updates in place | REQ-013, wyrd-design | PG journey: terminal Check first, late waiting delivery leaves it completed |
| FIND-009-8 | A cancelled execution delivers once more under its revision, separate from that revision's outcome; the Check is updated in place to `cancelled`; inspection reads the cancelled outcome; public cancel starts it; restart recovers it | REQ-014 rev 11, bifrost-design | PG journey: park → waiting delivered → public cancel → same Check `cancelled`, inspection `delivered` |
| FIND-009-10 | Rustdoc with `# Errors`/`# Panics` on every changed item | AGENTS.md Code | diff audit, lints |
