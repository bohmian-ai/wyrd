# TASK-001 round-three review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11 at `9a621a28a40b67e82e4ba119f7df577dde893f1e`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority:
  `changes/active/skald-workflow-runtime/review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-001-r2/TASK-001-R1-addendum-revision-11.md`

The candidate remained at the stated commit through discovery, focused
follow-up, and structured Ponytail validation. Revision 11 adds REQ-053: the
Skald Observer system is deleted and replaced with payload-free tracing spans.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Explicit Workflow graph, binding, routing, run contracts, and portable result | The Rust engine, Python projection, generated contracts, and direct consumers remain aligned. | PASS |
| Exact native provider behavior and Responses continuation | Reasoning identity, summaries, encrypted state, calls, and outputs are replayed in order. | PASS — prior findings closed |
| Bounded execution, deadlines, cancellation, retry, and owned lifetime | Existing precedence and drain behavior are present, but `StepTask` does not race the already-fixed Agent deadline. | FAIL — `FIND-TASK-001-20` |
| REQ-053 Observer deletion and payload-free tracing | The Observer crate and its hooks are removed without aliases and payload-bearing trace fields are absent, but panic settlement and three GenAI fields are inconsistent. | FAIL — `FIND-TASK-001-21`, `FIND-TASK-001-23` |
| ExtGateway policy and secret containment | Routing/header policy, DNS pinning, TLS, no-proxy, no-redirect, bounded bodies, and non-success sanitization are present, but successful typed responses can reflect a bound secret. | FAIL — `FIND-TASK-001-22` |
| Repository structure, documentation, and dependency rules | Public contracts remain cohesive, but required rustdoc is absent, stateful Agent orchestration remains free functions, a library directly adds `anyhow`, and changed permanent docs name deleted surfaces. | FAIL — `FIND-TASK-001-24` through `FIND-TASK-001-27` |
| Prior round-two findings | Every prior `FIND-TASK-001-1` through `FIND-TASK-001-19` was independently retraced and closed. | PASS |
| Required candidate evidence | Recorded scoped Rust, Python, codegen, boundary, docs, examples, and exact-test evidence is green; the explicit cumulative `git diff --check` exits zero. | PASS, but it does not exercise the eight retained gaps |
| Prohibited scope | No compatibility alias, second runtime, remote Workflow surface, durable scheduler, credential administration, or replacement Observer system was added. | PASS |

## Independent review results

| Report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | PASS | None |
| `task-review-invariants.md` | PASS | None |
| `standards-review.md` | FAIL | `STD-R3-001`, `STD-R3-002` |
| `maintainer-review.md` | FAIL | `MNT-R3-001` through `MNT-R3-004` |
| `system-review.md` | FAIL | `SYS-R3-001` |
| `domain-review-concurrency.md` | FAIL | `CONC-R3-001` |
| `domain-review-network-security.md` | FAIL | `NET-R3-001` |
| `domain-review-telemetry-privacy.md` | FAIL | `TEL-001`, `TEL-002` |
| `followup-review.md` | RESOLVED | Conflicts and overlapping correction boundaries reconciled |
| `findings-validation.md` | FIX_REQUIRED | Eight deduplicated findings |

All required reviewers and reports were available. No reviewer filled more than
one role.

## Follow-up decision

A fresh focused follow-up was required because discovery reports disagreed on
whether the fixed Agent deadline was enforced, whether REQ-053 tracing was
complete, and which OpenTelemetry GenAI values were mandatory. It also
reconciled overlapping rustdoc findings and independently retraced the security,
owner-shape, dependency, and documentation proposals.

The follow-up resolved the Agent and concurrency proposals to one missing
absolute deadline arm, retained the panic/run-span mismatch and successful
ExtGateway credential-reflection defects, narrowed the GenAI correction to
canonical provider/model fields and removal of the unsupported scalar finish
reason, combined the rustdoc proposals, and confirmed the remaining three
maintainer findings. The structured validator then retraced every proposal and
rejected unproved or duplicative corrections.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-20` | REVISED | INCORRECT | Race the existing fixed Agent deadline in `StepTask` with the required precedence and ordinary typed timeout settlement. |
| `FIND-TASK-001-21` | CONFIRMED | INCORRECT | Convert attempt panics while the attempt span is alive so the span and returned run both report the same internal failure. |
| `FIND-TASK-001-22` | CONFIRMED | INCORRECT | Refuse a successful ExtGateway response whose retained decoded content reflects a bound secret. |
| `FIND-TASK-001-23` | REVISED | INCORRECT | Emit canonical GenAI provider/model fields and remove the unsupported scalar finish-reason attribute. |
| `FIND-TASK-001-24` | CONFIRMED | VIOLATION | Add the required error, lifecycle, fixture, field, and trait-method rustdoc to the cited new or materially changed items. |
| `FIND-TASK-001-25` | CONFIRMED | VIOLATION | Make the two stateful Agent orchestration bodies inherent methods on `Agent`. |
| `FIND-TASK-001-26` | CONFIRMED | VIOLATION | Use a standard error at the resolver boundary and remove the direct library `anyhow` dependency. |
| `FIND-TASK-001-27` | CONFIRMED | REGRESSION | Remove deleted APIs and the absent example from the changed permanent documentation. |

The complete source traces, consequences, preserved behavior, rejected
corrections, and focused closure proofs are in `findings-validation.md`.

## Prior-finding closure

All prior `FIND-TASK-001-1` through `FIND-TASK-001-19` are closed. In
particular, Revision 11 closes the Observer/callback findings by deleting that
system, while the current deadline, panic, secret-reflection, telemetry,
documentation, owner-shape, and dependency findings are distinct reachable
gaps. No closed finding is carried into remediation.

## Verification limits

- Recorded green lanes and exact selectors do not exercise a pending terminal
  journal at the fixed Workflow Agent deadline, a panicking attempt, or a 2xx
  ExtGateway response that reflects a bound secret.
- The existing telemetry capture proves hierarchy and payload exclusion but is
  OpenAI-focused and does not prove canonical Gemini/Vertex provider/model
  fields or the finish-reason type.
- Green formatting, lint, docs, and examples lanes do not substitute for source
  compliance with the repository's rustdoc, struct-owner, library-error, and
  permanent-documentation rules.
- Review did not modify the immutable candidate or treat remediation as proof.

## Verdict

**FIX_REQUIRED**

Eight bounded implementation, source-structure, and documentation corrections
remain within approved Revision 11. The self-contained remediation task is
`TASK-001-R2-close-round-three-runtime-gaps.md` in this directory.
