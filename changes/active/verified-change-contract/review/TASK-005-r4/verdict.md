# TASK-005 task review, round 4

## Immutable subject

- Repository and branch: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, `vcc/task-005`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Reviewed candidate: `6d93b75825926ac27c9fd7de90879b19d67e39b3`; r3 remediation range `93a2e3c9..6d93b758`.
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 36, original `tasks/TASK-005-production-drift-verifier.md`, r1–r3 verdicts and validated findings, and the r2/r3 remediation tasks. `AGENTS.md`, `architecture/agent-rules.md`, applicable Wyrd/Bifrost authorities and testing references govern the source. `.codegraph/` is absent.
- HEAD matched the candidate throughout both waves. The original-to-candidate diff and `git diff --check f8811ac5..6d93b758` were available and clean.

## Acceptance matrix

The full original-task matrix, with source and test references, is in `task-review.md`. The validated repository-rule findings below supplement it.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| PSI/SPC/Custom contracts, exact Parquet baseline, readiness/status, bounded scoring, canonical results and three SDK journeys | Cumulative Vala, spec, server, SQL and SDK source; recorded Vala, SQL, Wyrd and Bifrost journeys | PASS |
| Revision 36 scoped SYSTEM observation read through query service, Gate/Oracle authorization and audit | Token issuance/verification, fixed SQL, scheduled query consumer; peer Oracle and denial tests | PASS |
| Trigger scheduling, two-Service isolation, manual binding dispatch, direct-run no dispatch, durable results | Shared verification runtime and Rust Drift journey; recorded server and SDK lanes | PASS |
| REQ-146 shared permits, execution timeout, shutdown grace, cancellation and fenced settlement | Runtime limits and permits; r3 fitter now rolls back an uncommitted claim on stop, releases a late committed claim unfitted, drains an admitted fit, then cancels/awaits/releases. The focused test proves fit completion within grace, expiry, and an already-stopped pass. | FAIL for missing controlled claim/commit race proof: `FIND-TASK-005-11` |
| Repository import placement in materially changed Rust | Ordinary function-scoped imports remain in changed PSI and Postgres-test functions | FAIL: `FIND-TASK-005-12` |
| Rustdoc and `# Errors` for materially changed fallible public Rust | Changed `fit_psi_baseline` wrapper has neither | FAIL: `FIND-TASK-005-13` |
| PostgreSQL coordination time, tenant/RLS and audit isolation, Forge namespace, task non-goals | Cumulative source and recorded SQL, boundary, codegen and journey checks; no private Drift scheduler, client scorer or new public surface | PASS |

## Review waves

| Report | Result | Material proposal |
|---|---|---|
| `task-review.md` | FAIL | `TASKREV4-001` |
| `standards-review.md` | FAIL | `REPO-R4-001`, `REPO-R4-002` |
| `domain-review-security-tenancy.md` | PASS | None |
| `domain-review-data-durability.md` | FAIL | `DATA-R4-001` |
| `domain-review-statistics.md` | PASS | None |
| `findings-validation.md` | Complete | Independently revised and deduplicated the task/data proposals under prior `FIND-TASK-005-11`; confirmed two standards findings as `-12` and `-13` |

## Validated finding ledger

- **`FIND-TASK-005-11` — REVISED, MISSING proof.** At `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1875-1938`, the new fitter test never crosses shutdown while a claim transaction or commit is in progress. Its final case starts after the stop token is already cancelled. REQ-146 and the r3 remediation require that race; a claim-boundary regression could escape the recorded test. The implementation appears correct by source inspection, so no production defect is asserted. Extend the existing Postgres fitter coverage to exercise an uncommitted claim and a late commit across stop, checking no fit starts and the durable row is retryable with no charged attempt.
- **`FIND-TASK-005-12` — CONFIRMED, VIOLATION.** `crates/vala/vala-drift/src/psi/mod.rs:171` and `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1762-1766,1784-1785` put ordinary imports inside changed functions, violating `architecture/agent-rules.md`'s module-top import rule. Move them into existing module import blocks and remove duplicates; behavior stays the same.
- **`FIND-TASK-005-13` — CONFIRMED, VIOLATION.** Materially changed public `fit_psi_baseline` at `crates/vala/vala-drift/src/psi/mod.rs:67-73` lacks rustdoc and `# Errors`, required by `AGENTS.md` §16 and `architecture/agent-rules.md`. Document its uncancelled-wrapper role and real fitting errors without changing logic.

Each finding's evidence, caller tracing, correction boundary and focused closure proof are in `findings-validation.md` and the remediation task.

## Prior closure and verification limits

Prior `FIND-TASK-005-1` through `-10` remain closed. The r3 production correction resolves the observed immediate-cancellation path of `FIND-TASK-005-11` and preserves the prior cooperative stop; the claim-race proof in its explicit acceptance criterion remains incomplete. The user recorded exit-zero focused Postgres test, `fmt`, `lints`, server integration (83), Wyrd (2135), Drift journey, and cumulative diff check after the last code change. Reviewers did not rerun these lanes. Green tests do not waive explicit import/rustdoc rules or prove an unexercised race. The one noninterruptible quantile sort remains within the decoded-data budget and has no separately demonstrated violation.

## Verdict

**FIX_REQUIRED** — three bounded findings remain. Implement `TASK-005-R4-claim-race-and-standards.md`, then review the full cumulative candidate against the same base.
