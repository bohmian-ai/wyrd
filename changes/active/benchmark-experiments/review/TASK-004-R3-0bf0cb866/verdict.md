# TASK-004 second remediation review — FIX_REQUIRED

> **Current disposition: FIX_REQUIRED for FIND-TASK-004-8 only.**
> Maintainer-approved revision 10 supersedes F7 as written:
> The maintainer withdrew FIND-TASK-004-7 as written. Smoke records production
> errors, dropped/refused calls, shedding, wrong answers and missed targets
> without failing for those outcomes. Broken measurement/evidence plumbing is
> still INVALID. Prove normal full-mode judging separately with controlled
> results. FIND-TASK-004-8 remains open and unchanged. The revised remediation
> task in this directory governs implementation; the original review below is
> retained as history under its recorded revision-9 authority, not an instruction
> to restore production criterion judgments in smoke.

The revised spec, R3 task, dependent TASK-005 sections and supersession notices
received a fresh read-only comprehension pass. The reader distinguished
production observations from plumbing invalidity, separate controlled judging
proof and unchanged F8 calibration; no material ambiguity remained. This was a
documentation revision, not another implementation review or runtime check.

## Immutable subject and authority

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
Prior candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
Candidate: `0bf0cb8662a221233a963ed147d57ef0cbb6720f`.
Review diff: `git diff c9d72729b 0bf0cb866`; cumulative original range remains
available. No rebase or candidate movement occurred during review.

Authority: approved revision-8 behavior and the maintainer-directed revision-9
real-smoke correction in `changes/active/benchmark-experiments/spec.md`.
Working-tree authority SHA-256:
`007d27e3a0e22a4aa912108da6a7478849a3f2f56c953fdea9e1491c65e96dce`.
The stale draft header does not reopen approval. Original TASK-004, prior
verdicts/ledgers and R1/R2 remediations remain acceptance context. Uncommitted
spec and TASK-005 revisions were preserved; their identities are in `scope.md`.

PR215 merged at `9a540b7b8`, parents `7746a4073` and `1c52b78bb`. Review
distinguishes first-parent remediation from imported schema work and checks
benchmark identity/projection, fixture and production timing/auth merge seams.
The imported change's existing review remains its acceptance evidence; this
round does not restart an unrelated full schema audit.

## Scope, reviewers and independent results

This round reviews original residual closure, changed code and consumers, and
the separate maintainer-directed smoke addition. Prior source-backed acceptance
carries for unchanged HDR arithmetic/bounds, component/overflow/reset invalidity,
real timer/wait union meanings, listener/privacy/recorder, warmup lifecycle,
retained artifacts, legacy readers, publication and checked numeric conversion.
The auth/classification changes reopen only F4's affected boundary. TASK-005
qualification, complete matrix and broad instrumentation audit are excluded.

| Fresh role / report | Result and coverage |
|---|---|
| Remediation ponytail discovery / `remediation-review.md` | FAIL; F1/F2/F3/F5 closure, measurement consumers, driver admissions, real-smoke lifecycle and evidence; R3-B01/R3-B02 proposed. |
| Maintainer / `maintainer-review.md` | FAIL; complete orchestration/fixture/report owner reuse, changed consumers/rules; MNT-R3-01 proposed. No replacement framework drift retained. |
| Security domain / `domain-review-security.md` | PASS; finite arrival labels, disabled/readout limits and authentication/tenant/audit preservation through merge. Empty material ledger. |
| Independent ponytail validation / `findings-validation.md` | FIX_REQUIRED; independently source-validates proposals, deduplicates the shared judge defect, revises calibration burden, confirms prior closure and affected acceptance. |

All selected reports are complete. `claim-comparison.md` explains grouping and
why no extra follow-up discovery was needed. No validator-new blocker or
unresolved authority/source conflict required a separate validation pass.
Agreement was not treated as proof. The independent validated ledger is the
only source of retained findings.

## Reconciled acceptance and prior closure

| Obligation | Evidence / disposition | Result |
|---|---|---|
| F1 measured byte counters and actual admissions | Window returns stop scrape separately; all four byte consumers use it; three schedulers recheck actual stop; exact regressions pass. Practical HTTP scrape skew is not an atomic-snapshot requirement. | CLOSED |
| F2 real OTLP metadata | Shared authorized constructor copies scoped headers for spans/logs and preserves bearer/errors; exact producer test passes. | CLOSED |
| F3 selected maximum | Bifrost L2 q=1 uses same selected cohort/rank/bound conversion; exact cohort regression passes. | CLOSED |
| F4 classification/auth | Current arrival-time metadata contract replaces old authenticated-only activation. Production credentials, tenant authority, audit and refusal stay at existing owners; finite unknown labels invalidate enabled benchmark evidence. Old activation proof is not claimed. | SUPERSEDED; no current material finding |
| F5 mandatory rustdoc | Pending field was documented then its superseded mechanism deleted; original omissions remain corrected. | CLOSED |
| F6 checked conversion | Installed checked rank conversion survives; unchanged proof carries. | CLOSED |
| Real smoke lifecycle and owner reuse | Same declarations/loaders/publication/steps/measure/drain/evidence/records/report; handwritten smoke removed. | PASS structurally |
| Positive CPU/work, nonempty exact populations and ordinary invalidity | Current source/owner tests and retained evidence. | PASS for traced validity; does not waive F7 |
| Smoke applicability and correctness | All remaining criteria waived by shared judge; real saved reports waive request errors and capture shedding. | FAIL — F7 |
| Mode, local output, baseline exclusion after relocation | Record publish/read/defaults and calibration full-mode guard; existing relocation assertion checked independently. | PASS |
| Isolated full prepass and clock | Existing child/Postgres launcher, no recursion; any nonzero prepass refuses preparation; separate recorded budget/time. | PASS mechanism; F7 gives false success |
| Independent same-path calibration and observation cost | Existing test directly feeds recorder/Population and reads self CPU; no actual Window/runner or observation-cost control. | FAIL — F8 |
| HDR/local readout/artifact/legacy/platform/dependency boundaries and inspected merge seams | Carried source-backed acceptance plus selected current coverage and focused checks. | PASS within recorded scope |

The complete source/proof matrix is in `findings-validation.md`. R2's
four-experiment smoke wording conflicts with the task partition: TASK-005 owns
writes-hot/writes-parquet and the complete matrix. The remediation clarifies
current default-plan proof versus final four-plan proof, without dropping final
coverage or requiring TASK-005 early.

## Retained validated findings

| Stable ID / decision | Violation and consequence | Minimal correction |
|---|---|---|
| FIND-TASK-004-7 — CONFIRMED; R3-B01 + MNT-R3-01; INCORRECT/VIOLATION | `experiment.rs:2534–2539` exempts every remaining smoke criterion. Wrong answers, errors, shed captures and sufficiently sampled misses can PASS and admit full preparation. Historical clean Gateway/Verification records show this occurred. | Delete the shared blanket exemption; retain only existing below-floor/reference exemptions and normal validity/limit/verdict rules. Correct conflicting changed docs/task/test in the same patch. |
| FIND-TASK-004-8 — REVISED; R3-B02; MISSING | `capacity.rs:1085–1193`'s calibration bypasses actual Window/resources/shared runner and treats service residual as instrumentation cost. Explicit same-path independent calibration proof remains incomplete. | One bounded controlled actual measurement-to-record case with independent time/CPU references; equivalent enabled/disabled collection observation-cost comparison using existing startup and measurement owners. No new production control or framework. |

`findings-validation.md` preserves exact source/consumer traces, authority,
observed consequences, materiality, correction burden, selected approach and
focused closure proof for each finding. These are implementation/proof failures
in the directed addition, not retroactive IDs for the maintainer direction.

Self-contained remediation:
`TASK-004-R3-restore-smoke-judgment-and-calibration.md`, routed to
`$wyrd-implement`. It reuses existing owners, avoids per-engine duplicated guards
and preserves measurement and production boundaries. No spec revision or new
product/security decision is required.

A fresh read-only packet reader reviewed the spec, remediation and TASK-005
dependency partition without source or review reports. It correctly explained
both corrections, enabled-path versus disabled-control calibration, proof and
limits, and found no material ambiguity or contradiction. No new maintainer
decision is requested; this comprehension pass is not implementation proof.

## Verification and limits

`verification.md` records independent sequential current-candidate checks:
both migration/idempotence tests, 46 affected library/capacity tests, eight
exact selectors, scoped fmt/Clippy, dependency and whitespace checks all pass.
The green smoke test encodes the incorrect exemption and cannot establish
acceptance. No source was changed to manufacture RED during immutable review.

Historical eight-plan release smoke records and timing artifacts at
`2f43ca9bd` were located and inspected. Release report readback reproduces a
Gateway PASS with shed captures. These premerge artifacts establish actual
workflow execution, not final-candidate smoke proof. No release smoke/full
benchmark rerun, broad gate, language journey suites, cross-platform execution
or unrelated imported-schema audit was performed. Clean final-source smoke
after correction remains existing proof, not a third finding. Real engine
criterion failures must remain visible, not be waived to manufacture PASS.

## Convergence assessment

Two remediation reviews have been unsuccessful, but all four original residuals
F1/F2/F3/F5 are now closed. The current defect is a fix-induced shared smoke
judgment weakening, plus incomplete explicitly requested calibration proof.
These are not more manifestations of the old producer/counter findings.
An unbounded new matrix, framework or production-control requirement would
expand expectations and is rejected. Next review is limited to F7/F8 closure,
changed callers/tests/docs and final-source smoke evidence, carrying the
accepted rows above. TASK-005 retains its full qualification and profile audit.

## Verdict

**FIX_REQUIRED — FIND-TASK-004-7, FIND-TASK-004-8.**

Review wrote reports and the remediation task only. No implementation, commit,
merge, push or deployment was performed. Remaining risk: smoke currently
conceals real failed checks, and calibration does not yet establish the full
measurement path or its observation cost.
