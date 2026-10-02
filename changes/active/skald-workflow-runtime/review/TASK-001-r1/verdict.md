# TASK-001 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate remained at the stated commit throughout discovery, follow-up,
and validation. The review inspected the complete base-to-candidate range.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Explicit Workflow graph, binding, route, and run contracts | Contract, schema, plan, builder, and execution coverage exists, but the fixed external-binding map and `RemoteProblem` public variant do not match the approved seam. | FAIL — FIND-TASK-001-3, FIND-TASK-001-4 |
| Native Agent-only execution and provider dialect preservation | The shared Agent loop is used, but OpenAI Responses continuation drops reasoning items. | FAIL — FIND-TASK-001-2 |
| Exact namespaced outputs and bounded snapshots | Namespaced projection is present, but escaped text can exceed `max_run_bytes` and oversized model output reaches observers before enforcement. | FAIL — FIND-TASK-001-1, FIND-TASK-001-5 |
| Bounded retries, deadlines, cancellation, and owned task lifetime | JoinSet ownership and ordinary precedence paths are covered, but callbacks can defeat liveness, a cancellation race emits zero-attempt active steps, extreme deadlines panic, and the maximum retry count overflows. | FAIL — FIND-TASK-001-6, FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16 |
| ExtGateway exact binding, SSRF defense, and secret containment | DNS screening/pinning, TLS, no-proxy, and no-redirect behavior are present, but secret bindings admit routing/transport headers and refusal bodies retain reflected credentials. | FAIL — FIND-TASK-001-17, FIND-TASK-001-18 |
| Rust and Python authoring/result parity | Rust authoring moved with the contract, but new PyO3 behavior is outside the SDK owner and public Python declarations erase exact result DTOs. | FAIL — FIND-TASK-001-10, FIND-TASK-001-11 |
| Repository documentation and maintainability rules | Generated surfaces largely moved together, but imports, observer documentation, and the public step-ID guide violate repository rules or behavior. | FAIL — FIND-TASK-001-7, FIND-TASK-001-12, FIND-TASK-001-13 |
| Required verification evidence | Recorded Skald/shared/Wyrd/Python/codegen/lint lanes are useful, but docs/examples lanes and four exact named-test selectors are absent. | FAIL — FIND-TASK-001-8, FIND-TASK-001-9 |
| Prohibited scope and unrelated changes | Validation did not retain a finding for a new crate, dependency, remote Python/TS/MCP Workflow surface, compatibility translation, or unrelated redesign. | PASS |

## Independent review results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | FAIL | BEH-001 through BEH-004 |
| `task-review-invariants.md` | FAIL | INV-001 through INV-003 |
| `standards-review.md` | FAIL | STD-001 through STD-003 |
| `maintainer-review.md` | FAIL | MNT-001 through MNT-005 |
| `system-review.md` | FAIL | SYS-001 |
| `domain-review-concurrency.md` | FAIL | CONC-001 through CONC-003 |
| `domain-review-network-security.md` | FAIL | NET-001 and NET-002 |
| `followup-review.md` | RESOLVED | No new findings |
| `findings-validation.md` | SPEC_REVISION_REQUIRED | 18 validated, deduplicated findings |

## Follow-up decision

A focused follow-up was required because discovery reports materially differed
on ExtGateway security, new PyO3 ownership, and whether two packet-local seams
were binding public contracts. The follow-up resolved all three conflicts:

- NET-001 and NET-002 are reachable, with NET-002 narrowed to retained
  error/`Debug` exposure rather than current journal or Workflow propagation.
- The newly added PyO3 behavior belongs in the Python SDK owner; the retained
  owner-crate migration exception does not authorize adding new wrappers there.
- The external-binding map and `RemoteProblem` variant are fixed public seams,
  not private mechanics. Their implementation constraints require renewed
  authority.

No additional discovery finding was introduced. The structured Ponytail pass
then validated every proposed claim independently.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED | INCORRECT | Charge the exact JCS replacement delta for admitted step payloads. |
| FIND-TASK-001-2 | CONFIRMED | INCORRECT | Preserve native OpenAI Responses reasoning continuation items. |
| FIND-TASK-001-3 | CONFIRMED — SPEC_REVISION_REQUIRED | VIOLATION | Approve an implementable public external-binding collection type. |
| FIND-TASK-001-4 | CONFIRMED — SPEC_REVISION_REQUIRED | VIOLATION | Decide the public `RemoteProblem` variant shape under the error-size constraint. |
| FIND-TASK-001-5 | CONFIRMED | VIOLATION | Enforce the Workflow result ceiling before payload-bearing observation. |
| FIND-TASK-001-6 | REVISED | REGRESSION | Route Workflow callbacks through one panic-isolated, cancellation/deadline-bounded best-effort boundary. |
| FIND-TASK-001-7 | CONFIRMED | VIOLATION | Move five function-scoped imports to their test-module import blocks. |
| FIND-TASK-001-8 | CONFIRMED | VIOLATION | Run and record the existing docs and examples lanes. |
| FIND-TASK-001-9 | CONFIRMED | VIOLATION | Run and record exact selectors for four named Rust tests. |
| FIND-TASK-001-10 | CONFIRMED | VIOLATION | Move this task's new PyO3 behavior into the Python SDK owner. |
| FIND-TASK-001-11 | CONFIRMED | INCORRECT | Generate precise typed projections for Workflow run results. |
| FIND-TASK-001-12 | CONFIRMED | VIOLATION | Document new observer implementations and Python hook arguments. |
| FIND-TASK-001-13 | CONFIRMED | INCORRECT | Correct the guide's generated step-ID behavior. |
| FIND-TASK-001-14 | CONFIRMED | INCORRECT | Terminalize spawned-but-never-polled work as unstarted, not cancelled with zero attempts. |
| FIND-TASK-001-15 | CONFIRMED | INCORRECT | Use checked deadline construction and stable pre-dispatch rejection. |
| FIND-TASK-001-16 | CONFIRMED | INCORRECT | Reject the sole retry count whose total attempts cannot fit `u32`. |
| FIND-TASK-001-17 | CONFIRMED | INCORRECT | Reject routing, framing, proxy, and internal names in secret bindings while retaining credential headers. |
| FIND-TASK-001-18 | REVISED | INCORRECT | Sanitize external-gateway refusal bodies before returning provider errors. |

The decision-complete corrections and focused closure proofs are recorded in
`findings-validation.md`. BEH-001/INV-001 were consolidated into
FIND-TASK-001-1; INV-003/SYS-001 into FIND-TASK-001-6; the import portion of
MNT-004 into FIND-TASK-001-7. MNT-004's test-splitting proposal was rejected as
unrequired scope.

## Verification limits

- Discovery reviewers reran selected focused tests, and the task records green
  Skald, shared, Wyrd, Python, codegen, boundary, formatting, and lint lanes.
- The existing focused tests do not exercise the retained escaped-text,
  reasoning-continuation, adversarial-observer, pre-poll cancellation,
  maximum-timeout/retry, reserved secret-header, or reflected-secret paths.
- `mise run docs:check`, `mise run check:examples`, and exact selectors for four
  named Rust tests are not recorded.
- Review is read-only and did not substitute new aggregate verification for the
  candidate's implementation evidence.

## Prior-finding closure

This is the first review attempt for TASK-001. There are no prior `FIND-*`
identifiers or remediation findings to close.

## Verdict

**SPEC_REVISION_REQUIRED**

FIND-TASK-001-3 and FIND-TASK-001-4 require human-approved public-seam
decisions before a remediation task can be decision-complete. No remediation
task is emitted. The remaining findings are bounded implementation or evidence
corrections under the revised authority.
