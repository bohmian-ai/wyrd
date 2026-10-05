# TASK-001 round-two review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior review: `changes/active/skald-workflow-runtime/review/TASK-001-r1/`

The candidate remained at the stated commit through discovery, focused
follow-up, and structured Ponytail validation. Production source is unchanged
from round-one candidate `eb22b03f2bb766886d839bda23aafbd4ba130ab3`;
revision 10 changes the approved packet authority to adopt the two public shapes
already implemented by that source.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Explicit Workflow graph, binding, route, and run contracts | Revision 10 now fixes `HashMap<HeaderName, SecretString>` and `RemoteProblem(Box<RemoteProblem>)`; the candidate exposes those exact shapes. | PASS — prior FIND-TASK-001-3/4 closed |
| Native Agent-only execution and provider dialect preservation | The shared Agent loop is used, but OpenAI Responses continuation still drops reasoning items. | FAIL — FIND-TASK-001-2 |
| Exact namespaced outputs and bounded snapshots/observations | Namespaced projection is present, but escaped text can exceed the run ceiling and oversized provider output reaches observation before enforcement. | FAIL — FIND-TASK-001-1, FIND-TASK-001-5 |
| Bounded retries, deadlines, cancellation, and owned lifetime | JoinSet ownership and ordinary paths exist, but callbacks can defeat liveness, a pre-poll race creates zero-attempt active steps, extreme deadlines panic, and the maximum retry count overflows. | FAIL — FIND-TASK-001-6, FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16 |
| ExtGateway exact binding, SSRF defense, and secret containment | DNS screening/pinning, TLS, no-proxy, and no-redirect behavior are present, but secret bindings admit routing/transport headers and refusal errors retain reflected credentials. | FAIL — FIND-TASK-001-17, FIND-TASK-001-18 |
| Rust and Python authoring/result parity | Rust authoring moved with the contract, but new PyO3 behavior is outside the SDK owner and public declarations erase exact result DTOs. | FAIL — FIND-TASK-001-10, FIND-TASK-001-11 |
| Repository documentation and maintainability rules | Generated surfaces largely moved together, but test imports, observer documentation, and the step-ID guide remain noncompliant or inaccurate. | FAIL — FIND-TASK-001-7, FIND-TASK-001-12, FIND-TASK-001-13 |
| Required verification evidence | Recorded scoped lanes remain useful, but docs/examples lanes and four exact selectors are absent, and the cumulative candidate fails its required diff check. | FAIL — FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-19 |
| Prohibited scope and unrelated changes | Validation retained no finding for a new crate/dependency, remote language or MCP Workflow surface, compatibility translation, or unrelated redesign. | PASS |

## Independent review results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | FAIL | BEH-R2-001 through BEH-R2-002 |
| `task-review-invariants.md` | FAIL | INVAR-R2-001 through INVAR-R2-012 |
| `standards-review.md` | FAIL | STD-001 through STD-010 |
| `maintainer-review.md` | FAIL | MNT-R2-001 through MNT-R2-005 |
| `system-review.md` | FAIL | SYS-001 through SYS-004 |
| `domain-review-concurrency.md` | FAIL | CONC-001 through CONC-004 |
| `domain-review-network-security.md` | FAIL | NET-R2-001 through NET-R2-002 |
| `followup-review.md` | RESOLVED | FOLLOWUP-R2-001 |
| `findings-validation.md` | FIX_REQUIRED | 17 active deduplicated findings; prior 3/4 closed |

All required reviewers and reports were available. No reviewer filled more than
one role.

## Follow-up decision

A focused follow-up was required because the standards reviewer reproduced a
base-to-candidate `git diff --check` failure while the invariant reviewer
recorded a pass from a no-argument working-tree check. The follow-up established
that the commands inspect different subjects: the explicit immutable range
fails on one extra terminal blank line in the committed round-one validation
report, while the no-argument command does not inspect committed candidate
changes. The uncertainty is resolved and validated as evidence-only
FIND-TASK-001-19.

No other discovery disagreement needed follow-up. Unique findings proceeded to
independent Ponytail validation as required.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED | INCORRECT | Charge the exact JCS replacement delta for admitted step payloads. |
| FIND-TASK-001-2 | CONFIRMED | INCORRECT | Preserve native OpenAI Responses reasoning continuation items. |
| FIND-TASK-001-5 | CONFIRMED | VIOLATION | Enforce the Workflow result ceiling before payload-bearing observation. |
| FIND-TASK-001-6 | REVISED | REGRESSION | Route every Workflow callback through one panic-isolated, cancellation/deadline-bounded best-effort boundary. |
| FIND-TASK-001-7 | CONFIRMED | VIOLATION | Move five function-scoped imports to their test-module import blocks. |
| FIND-TASK-001-8 | CONFIRMED | VIOLATION | Run the existing docs and examples lanes. |
| FIND-TASK-001-9 | CONFIRMED | VIOLATION | Run exact selectors for four named Rust tests. |
| FIND-TASK-001-10 | CONFIRMED | VIOLATION | Move this task's new PyO3 behavior into the Python SDK owner. |
| FIND-TASK-001-11 | CONFIRMED | INCORRECT | Generate precise typed projections for Workflow run results. |
| FIND-TASK-001-12 | CONFIRMED | VIOLATION | Document new observer implementations and Python hook arguments. |
| FIND-TASK-001-13 | CONFIRMED | INCORRECT | Correct the guide's generated step-ID behavior. |
| FIND-TASK-001-14 | CONFIRMED | INCORRECT | Terminalize spawned-but-never-polled work as unstarted, not cancelled with zero attempts. |
| FIND-TASK-001-15 | CONFIRMED | INCORRECT | Use checked deadline construction and stable pre-dispatch rejection. |
| FIND-TASK-001-16 | CONFIRMED | INCORRECT | Reject the sole retry count whose total attempts cannot fit `u32`. |
| FIND-TASK-001-17 | CONFIRMED | INCORRECT | Reject routing, framing, proxy, and internal names in secret bindings while retaining credential headers. |
| FIND-TASK-001-18 | CONFIRMED | INCORRECT | Sanitize external-gateway refusal bodies before returning provider errors. |
| FIND-TASK-001-19 | CONFIRMED | VIOLATION | Remove the extra terminal blank line and prove the cumulative range with explicit `git diff --check`. |

The complete source traces, decision-complete corrections, preserved adjacent
behavior, and focused closure proofs are in `findings-validation.md`.

## Prior-finding closure

- **FIND-TASK-001-3 is closed.** Revision 10 approves
  `HashMap<HeaderName, SecretString>` because header order is unobservable and
  `HeaderName` has no `Ord`; the candidate exposes exactly that public type.
- **FIND-TASK-001-4 is closed.** Revision 10 approves
  `RemoteProblem(Box<RemoteProblem>)` over the public five-field payload; the
  candidate and its direct consumers use exactly that shape.
- FIND-TASK-001-1/2 and FIND-TASK-001-5 through FIND-TASK-001-18 remain open
  after independent retracing. FIND-TASK-001-19 is new and evidence-only.

The closed findings are omitted from the active remediation ledger. No active
finding requires another specification revision.

## Verification limits

- Production source did not change after the round-one implementation, so the
  round-one scoped lane results remain the available broad implementation
  evidence rather than proof of the newly identified edge paths.
- The network-security reviewer reran the focused Workflow security test (1/1)
  and endpoint-policy tests (4/4); those tests pass but do not cover reserved
  binding names or reflected credentials.
- Existing focused tests do not exercise escaped-text expansion, reasoning
  replay, pre-observation result rejection, adversarial observers, pre-poll
  cancellation, unrepresentable deadlines/retries, reserved secret headers, or
  reflected-secret errors.
- `mise run docs:check`, `mise run check:examples`, and exact selectors for four
  named Rust tests are still unrecorded.
- `git diff --check
  a51af030b6039eea4b2914f3ebf2c31925d08721..28473e049705595306f2934cf4bc664168254086`
  exits 2 on the committed round-one validation report.
- Review did not modify the immutable candidate or substitute remediation for
  missing candidate evidence.

## Verdict

**FIX_REQUIRED**

Revision 10 removes the two public-seam blockers, leaving only bounded
implementation, source-structure, documentation, and evidence corrections.
The self-contained remediation task is
`TASK-001-R1-close-validated-runtime-gaps.md` in this directory.
