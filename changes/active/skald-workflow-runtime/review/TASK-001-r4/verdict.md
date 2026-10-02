# TASK-001 round-four review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Candidate tree: `bf9ee6b314482a8fe028c31f4121eab6b8ac9e92`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11, including `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Reviewed remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate remained at the stated commit through discovery, focused
follow-up, and structured Ponytail validation. The review covered the complete
base-to-candidate range and explicitly reviewed the `516d0fbcc` test-harness
fix.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Explicit Workflow graph, binding, routing, execution, and portable results | Cumulative Rust, Python, schema, consumer, and focused execution evidence remains aligned. | PASS |
| Fixed Agent deadline, retry, cancellation, panic settlement, and owned Workflow work | The deadline is raced with the required precedence; attempt panics settle as failed while genuine interruption remains cancelled. | PASS — `FIND-TASK-001-20` and `FIND-TASK-001-21` closed |
| ExtGateway routing, SSRF controls, and secret containment | The shared endpoint owner preserves DNS screening and pinning; decoded successful responses are checked before result projection. | PASS — `FIND-TASK-001-22` closed |
| Revision 11 Observer deletion and payload-free tracing | Observer surfaces remain deleted; ordinary provider/model attributes and failure outcomes are corrected, but a supported callback-replaced OpenAI request records the original Prompt model. | FAIL — `FIND-TASK-001-29` |
| Repository Rust shape, documentation, and dependency rules | Required rustdoc, Agent owner shape, and removal of direct library `anyhow` are complete. The remediated architecture page contradicts the required `skald-workflow -> wyrd-spec` edge. | FAIL — `FIND-TASK-001-27`; `FIND-TASK-001-24` through `FIND-TASK-001-26` closed |
| Test-harness root-cause fix required for `test:wyrd` | The state token is cancelled and the fixture is declared last, but cancellation is not quiescence: in-process role owners are not shut down/aborted and a timed-out bound serve handle is detached before the fixture force-drops Postgres. | FAIL — `FIND-TASK-001-28` |
| Prior findings | `FIND-TASK-001-1` through `FIND-TASK-001-26` are closed. The prior documentation ID `FIND-TASK-001-27` remains open with a revised diagnosis. | FAIL — one prior ID retained |
| Prohibited scope | No Observer replacement, compatibility alias, second Workflow runtime, remote Workflow surface, credential administration, or new production dependency entered the candidate. | PASS |

## Independent review results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | FAIL | `BEH-R4-001` |
| `task-review-invariants.md` | PASS | None |
| `standards-review.md` | FAIL | `STD-R4-001` |
| `maintainer-review.md` | FAIL | `MNT-R4-001` |
| `system-review.md` | FAIL | `SYS-R4-001` |
| `domain-review-concurrency.md` | FAIL | `CONC-R4-001` |
| `domain-review-network-security.md` | PASS | None |
| `domain-review-telemetry-privacy.md` | FAIL | `TEL-R4-001` |
| `followup-review.md` | RESOLVED | `FUP-R4-001` through `FUP-R4-003` |
| `findings-validation.md` | FIX_REQUIRED | Three validated findings |

All required reviewers and reports were available. No reviewer filled more
than one role.

## Follow-up decision

A fresh follow-up was required because discovery conflicted on whether the
test-server patch establishes background-role quiescence, and because the
telemetry reviewer exposed a supported callback path not covered by the other
reports. It also reconciled overlapping documentation claims.

The follow-up established that token cancellation and field order do not join
or abort retained database-using work; that `before_model` can replace the
request reaching provider dispatch while the span retains the Prompt model;
and that the changed architecture page contradicts the live foundational
contract edge. Structured validation retained those three bounded gaps and
rejected expansion into pre-existing PyO3 documentation debt.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-27` | REVISED | INCORRECT | Align only the Skald architecture overview, dependency prose, and diagram with the existing narrow `skald-workflow -> wyrd-spec` contract edge. |
| `FIND-TASK-001-28` | REVISED | INCORRECT | Make bound and in-process `WyrdTestServer` teardown abort or complete owned database-using work before releasing the fixture; do not detach a timed-out serve task. |
| `FIND-TASK-001-29` | CONFIRMED | INCORRECT | Record the effective post-callback request model in the GenAI call span, with the resolved Prompt model only as the existing fallback for request shapes without a model. |

The source traces, rejected proposals, correction boundaries, and focused proof
requirements are in `findings-validation.md`.

## Verification limits

- Focused Workflow deadline, panic, ExtGateway, endpoint-policy, and ordinary
  telemetry tests passed across the independent reviews.
- The candidate records green format, lint, Skald, shared, Python, codegen,
  boundary, docs, examples, and post-harness-fix Wyrd lanes; cumulative
  `git diff --check` is green.
- Those results do not exercise callback model replacement or deterministically
  assert role/serve-task quiescence before fixture drop. The docs lane does not
  detect the contradictory dependency prose.
- Review did not modify the immutable candidate.

## Verdict

**FIX_REQUIRED**

Three bounded corrections remain within approved Revision 11. The remediation
task is `TASK-001-R3-close-round-four-review-gaps.md` in this directory.
