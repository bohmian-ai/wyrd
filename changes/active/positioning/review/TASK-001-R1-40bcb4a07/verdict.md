# TASK-001 verdict: FIX_REQUIRED

## Immutable subject and authority

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.

- Base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
- Candidate: `40bcb4a072dcbb33bee734f9b1c3ff0f63cf0718` on `docs`.
- First review: full `git diff 443a79337...40bcb4a07` (130 paths), not the implementation summary.
- Approved specification: `changes/active/positioning/spec.md`, SPEC-positioning revision 4 at candidate.
- Original task: `changes/active/positioning/tasks/TASK-001-positioning-and-finance-onboarding.md` at candidate, including its approved landing design.
- Uncommitted design/task edits present at entry are excluded from the immutable subject and preserved. Source and HEAD remained unchanged during review.

Revision 4 removes the live-provider walkthrough and release-availability gates; neither is a finding. It retains the PDF workflow and no-public-API constraint. The task's complete status and historical completion claim do not substitute for independent acceptance or amend those constraints.

## Round scope, coverage and independent results

No prior independent review, finding closure, remediation or carried-forward acceptance evidence exists. This is the first round; no convergence assessment applies. `review-scope.md` records the source navigation and specialist selection before discovery; `claim-comparison.md` reconciles related proposals.

| Role | Report | Result and coverage |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; full task acceptance, reader/application flows, negative paths and presentation |
| repo-rev | [standards-review.md](standards-review.md) | FAIL; all changed tiers, nested rules, mandatory rustdoc, source/claim consistency |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL; schema/projection/result APIs, existing owner alternatives, presentation/source owners |
| domain-rev evidence | [domain-review-evidence.md](domain-review-evidence.md) | FAIL; policy extraction, exact selection, workspace trust, citations and public attribution |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; independently source-validated six findings, corrections, matrix and specialist applicability |

The independent validator accepted every selected role's coverage. Separate invariant/system/security/tenancy/concurrency/storage-engine specialists were not applicable: local value projection/component motion state changed, but no shared server lifecycle, recovery, permission evaluator, SQL/RLS, persistence engine or coordination owner changed. Evidence review traced the unchanged workspace authorization and storage dependencies. Detailed selection/omission evidence is in the scope and validation reports.

No focused follow-up was needed: duplicate proposals shared causes, unique proposals were source validated, no material conflict remained, and validation introduced no new blocker. Agreement alone was not treated as proof.

## Reconciled acceptance

The complete reconciled matrix and precise source/test evidence are in `findings-validation.md`; discovery matrices preserve broader passed sub-obligations.

| Obligation | Result and evidence |
|---|---|
| REQ-001/007, AC-006: exact opening, hierarchy, CTAs, languages, brand/navigation | PASS within task; source inspection and focused render/parity checks |
| REQ-002, AC-001: finance programs and synthetic evidence | FAIL; findings 1 and 5; language parity and existing route otherwise present |
| REQ-003, AC-003: one Service/Verifier/three checks, gateway policy judgment | FAIL; topology present, required PDF extraction absent (finding 2) |
| REQ-004, AC-001/002: application decisions, requirement reasons and native errors | FAIL; finding 1 |
| REQ-005, AC-004: reuse, exact historical attribution and inspection | FAIL; declaration pinning exists, required extraction/update/execution proof missing (2/4) |
| REQ-006, AC-005: complete onboarding and coupled proof | FAIL; prerequisites/pins/exposure accepted under rev4, findings 1/2/4/5 remain |
| REQ-008, AC-007: bounded inventory, editorial/handoff/source owners | PASS within task, subject to coupled correction of retained findings |
| INV-001: no new public APIs | FAIL; finding 3 |
| INV-002: precise synthetic supplied evidence | FAIL for purpose omission (5); provider exposure and no real transmission accepted |
| INV-003: native outcomes, missing evidence and durability | PASS for runtime boundary; missing consumer demonstration/inspection covered by 1/4 |
| INV-004–006 and approved animations | PASS within task on source/recorded evidence, except PDF mismatch consolidated into 2 |
| Mandatory changed-item rustdoc | FAIL; finding 6 |

Later-task educational corpus, FAQ/tutorials and shared discovery closure were not incorrectly required for TASK-001 or declared completed by it.

## Independently validated ledger

The authoritative detailed ledger is `findings-validation.md`, including exact locations, reachability, violated obligations, consequences, materiality, correction burden and closure proof.

| Stable ID | Validation / discovery IDs | Retained finding |
|---|---|---|
| FIND-TASK-001-1 | REVISED / BEH-001 | Runnable programs omit explicit application decisions, requirement reasons and native-error hold demonstration |
| FIND-TASK-001-2 | CONFIRMED / BEH-002, EVID-001, ST-002 | Raw Markdown replaces required PDF extraction while published surfaces still claim PDF |
| FIND-TASK-001-3 | CONFIRMED / BEH-003, MNT-001 | New public SDK authoring/input/result contracts exceed approved scope |
| FIND-TASK-001-4 | REVISED / BEH-004, EVID-002 | Policy update lacks required changed-policy and public execution-attribution demonstration |
| FIND-TASK-001-5 | CONFIRMED / BEH-005 | Explicitly required release-purpose evidence is absent |
| FIND-TASK-001-6 | CONFIRMED / ST-001 | Retained new Rust fields/test items omit mandatory rustdoc |

No new finding is assigned for whitespace, optional hardening or missing broad reruns. Existing named-slot omission semantics already existed; the review does not claim a server regression there. The SDK correction removes task-unearned APIs while preserving base Python model authoring, TS TableConfig schema support and native semantics.

## Verification and hot-path limits

Fresh checks passed: docs README/indexes/shell-links (18 tests), narrower overlapping render tests, Python lint/stub checks, TypeScript lint and docs command checks. See `verification.md` and the role reports for exact commands. Historical task evidence names earlier candidates and is not fresh final-candidate proof. Full Postgres/SDK suites, docs build/link/accessibility/browser/codegen and benchmarks were not rerun. These source-proven findings do not depend on an invented runtime failure.

No added/moved PostgreSQL, Scribe, object-storage or Vault waits were found. `Target::project` is local client preparation; schemas are declaration-time and predicates result-local. Named server verification/gateway/Oracle/Scribe hot-path owners are unchanged. The independently validated empty wait inventory is recorded in standards/validation reports. This grants no benchmark exception for future hot-path changes.

## Verdict and routing

**FIX_REQUIRED**. All six findings can be corrected using existing owners under revision 4; no new product/architecture/persistence decision is necessary. Retaining the Markdown substitution or public SDK additions would require separate specification approval, but neither is required to satisfy this task.

One cohesive remediation: [TASK-001-R1-restore-approved-finance-workflow.md](TASK-001-R1-restore-approved-finance-workflow.md), routed directly to `$wyrd-implement`. This review implements nothing and does not merge, push, publish or deploy.
