# TASK-003-r3 verdict

**FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation tasks:
  `review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
  and
  `review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall` and the R1 same-spec native-`401` wording correction are
authoritative and are not drift. The complete base-to-candidate range was
reviewed. `HEAD` remained the candidate throughout the review.

The review was strictly read-only with respect to the implementation. No
reviewer built, compiled, ran tests or lanes, or invoked Cargo, mise, pnpm,
pytest, or another verification command. The only mutations are the requested
review artifacts. Source, the cumulative diff, and the implementer's recorded
evidence are the acceptance inputs.

## Independent review results

| Role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | PASS |
| Invariants | `task-review-invariants.md` | PASS |
| Repository standards | `standards-review.md` | FAIL |
| Maintainer | `maintainer-review.md` | PASS |
| System resilience | `system-review.md` | FAIL |
| Security/RBAC | `domain-review-security.md` | FAIL |
| Concurrency/cancellation | `domain-review-concurrency.md` | FAIL |
| Focused follow-up | `followup-review.md` | RESOLVED |
| Independent Ponytail validation | `findings-validation.md` | Four bounded retained findings |

All required reviewers and reports were available. No reviewer filled more than
one role.

## Claim comparison and follow-up decision

Discovery materially conflicted over whether R2 closed renewal for every
observed native `401`. It also produced three paths not covered by all
reviewers: synchronous ambient configuration IO on the async Workflow path,
the retry contract documented on `force_refresh`, and client context discarded
by `Workflow::into_skald`. A fresh follow-up traced all four uncertainties and
resolved them from source. Independent Ponytail validation then retained the
four findings below, rejected cancellation-safe cache retirement as unapproved
DRIFT, and required no new mechanism, check, file, setting, option, dependency,
or test harness.

## Reconciled acceptance matrix

| Obligation | Source and recorded evidence | Result |
|---|---|---|
| Shared remote create/get/cancel/wait contract, stable create idempotency, direct snapshots, fixed polling, and drop semantics | `workflow/remote.rs`; recorded `shared_workflow_client_contract` and shared/Rust SDK evidence | PASS |
| Prompt-derived model plus immutable fallback, deadline, cancellation, and trace-only correlation across supported public protocols; Vertex refusal | Skald route/plan and shared public caller; recorded focused transport evidence | PASS |
| Native call sends one model POST, never replays, and renews every known `401` before a pending body can prevent the attempt | `transport/http.rs` learns `401`, then awaits the full body before `force_refresh`; R2 proof closes an abruptly truncated body only | FAIL — `FIND-TASK-003-2` |
| Safe native error projection uses catalog/category text and approved fields only | `workflow/gateway.rs`; recorded three-dialect canary and uncoded-status evidence | PASS |
| Authenticated fallback-header decoding, bounds, validation, typed forwarding, omission behavior, and served OpenAPI | gateway policy, ingress, routes, and recorded PG/OpenAPI evidence | PASS |
| Shared local preparation selects routes before secrets and performs all task-added ambient filesystem work outside the async executor | `Workflow::run_with` directly loads ambient config and can assemble a client through synchronous config/credential IO | FAIL — `FIND-TASK-003-10` |
| Rust/Python/TypeScript retain the loading Workflow client; selected secrets resolve only at run time | shared Workflow owner, Python wrapper, TypeScript native wrapper, and recorded Python context journey | PASS |
| Auth owner documentation accurately describes replay-safe retry callers and native send-once callers | `AuthMiddleware::force_refresh` still says the caller retries once, while TASK-003's native consumer must not retry | FAIL — `FIND-TASK-003-11` |
| Public consuming conversion documents loss of retained client context and automatic dependency composition | `Workflow::into_skald` consumes the new client-bearing facade without documenting that loss | FAIL — `FIND-TASK-003-12` |
| Prior import, panic-contract, cancellation-contract, and post-dispatch cancellation corrections | Current source and recorded R2 evidence | PASS |
| Non-goals and drift boundary | No new ingress, credential API, public arbitrary-header API, polling/retry option, duplicate engine/transport, or unsupported permanent check | PASS |

## Validated finding ledger

| ID | Status and classification | Required outcome |
|---|---|---|
| `FIND-TASK-003-2` | REVISED / INCORRECT | In the existing native HTTP owner, attempt `force_refresh` as soon as the response is known to be `401`, before body collection can postpone it; preserve one POST, no replay, renewal-error precedence, and existing response/body-error behavior. |
| `FIND-TASK-003-10` | REVISED / VIOLATION | Move the complete task-added ambient configuration and client assembly that may touch the filesystem onto the established blocking boundary, while preserving one lazy snapshot, retained clients, selected-secret-only behavior, and current errors. |
| `FIND-TASK-003-11` | CONFIRMED / VIOLATION | Correct `force_refresh` rustdoc to distinguish replay-safe callers from the native send-once caller that refreshes only for later calls. |
| `FIND-TASK-003-12` | CONFIRMED / VIOLATION | Document that `Workflow::into_skald` discards retained client context and automatic shared dependency composition. |

The complete source traces, rejected alternatives, and closure proofs are in
`findings-validation.md`.

## Verification limits and prior-finding closure

The implementer recorded green focused and broader evidence for the remote
handle, public caller, fallback ingresses, OpenAPI, language projections,
selected dependency behavior, Python client retention, code generation,
boundaries, formatting, and lints. This review did not rerun it. The recorded
cut-off-body case does not cover a body that remains pending after `401`
headers; that missing and source-contradicted proof is part of
`FIND-TASK-003-2`, not a generic verification limit.

`FIND-TASK-003-1`, `-3`, `-4`, `-5`, `-6`, `-7`, `-8`, and `-9` remain closed.
`FIND-TASK-003-2` is reopened and revised. Findings `-10`, `-11`, and `-12` are
new, bounded findings. None requires a specification revision or a new product,
public API, architecture, security, compatibility, cross-service,
persistent-data, or concurrency decision.

## Remediation route

`TASK-003-R3-close-pending-renewal-and-async-context-contracts.md` packages the
validated findings for `$wyrd-implement`. The next task review must reassess
the complete original base-to-new-candidate range and all three remediation
tasks, not only the R3 delta.
