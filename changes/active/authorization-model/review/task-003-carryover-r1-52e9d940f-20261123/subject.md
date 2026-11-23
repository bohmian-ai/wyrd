# R1 remediation review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Original base: `4355587f128a0ac1d05071be0a8a84ba53a52290`.
- Prior candidate: `b7206eafefbf7e981898eca3fc22a5d5ae76b815`.
- Current committed candidate: `52e9d940fd57c49a3ab5084fb319171e21eafb37`.
- Review diff: `git diff b7206eafef 52e9d940f`; cumulative original range remains available. Prior candidate is an ancestor; no rebase correspondence is needed.
- Prior review and implemented task: `../task-003-carryover-mid-b7206eafe-20261122/`, including `verdict.md`, `findings-validation.md`, and `TASK-003-carryover-R1-current-work-corrections.md`.
- Scope: FIND-003-carryover-1 through -9, affected consumers and PR 242 integration seams, approved REQ-019/AC-014, and maintainer-added REQ-020/AC-015. The parent task is still mid-implementation: cycle 11 deliberately RED; cycles 12–20 deferred.

## Pinned authority and working tree

Committed specification is revision 8, approved in `75b4e800b2397cdb9283b9159ed28b72468f4705`. Current on-disk specification is approved revision 9, with a 2026-11-23 approval record. The maintainer identifies its commit as held by a hook. These two pre-existing modified files are pinned authority inputs, not implementation dirt or review edits:

| File | Git blob hash of on-disk input |
|---|---|
| `changes/active/authorization-model/spec.md` | `55689bae96c62e78b1ea8d765a32629664b4e603` |
| `architecture/wyrd-security-posture.md` | `0210235fb05322ef6e888526c4c9ba6bec02b234` |

No staging, commits, hook changes or publication is authorized. Reviewed production source remains committed and immutable. Recheck HEAD and both authority hashes before delivery. Reports disclose that revision-9 authority is available on disk but absent from the candidate commit.

PR 242 entered via merge `70a56a136e5ea6bd13c1094f2b4a09b8da813728`; the auth branch port follows in `e59d2dd9d`. Inspect the affected KMS, source, error, cache and SQL seams; this review does not restart acceptance of unrelated Bifrost, benchmark or stored-secret work.

## Assignments

| Fresh reviewer | Scope | Report |
|---|---|---|
| r1_validation | Independent closure of all nine prior findings, changed paths and affected consumers, evidence/TDD reconciliation; final validation/verdict | `task-review-remediation.md`, `findings-validation.md`, `verdict.md` |
| r1_kms | Material security/lifecycle boundary: WyrdKms secret port, derivation-key creation/rotation/cache/refusal, shared cap and SQL ownership | `task-review-kms.md` |
| r1_signin | Material new REQ-020 boundary: all sign-in starts, shared admission, BFF source trust and HTTP/OAuth errors | `task-review-sign-in.md` |

The orchestrator does not discover or validate findings. New validator-origin findings require another independent focused source check. Failure/recovery and persistent state are covered by KMS and primary; a whole-PR review, extra maintainability vote or repeated original discovery would add no required coverage.

## Entry evidence and limits

R1 task records focused journeys, SQL, UI, lints/codegen/docs and boundary checks, plus the new sign-in-start evidence. Reviewers inspect assertions and source binding before relying on them. Root has not rerun tests at entry; Cargo work must be sequential across reviewers. Historical and R1 TDD violations remain process facts, not requests to manufacture RED. No benchmarks were requested; current repository rules require explicit maintainer authorization for them. Source-based hot-path wait audits remain applicable.

A cumulative `git diff --check` reports trailing blank lines in unrelated merged documentation/examples; this is not recorded as a clean candidate-wide check. Scope any material gate conclusion to the actual remediation and its required checks.
