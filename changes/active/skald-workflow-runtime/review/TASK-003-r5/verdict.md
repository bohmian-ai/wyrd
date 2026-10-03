# TASK-003-r5 verdict

**Verdict: FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R4 and their validated ledgers

The review applied the human-approved 2026-10-03 minimal amendment adding
`pub model: ModelRef` to `WyrdGatewayCall`, the R1 same-spec correction to the
native-`401` task wording, and the approved run-start `spawn_blocking` boundary
for configuration, client, and selected-secret reads. Those reads remain at
run start because Workflow loading must read no execution secret.

The complete base-to-candidate range was reviewed. The candidate remained the
stated commit throughout the independent review and validation passes.

## Independent review results

| Required report | Result | Material proposal |
|---|---|---|
| `task-review-behavior.md` | FAIL | R4's helper-level fixture does not prove the required client-less mixed-route Workflow run-start path. |
| `task-review-invariants.md` | FAIL | The source invariant is corrected, but the producer-to-consumer proof and exact evidence record remain incomplete. |
| `standards-review.md` | FAIL | Active gateway architecture synchronization and complete run-start rustdoc are missing. |
| `maintainer-review.md` | FAIL | Rust run-start docs contradict source, and the Python public stub retains the pre-change local-only contract. |
| `system-review.md` | FAIL | The required mixed-route recovery/configuration path is not exercised by the recorded helper test. |
| `domain-review-security.md` | PASS | No security, authentication, tenancy, credential, or redaction defect. |
| `domain-review-concurrency.md` | PASS | No remaining runtime concurrency, cancellation, renewal, or snapshot defect. |

The initially assigned invariant reviewer disclosed running a prohibited
verification command. Its report was excluded and replaced by a fresh
source-only invariant review. All reports used for this verdict were produced
without builds, tests, lanes, Cargo, mise, pnpm, pytest, or other verification
commands.

## Follow-up decision

A focused follow-up was required because the behavior, invariant, and system
reviews found the R4 proof incomplete while the concurrency review accepted
it. `followup-review.md` resolved the conflict: production source now derives
both mixed-route consumers from one configuration snapshot, but the new test
calls `local_setup_from` with manually supplied flags and bypasses
`SelectedRoutes::of`, `Workflow::run_with`, `spawn_blocking`, and
`load_local_setup`. The recorded five-test union also does not provide R4's
mandated exact selector and selected count. This remains closure work for
stable `FIND-TASK-003-10`.

## Reconciled acceptance matrix

| Obligation | Source and recorded evidence | Result |
|---|---|---|
| Shared remote create/get/cancel/wait contract, stable idempotency, terminal polling, and Rust SDK projection | `wyrd-client::Workflows`, shared transport, Rust SDK re-export, and recorded focused transport/SDK evidence | PASS |
| Immutable per-call model, fallback, deadline, cancellation, and trace-only correlation through existing public gateway ingress | Skald route adapter, `PublicWyrdGatewayCaller`, native projection, and recorded protocol/concurrency cases | PASS |
| Native `401` send-once renewal before body collection, no replay, and renewal-error precedence | `HttpTransport::post_native`, shared auth owner, and recorded complete/truncated/pending-body cases | PASS |
| Safe native error normalization and redaction | Shared gateway error projection plus recorded OpenAI/Anthropic/Google/category cases | PASS |
| Authenticated bounded fallback header consumption, rejection, non-forwarding, and served OpenAPI projection | Typed gateway policy and server ingress/routes plus recorded PG/OpenAPI evidence | PASS |
| Shared local dependency composition, selected-secret-only resolution, retained client context, and language delegation | Shared Workflow owner, selected-route owner, Python retained facade, and recorded retained-client journey | PASS |
| One run-start `GlobalConfig` snapshot supplies client-less mixed-route bindings and Wyrd client | Correct in production source, but required live Workflow-path proof and exact selector/count are absent | FAIL — `FIND-TASK-003-10` |
| Active architecture records the authenticated fallback-header contract required by Revision 12 `INV-015` | Runtime contract and served OpenAPI exist; `architecture/wyrd-design.md` does not record the boundary | FAIL — `FIND-TASK-003-13` |
| Materially changed Rust run-start operations document ambient IO, cancellation, and partial progress accurately | `run_with` states an incomplete trigger; `run`, `SelectedRoutes::dependencies`, and `resolve_binding` omit applicable cancellation/partial-progress facts | FAIL — `FIND-TASK-003-14` |
| Python public declaration matches the changed shared Workflow runtime | PyO3 runtime docs are current; source and assembled public stubs still describe only process-local execution | FAIL — `FIND-TASK-003-15` |
| Prohibited scope remains excluded | No new ingress, Vertex endpoint, credential mutation API, polling option, arbitrary-header API, competing transport, cache, watcher, checker, or language-specific executor | PASS |

## Validated finding ledger

The mandatory independent Ponytail validation confirmed or revised every
proposal and retained only these findings:

| Finding | Status and classification | Required outcome |
|---|---|---|
| `FIND-TASK-003-10` | CONFIRMED / MISSING | Exercise one client-less mixed-route Workflow through the real shared run-start path and record R4's exact selector/count, reusing existing fixtures and production owners. |
| `FIND-TASK-003-13` | CONFIRMED / VIOLATION | Synchronize the implemented authenticated fallback-header contract into the existing gateway section of `architecture/wyrd-design.md`. |
| `FIND-TASK-003-14` | REVISED / INCORRECT, VIOLATION | Correct and complete operation-local Rust documentation for shared run-start ambient IO, cancellation, and partial progress without changing runtime behavior. |
| `FIND-TASK-003-15` | CONFIRMED / MISSING, VIOLATION | Update the Python source-stub contract for `Workflow.run` and regenerate the assembled public stub through the existing owner. |

No retained finding requires a new product, public API, architecture choice,
security model, compatibility contract, concurrency semantic, resource owner,
or persistent-data decision. `SPEC_REVISION_REQUIRED` is not indicated. The
corrections add no nonstandard mechanism, setting, file, option, checker,
dependency, or compatibility path, so no additional DRIFT is present.

## Prior-finding closure

`FIND-TASK-003-1` through `-9`, `-11`, and `-12` remain closed. The production
portion of `FIND-TASK-003-10` is corrected and the Python retained-client rerun
is recorded, but its explicitly required mixed-route Workflow proof remains
open. Findings `-13` through `-15` are new, independently validated source or
contract-documentation defects.

## Verification limits

This review was strictly source-only under the human standing direction. No
reviewer whose report is used here built, compiled, or ran tests, Cargo, mise,
pnpm, pytest, formatting, linting, type checking, code generation, or any other
verification command. Existing implementation evidence was judged from its
record and source representation. Missing, narrower, or source-contradicted
evidence was treated as a finding rather than rerun during review.

## Remediation route

Route
`changes/active/skald-workflow-runtime/review/TASK-003-r5/TASK-003-R5-close-proof-and-contract-documentation.md`
directly to `$wyrd-implement`. A later `$wyrd-task-review` must reassess the
complete base-to-new-candidate range against the original task and all five
remediation packets.
