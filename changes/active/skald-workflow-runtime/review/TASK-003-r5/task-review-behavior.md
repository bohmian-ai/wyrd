# TASK-003-r5 behavior review

**Overall result: FAIL**

## Immutable subject and review limits

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003-R1 through TASK-003-R4 in the prior review directories

The human-approved `WyrdGatewayCall.model` amendment, corrected native-`401`
send-once wording, and run-start `spawn_blocking` decision were treated as
authoritative. The complete base-to-candidate source and recorded evidence were
reviewed. The candidate remained at the stated commit. This was a strictly
read-only source review: no build, compile, test, formatter, linter, Cargo,
mise, pnpm, pytest, or other verification command was run.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-024, REQ-031, REQ-045, REQ-046, AC-019: one shared Rust remote run handle provides exact create/get/cancel/wait behavior, direct run snapshots, stable create idempotency, one-second polling, terminal-value return, and drop-without-cancel semantics | `crates/shared/wyrd-client/src/workflow/remote.rs:17-121` owns `Workflows` over the existing `WyrdClient` transport and `submit_idempotent`; `sdks/wyrd-sdk-rust/src/lib.rs` re-exports the same type | Original task records the exact `shared_workflow_client_contract` selector and shared/Rust-SDK lanes; R1-R4 record the selector as preserved | PASS |
| REQ-036A, REQ-038, REQ-039, REQ-043, INV-011, INV-020: each public gateway call carries its Prompt-derived model, immutable fallback, remaining deadline, cancellation, and trace-only correlation through the existing native ingresses; unsupported Vertex refuses before dispatch | `crates/skald/skald-workflow/src/{plan,route}.rs`; `crates/shared/wyrd-client/src/workflow/gateway.rs:50-124,217-286`; the approved `WyrdGatewayCall.model` contract is in `crates/skald/skald-workflow/src/workflow.rs` | Original and remediation evidence records `public_gateway_call_context_and_errors`, Skald route tests, cancellation-after-dispatch proof, and all supported dialect projections | PASS |
| R1 corrected native-`401` behavior: send the model request once, renew before collecting a pending body, propagate renewal failure, otherwise return the original refusal, and never replay | `crates/shared/wyrd-client/src/transport/http.rs::post_native` keeps renewal in the shared auth owner and performs no second model POST | R1-R3 record complete-body, truncated-body, pending-body, renewal-success, and renewal-failure cases in the exact gateway caller selector | PASS |
| Task native-error contract: normalize OpenAI, Anthropic, and Google refusal envelopes to safe common fields; recognized codes use catalog title/remediation; uncoded statuses reuse provider categories; no raw body or arbitrary details survive | `crates/shared/wyrd-client/src/workflow/gateway.rs:157-214,313-339` | Original/R1 evidence records all three envelopes, status categories, catalog canaries, and redaction assertions | PASS |
| AC-011A, AC-012: authenticated OpenAI Chat/Responses, Anthropic, and Gemini ingresses decode the exact bounded fallback header after authentication, reject malformed/empty/duplicate/self-reference values before dispatch, never forward it, and publish the served OpenAPI contract | `crates/wyrd/wyrd-server/src/components/gateway/{ingress,routes}.rs`; `crates/wyrd-spec/src/gateway/policy.rs`; existing gateway invocation owner receives the typed fallback | Original task records the focused PG ingress proof and served `pg_openapi_contract` proof under the repository-managed Postgres lane; codegen and gateway regressions are recorded | PASS |
| REQ-058, INV-004, INV-012, AC-031: the shared local owner selects routes first, resolves only selected external secrets at run start, retains explicit native injection, reads no execution secret during load/apply, leaks no provider credential, and is reused by SDK/CLI projections | `crates/shared/wyrd-client/src/workflow/{mod,local}.rs`; `crates/shared/wyrd-utils/src/secret.rs`; Python delegates to the retained shared-client Workflow rather than duplicating setup | Original/R1-R3 evidence records selected binding behavior, unused-secret non-resolution, loading-without-dispatch, Python retained-client behavior, and client/PyO3 boundaries | PASS |
| R4 / reopened FIND-TASK-003-10 source behavior: one run-start `GlobalConfig` snapshot supplies both selected Workflow bindings and a client-less Wyrd gateway client; a retained client wins; purely native runs read no ambient configuration | `Workflow::run_with` calls the blocking setup at `workflow/mod.rs:131-149`; `load_local_setup` has one conditional `GlobalConfig::load` at `:191-201`; `local_setup_from` derives both `global.workflow` and `ClientConfig::from_global_with_env(&global)` at `:215-239` | Static source supports the correction. The recorded Rust proof does not exercise this reachable Workflow path; see `BEH-TASK003-R5-1` | FAIL |
| R4 retained-client closure: registered and authored-ref Python Workflows keep the loading client when ambient configuration points elsewhere or is absent | `WorkflowCards::load` retains `self.cards.engine.client` at `workflow/mod.rs:322-348`; Python stores and runs the shared Workflow; `test_workflow_gateway_context.py:123-181` drives hostile and absent ambient configuration | R4 records the exact repository-managed Python integration selector with `1 passed` | PASS |
| REQ-058/R4 selected-secret and loading boundary preservation: selected secrets still resolve only at run start, while loading/apply read none and dispatch nothing | `Workflow::run_with` prepares routes at run start; `SelectedRoutes::dependencies` resolves only selected bindings; loader paths retain no dependency preparation | Earlier exact selected-dependency and Python journey evidence remains applicable; R4 changes only snapshot derivation | PASS |
| R4 non-goals: no new cache, watcher, generation token, lock, reload protocol, async config API, setting, option, dependency, compatibility path, checker, allowlist, timing fixture, or language-specific execution owner | R4 production delta is confined to `workflow/mod.rs` and reuses `GlobalConfig`, `ClientConfig::from_global_with_env`, and `WyrdClient::with_config` | Cumulative diff and R4 commit range show no prohibited mechanism | PASS |
| INV-007 and AC-029/031 task boundary: Rust/Python/TypeScript local surfaces continue through the shared Workflow engine; Python/TypeScript gain no server-run lifecycle surface; Rust alone projects `Workflows` here | Shared-client Workflow remains the execution owner; Python wrapper delegates to it; Rust SDK re-exports the remote handle; no Python/TypeScript lifecycle API appears in the cumulative diff | Original task records Python and TypeScript unit/type/binding lanes plus Rust SDK proof; the R4 Python regression was rerun | PASS |
| Scope and regression boundaries: no new public ingress, Vertex public endpoint, gateway credential mutation API, provider-body Workflow context, polling knob, public arbitrary-header transport, recursive server call, competing transport, or unrelated runtime behavior | `post_native` remains crate-private; public caller uses existing ingresses and shared transport; remote and local owners remain separated | Cumulative source and recorded original/R1-R4 regression evidence | PASS |

## Proposed finding

### BEH-TASK003-R5-1 — MISSING: the R4 proof bypasses the mixed-route Workflow run-start path

- **Violated obligation:** R4 requires the existing selected-local-dependencies
  proof to exercise a client-less `Workflow` selecting both `ext_gateway` and
  `wyrd_gateway`, and requires the exact named selector and selected count to
  be recorded (`TASK-003-R4-use-one-run-start-config-snapshot.md:122,130-139,157-159`).
- **Exact location:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:783-826` and
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md:168-177`.
- **Evidence:** `mixed_routes_use_one_config_snapshot` parses a `GlobalConfig`
  and directly calls the private `local_setup_from(global, true, true, None)`
  helper. It constructs no mixed-route `Workflow`, never calls
  `Workflow::run_with`, never exercises `SelectedRoutes::of`, never reaches
  the approved `spawn_blocking`/`load_local_setup` boundary, and never resolves
  or dispatches either selected route. The pre-existing named test at
  `workflow/mod.rs:612-758` still tests an ExtGateway Workflow and a separate
  retained-client WyrdGateway Workflow, not one client-less Workflow with both
  route families. The implementation record substitutes a combined regex
  command selecting five tests for the mandated exact named command and does
  not record that selector's selected count.
- **Observable consequence:** source inspection indicates that the one-snapshot
  implementation is correct, but the required regression can stay green if
  route analysis or `Workflow::run_with` wiring stops selecting both needs, if
  the blocking setup boundary is bypassed, or if the real client-less mixed
  path stops consuming both values from the snapshot. The recorded evidence
  therefore does not close the previously reopened `FIND-TASK-003-10`.
- **Required testable correction:** extend or replace the existing shared-client
  unit proof so it drives one client-less mixed-route `Workflow` through the
  actual `Workflow::run_with` run-start path and observes use of both the
  selected external binding and the Wyrd client derived from the same ambient
  configuration. Preserve the static one-load source proof and existing
  retained-client/native laziness coverage. Rerun and record the exact focused
  selector required by R4 with its selected count; no new harness, hook,
  timing assertion, setting, or production mechanism is needed.

## Prior-finding closure

| Stable finding | Behavior-review result |
|---|---|
| `FIND-TASK-003-1` | CLOSED in source and recorded Python integration evidence: the shared/Python Workflow retains its loading client through edits and runs. |
| `FIND-TASK-003-2` | CLOSED: native `401` renewal occurs before pending-body collection, uses the auth owner, sends no replay, and preserves renewal-error precedence. |
| `FIND-TASK-003-3` | CLOSED: recognized native-envelope codes use derive-backed catalog title/remediation; upstream text is discarded. |
| `FIND-TASK-003-4` | CLOSED: plaintext secret-file reading remains private behind the redacted shared `SecretRef` reader. |
| `FIND-TASK-003-5` | CLOSED: changed imports remain at module tops. |
| `FIND-TASK-003-6` | CLOSED: changed declarations use imported bare or role-specific type aliases. |
| `FIND-TASK-003-7` | CLOSED: the identified panic-capable items have substantive panic contracts. |
| `FIND-TASK-003-8` | CLOSED: durable and externally visible operations document cancellation and partial progress. |
| `FIND-TASK-003-9` | CLOSED: recorded proof cancels after one model POST and observes no replay. |
| `FIND-TASK-003-10` | Source correction is present, and the Python retained-client rerun is recorded, but the mandated mixed-route Workflow proof is incomplete as `BEH-TASK003-R5-1`. |
| `FIND-TASK-003-11` | CLOSED: `force_refresh` documents retrying transport versus send-once native-call use. |
| `FIND-TASK-003-12` | CLOSED: `into_skald` documents the loading-client and automatic-dependency context it discards. |

## Verification-evidence assessment

The original task and R1-R3 records provide focused and broader evidence for
the remote handle, public gateway caller, native refusal normalization,
fallback ingress/OpenAPI, selected binding behavior, Python context retention,
boundary checks, code generation, formatting, and lints. R4 records green
package Clippy/formatting, a combined Rust selector, Python setup, the exact
Python retained-client integration, and `git diff --check`.

No command was rerun during this review. The R4 Rust record is not merely a
generic residual limit: its asserted new test bypasses the specific producer
and caller path the remediation required, and the prescribed exact selector
was not recorded. Under the task-review evidence rule, that is the bounded
finding above. No other behavior defect or unsupported scope drift was found.
