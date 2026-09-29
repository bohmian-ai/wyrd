# TASK-011 review verdict

**Verdict: SPEC_REVISION_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, branch `vcc/task-005`.
- Base: `338f33235f81c30dfe3a570dc26934fe7bb77048` (approved specification revision 37).
- Candidate: `6e3bac0370a31b19d767ac4d20830d430c3f2ff5`.
- Authority: `changes/active/verified-change-contract/spec.md` revision 37 and `tasks/TASK-011-conventional-psi-spc.md`.
- Scope: the complete 34-file base-to-candidate diff. The candidate remained unchanged through both review waves. This is the first TASK-011 review, so there are no prior findings to close.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-153, exhaustive PSI bins and `other` evidence | Vala PSI fit/score and server aggregate changes; PSI fixtures, SQL tests, SDK journeys | PASS for inspected static behavior |
| REQ-154, authored fixed SPC subgroups and complete targets | Contract, fitter, ordered server query, SPC tests; mixed signal and partial feature can still produce a failed run (FIND-TASK-011-2) | FAIL |
| REQ-155, NIST X-bar/S formulas and typed evidence | Vala control limits/report code; independent constant and signal fixtures; three SDK evidence assertions | PASS |
| REQ-156, baseline and target completeness; direct/server agreement | Baseline rejection and ordinary incomplete fixtures pass; direct selected null-only rows are indistinguishable from unrelated rows (FIND-TASK-011-1), and server completeness/aggregation use separate query cuts (FIND-TASK-011-3) | FAIL |
| REQ-157, immutable fitted-version boundary and historical reads | Format marker/refusal and Rust stored-result journey | PASS |
| INV-012, preserve query authorization, audit, tenant, runtime, Custom and result paths | Source inspection and passing Vala/server/journey lanes; no contrary finding | PASS |
| AC-034, statistical fixtures and Rust/Python/TypeScript journeys | Formula/bin fixtures and reported journey lanes pass; Python/TypeScript lack scheduled dispatch and stored-legacy refusal (FIND-TASK-011-7); required OpenAPI gate and exact focused-command record are missing (FIND-TASK-011-4/5) | FAIL |
| Non-goals: no extra charts, rules, imputation, inferred groups, or automatic migration | Diff removes WECO rules and refuses old fitted format; no excluded mechanism added | PASS |
| Task lifecycle and evidence | Submitted implementation still has `status: ready` (FIND-TASK-011-6) | FAIL |

## Independent review results

| Wave | Report | Result |
|---|---|---|
| Task implementation | [task-review.md](task-review.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Statistics domain | [domain-review-statistics.md](domain-review-statistics.md) | FAIL |
| Persistence and server reads domain | [domain-review-persistence.md](domain-review-persistence.md) | FAIL |
| Ponytail finding validation | [findings-validation.md](findings-validation.md) | SPEC_REVISION_REQUIRED; seven retained findings |

## Validated findings and decision

| Finding | Status | Closure boundary |
|---|---|---|
| FIND-TASK-011-1 | REVISED; spec decision required | REQ-156 must define how direct input distinguishes a selected null-only observation from a genuinely unrelated all-null row. The current wide nullable RecordBatch cannot express that distinction while preserving both required outcomes. |
| FIND-TASK-011-2 | CONFIRMED | Make any partial or empty SPC feature force one wholly unscored run. |
| FIND-TASK-011-3 | CONFIRMED | Derive PSI/SPC completeness and aggregates from one authorized, audited query cut. |
| FIND-TASK-011-4 | CONFIRMED | Run and record the served OpenAPI integration gate. |
| FIND-TASK-011-5 | CONFIRMED | Record and, where necessary, run each exact named Vala test command. |
| FIND-TASK-011-6 | CONFIRMED | Set the implemented task's state to `review`. |
| FIND-TASK-011-7 | REVISED | Prove scheduled dispatch and stored-legacy refusal in Python and TypeScript SDK journeys. |

FIND-TASK-011-1 requires a human-approved contract decision: a direct-input membership signal or an explicit rule for classifying all-null rows. A nullness-only code change would silently violate either selected-null refusal or unrelated-row exclusion. The bounded findings remain open; no remediation task is issued under the current approved revision. After the spec decision, package the validated findings for implementation and review the cumulative candidate again.

## Verification limits

The candidate records green format/lint, Vala, `wyrd-spec`, server integration, Rust/Python/TypeScript Drift journeys, codegen, docs, and diff checks. This was a static review; reviewers did not rerun those lanes. Their coverage does not include the seven cases above. JSON observation APIs cannot send NaN or infinity, so the existing Vala/server SQL fixtures are the available evidence for those values. Python and TypeScript currently prove manual binding dispatch and authored-field rejection, not the scheduled and stored-legacy outcomes AC-034 requires.
