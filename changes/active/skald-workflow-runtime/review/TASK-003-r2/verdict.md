# TASK-003-r2 verdict

**FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior verdict and remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/`
- Current review directory: `changes/active/skald-workflow-runtime/review/TASK-003-r2/`

The human-approved 2026-10-03 minimal amendment adding
`pub model: ModelRef` to `WyrdGatewayCall` and the R1 same-spec correction of
TASK-003's native-`401` wording are authoritative. Neither is drift. The
complete base-to-candidate range, the prior verdict and ledger, and the R1
remediation were included in this repeat review.

The review was strictly source-only. No build, compile, test, Cargo, mise,
pnpm, pytest, package-manager, formatter, linter, test-listing, or other
verification command was run by any report used in this verdict. One initial
repository-standards reviewer disclosed running a prohibited verification
command; that reviewer was disqualified and replaced. Its work was not used.
All accepted reports judge source, the cumulative diff, and the implementer's
recorded evidence. The candidate identity remained unchanged through discovery,
follow-up, and validation.

## Required independent reports

| Role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | FAIL; one proposal later rejected as DRIFT |
| Invariants | `task-review-invariants.md` | FAIL |
| Repository standards | `standards-review.md` | FAIL |
| Maintainer | `maintainer-review.md` | FAIL |
| System resilience | `system-review.md` | PASS |
| Security/credentials | `domain-review-security.md` | FAIL; one proposal later rejected as requiring spec authority |
| Concurrency/cancellation | `domain-review-concurrency.md` | FAIL |
| Focused follow-up | `followup-review.md` | RESOLVED |
| Independent Ponytail validation | `findings-validation.md` | Five retained findings |

All required roles and reports are complete. The unavailable/disqualified
standards attempt was not converted into a verification limit or accepted
report; a fresh replacement completed the required role.

## Claim comparison and follow-up decision

Discovery claims grouped into native-`401` renewal ordering, post-dispatch
cancellation proof, qualified declaration types, panic-contract documentation,
durable-operation cancellation documentation, receiver-side JCS strictness,
and OpenAI `param` handling.

A focused follow-up was required because the system-resilience report accepted
the `401` and cancellation paths while invariant and concurrency reviewers
identified narrower reachable/proof gaps. It also resolved two authority
questions. The follow-up confirmed the unreadable-body `401` renewal gap and
the missing post-dispatch cancellation proof. It rejected receiver-side JCS
byte equality as nonstandard, unapproved DRIFT and rejected changing OpenAI
`param` projection because Revision 12 expressly selects that field and defines
no permitted grammar or bound. Independent Ponytail validation reached the
same dispositions from source and authority.

## Reconciled acceptance matrix

| Obligation | Implementation and recorded evidence | Result |
|---|---|---|
| Shared create/get/cancel/wait handle, stable create idempotency, fixed polling, terminal snapshots, and drop-without-cancel | `wyrd-client/src/workflow/remote.rs`; recorded `shared_workflow_client_contract` | PASS |
| Rust SDK projects the shared handles without Python activation | Shared and Rust SDK exports; recorded SDK and boundary evidence | PASS |
| Prompt-derived model plus immutable per-call fallback, deadline, cancellation, and correlation; supported ingress and Vertex refusal | Skald route/call types and `PublicWyrdGatewayCaller`; recorded focused transport proof | PASS for implementation behavior |
| One native model POST; every observed `401` renews through the existing auth owner without replay | `HttpTransport::post_native` sends once, but body collection can fail before renewal | **FAIL — FIND-TASK-003-2** |
| Safe native error normalization uses catalog text for recognized codes and preserves the approved optional OpenAI field | `workflow/gateway.rs`; recorded three-dialect canaries | PASS; proposed field restriction rejected as a spec change |
| Fallback producer uses unpadded base64url over JCS; receiver decodes, bounds, deserializes, and validates after authentication | `wyrd-spec` codec and authenticated public ingress; recorded policy, PG, and served OpenAPI evidence | PASS; receiver byte-recanonicalization rejected as DRIFT |
| Shared selected local dependencies, selected-only secret resolution, no load/apply dispatch, explicit native injection | `workflow/local.rs`, shared config and secret reader; recorded focused/shared evidence | PASS |
| Python retains the complete loading client through registered/authored loading, mutation, and run | Shared `Workflow` owner and Python wrapper; recorded public Python journey | PASS — FIND-TASK-003-1 closed |
| Redacted secret boundary and mandatory function-local import placement | Shared secret reader and corrected imports; recorded/static evidence | PASS — FIND-TASK-003-4 and -5 closed |
| Changed declarations use imported bare type names | Multiple new fields/signatures retain qualified paths | **FAIL — FIND-TASK-003-6** |
| New panic-capable Rust tests/helpers document their panic contracts | New task-owned items omit required `# Panics` sections | **FAIL — FIND-TASK-003-7** |
| New durable/async operations document cancellation and partial progress | `create`, `cancel`, and native POST docs omit material post-dispatch behavior | **FAIL — FIND-TASK-003-8** |
| Scenario 2 directly proves cancellation of an already-dispatched public-gateway call | Recorded selector proves pre-cancel and in-flight timeout, not post-dispatch cancellation | **FAIL — FIND-TASK-003-9** |
| Non-goals: no new ingress, Vertex endpoint, credential mutation, provider-body Workflow context, retry/polling setting, arbitrary-header API, duplicate transport/graph/executor, or bespoke check | Complete cumulative diff and callers | PASS |

## Validated finding ledger

The independent source traces, dispositions, correction boundaries, and proof
requirements are in `findings-validation.md`.

| ID | Status / classification | Required outcome |
|---|---|---|
| `FIND-TASK-003-2` | REVISED, reopened / INCORRECT | After the native response status is known to be `401`, renew through the existing auth owner even if body collection fails; never replay the model POST. |
| `FIND-TASK-003-6` | CONFIRMED / VIOLATION | Use module imports and bare, role-specific type names in every cited changed declaration. |
| `FIND-TASK-003-7` | REVISED / VIOLATION | Add substantive panic-contract rustdoc to the identified new panic-capable tests and helpers. |
| `FIND-TASK-003-8` | CONFIRMED / VIOLATION | Document cancellation and partial-progress behavior on the new create, cancel, and native-post operations. |
| `FIND-TASK-003-9` | CONFIRMED / MISSING | Directly prove post-dispatch public-gateway cancellation in the existing focused selector. |

`B-001` is omitted because strict receiver reserialization/equality is not
required by approved authority or ordinary JSON receiver practice and would be
DRIFT. `SEC-R2-001` is omitted because constraining or removing the expressly
approved OpenAI field requires specification authority. No rejected proposal
is retained as optional advice.

## Verification limits and prior-finding closure

The implementation record contains successful focused, family, Python,
gateway, served OpenAPI, codegen, typing, boundary, format, and lint evidence.
Source supports that evidence for the exercised cases. It does not exercise an
unreadable native `401` body or cancellation after the model POST reaches the
HTTP boundary. Those omissions accompany validated findings and must be rerun
by the implementer; this review did not execute them.

Prior `FIND-TASK-003-1`, `-3`, `-4`, and `-5` are closed. Prior
`FIND-TASK-003-2` is reopened only for the same renewal invariant when a known
`401` body cannot be collected. The declaration, documentation, and direct
cancellation-proof findings are new and use the next unused stable IDs.

## Remediation route

`TASK-003-R2-close-renewal-proof-and-source-contracts.md` packages the five
validated bounded findings for `$wyrd-implement`. The selected corrections use
the existing transport/auth owner, import blocks, rustdoc, and focused test.
They require no new public API, architecture, security decision, compatibility
path, concurrency semantics, persistence decision, dependency, mechanism,
checker, setting, option, allowlist, or test harness.
