# TASK-001 R3 task-review verdict

**Verdict: FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-principals`.
- Base: `c46afdcac`; candidate: `437205debc628538ba6aa4ec828601c7c40145b4` (HEAD still matched when this verdict was written).
- Approved authority: `changes/active/verification-closeout/spec.md`, revision 2; original task: `changes/active/verification-closeout/tasks/TASK-001-r3-principal-roles-and-local-flow.md`; repository rules and applicable Wyrd architecture.
- Reviewed range: the complete `git diff c46afdcac 437205deb` (172 files). Review artifacts are outside the immutable candidate.
- User scope correction during review: a migration is not needed; focus actionable issues on routine workflows. Historical database upgrade proposals and non-v7 external cursor rejection are excluded from this task's corrective ledger. This verdict does not certify those paths.

## Reconciled acceptance matrix

| Obligation | Candidate and proof | Result |
|---|---|---|
| REQ-001 principal discovery, source-specific assignment, gates, audit, and tenant isolation | Principal routes, SQL, shared client, and recorded principal integration and SDK journeys cover fresh-schema administration; direct/IdP coexistence and idempotency are exercised. | PASS within the narrowed scope |
| REQ-002 four Roles, initial `workload`, issuer default, no aliases | `BUILTIN_ROLES`, Card projection, and recorded role and SDK tests cover newly provisioned tenants. | PASS within the narrowed scope |
| REQ-003 observation attribution and Verifier invocation | Gate and OTLP enforce the observation-target rule. Real-time verification invokes a selected Verifier under `evals:run`; applying Gate's subject-kind rule there would reject valid analysis. | PASS for the reviewed paths |
| REQ-003 typed principal results in Rust, Python, TypeScript; per-request stock-client auth | Adapters and signed-in expiry journeys are reported passing; Python and TypeScript discovery declarations misstate the CardRef result. See FIND-TASK-001-2. | FAIL |
| AC-002 admin-key and saved-login local journeys without Card key or publication flush | Rust, Python, and TypeScript journeys and guide show the requested routine path; recorded identity, Bifrost, OTLP, and gateway lanes passed. | PASS for exercised paths |
| Non-goals: no role aliases, direct principal permissions, MCP surface, compatibility route, second refresh owner, or Rust gateway adapter | Diff removes the Card-addressed route and reuses the shared token path; no prohibited surface was validated. | PASS |
| Repository completion rules | Recorded format, lint, codegen, dependency, tenant, package, typing, docs, and scoped journey lanes passed, but changed Rust declarations lack mandatory rustdoc. See FIND-TASK-001-3. | FAIL |

## Independent reviews and validation

| Report | Result | Material claim |
|---|---|---|
| `task-review-behavior.md` | FAIL | Migration, verification subject kind, cursor version |
| `task-review-invariants.md` | FAIL | Same three shared causes |
| `standards-review.md` | FAIL | Missing rustdoc hard gate; structural notes |
| `maintainer-review.md` | FAIL | Principal CardRef declaration mismatch |
| `system-review.md` | FAIL | Historical migration and readiness path |
| `domain-review-security.md` | FAIL | Historical migration and retired-role data |
| `domain-review-data.md` | FAIL | Historical migration and role data |
| `findings-validation.md` | Two retained | Independently validated each discovery claim and correction against source; rejected the verification subject-kind proposal after the user clarified Verifier invocation, excluded historical migration claims and lower-frequency cursor input. |

No focused follow-up reviewer was needed: the discovery claims did not materially conflict. After the user clarified Verifier invocation, the independent validator re-examined and rejected both task reviewers' subject-kind proposal. The system, security, and data reports traced the same historical migration cause. The unique maintainer and standards claims were independently validated. There is no prior task-review verdict or prior finding ledger for this initial round.

## Validated finding ledger

| ID | Classification | Consequence | Correction boundary |
|---|---|---|---|
| FIND-TASK-001-2 | INCORRECT public typing | Python and TypeScript principal discovery declarations misdescribe the structured CardRef value. | Reuse TypeScript `CardRef` and describe Python's actual dictionary shape in the stub source; regenerate declarations. |
| FIND-TASK-001-3 | VIOLATION | Missing rustdoc on changed Rust items fails the repository's explicit completion gate. | Document the changed declarations in place and inspect the cumulative changed declaration set. |

The full producer-to-consumer evidence, rejected and excluded proposals, and focused closure proof are in `findings-validation.md`. Structural import and redundant RLS predicate notes did not establish a behavioral consequence and do not produce a separate task round.

## Verification limits

This review assessed the committed source, complete diff, and the task's recorded passing verification; it did not rerun the lanes. The reported journeys cover fresh-schema routine flows, not historical database upgrades, and they do not exercise the CardRef declaration mismatch. The remediation task requires only focused proof and narrow lanes for its write set; full journeys belong to change review.

**Remediation:** `TASK-001-R1-principal-client-contract.md` in this directory.
