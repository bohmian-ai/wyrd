# TASK-003 r6 invariant review

**Overall result: PASS**

## Immutable subject and evidence boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R5 and their preserved findings, verdicts, and remediation tasks under `changes/active/skald-workflow-runtime/review/`

I reviewed the complete base-to-candidate diff and candidate source. The repository has no `.codegraph/` directory, so the repository instruction correctly routes navigation to ordinary source and Git inspection. Per the caller's strict read-only direction, I did not build, compile, run tests, invoke Cargo/Mise/Python/Node tooling, run formatting/lints/codegen, or execute any verification lane. Verification entries below are the immutable records in the task and remediation artifacts, checked for correspondence to the candidate source rather than rerun.

## Producer-to-consumer invariant trace

| State or invariant | Producer | Consumers and sinks traced | Assessment |
|---|---|---|---|
| Loading-client context | `Workflow::from_path` records the lazy Cards client's `WyrdClient`; `WorkflowCards::load` records its parent Cards client (`crates/shared/wyrd-client/src/workflow/mod.rs:78-96, 301-347`) | Shared `Workflow::run_with`; Python `PyWorkflow` storage/run; TypeScript `NativeWorkflow` storage/run; authoring mutation through `as_skald_mut` | Context is retained without copying a bearer or consulting ambient connection settings for a loaded Workflow. |
| Run-start configuration snapshot | `Workflow::run_with` computes `SelectedRoutes`, then `load_local_setup` performs at most one `GlobalConfig::load` and derives both binding config and a client-less Wyrd client from it (`workflow/mod.rs:142-160, 190-237`) | `SelectedRoutes::dependencies`, selected secret resolution, `PublicWyrdGatewayCaller` | A client-less mixed route cannot combine two file generations. Retained-client precedence and native-only no-read behavior remain explicit. |
| Selected external secrets | `SelectedRoutes::of` derives exact binding names from resolved routes; `dependencies` iterates that ordered set only (`workflow/local.rs:30-110`) | `resolve_binding`, `ExternalGatewayBindings`, Skald route validation | Unselected references are never read. Earlier completed reads are discarded on failure/drop; model dispatch begins only after complete dependency preparation. |
| Per-attempt gateway context | Skald plan resolves the Prompt provider/model and stores `ModelRef`, fallback, attempt deadline, cancellation, and correlation in the private route adapter (`crates/skald/skald-workflow/src/plan.rs:234-305`; `route.rs:281-428, 464-480, 543-590`) | `WyrdGatewayCall`, shared public caller, trace span, fallback header | Each call receives owned/immutable context; it is not stored in a shared registry, native request extension, or global header map. |
| Native model request and auth renewal | `PublicWyrdGatewayCaller` projects the native dialect and calls the shared HTTP transport once (`workflow/gateway.rs:50-124, 217-287`); `post_native` observes status, renews on `401` before body collection, then returns the original response (`transport/http.rs:351-411`) | `AuthMiddleware::force_refresh`, later calls through the shared token cache, error normalization | One model POST is sent. Renewal failure has precedence, body delay cannot postpone renewal, and neither cancellation nor timeout causes a resend. |
| Native refusal normalization | Native protocol envelopes supply only a recognized code and optional OpenAI `param`; catalog/category owners supply safe message/remediation (`workflow/gateway.rs:157-214, 313-340`) | `RemoteProblem`, Skald provider error projection, bounded Workflow run error | Provider body text and arbitrary details do not cross the portable boundary. The approved OpenAI field remains the only optional envelope detail. |
| Fallback header | `GatewayFallbackOverride::to_header_value` produces unpadded base64url over JCS (`crates/wyrd-spec/src/gateway/policy.rs:88-173`) | Authenticated OpenAI/Anthropic/Gemini ingress; `requested_fallback`; `GatewayCallRequest.fallback` | Authentication wraps handlers before interpretation; duplicate/malformed/oversized/empty/duplicate-candidate/self-referential values fail before dispatch; the header is consumed rather than forwarded. Absence leaves tenant policy unchanged. |
| Remote lifecycle identity | `Workflows::create` delegates to the existing stable-key idempotent submission owner; run paths are derived only from typed UUID IDs (`workflow/remote.rs:17-121`) | create/get/cancel/wait direct snapshots | Create keeps one key within its retry loop; terminal failure states remain returned snapshots; dropping wait performs neither cancel nor resubmit. |
| Secret file rule | `wyrd_utils::secret::read_secret_ref` owns environment/file reads and the open-handle, regular-file, owner-only, bounded rule (`crates/shared/wyrd-utils/src/secret.rs:1-76`) | Gateway credential resolver, managed-key loading, local Workflow bindings | The refactor retains existing server/gateway safety while giving local composition the same redacted result boundary. No extra cache, watcher, reload protocol, setting, or secret surface was introduced. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/045/046 and AC-019: one exact shared create/get/cancel/wait facade, direct run DTOs, stable create idempotency, one-second polling, terminal snapshots, and drop-without-cancel | `crates/shared/wyrd-client/src/workflow/remote.rs:17-121`; shared and Rust SDK exports | Original task records `shared_workflow_client_contract` PASS and Rust SDK/boundary lanes PASS | PASS |
| REQ-036A/038/039/043 and INV-020: Prompt-derived model plus immutable fallback, timeout, cancellation, and trace-only correlation across supported public protocols; reject public Vertex before IO | Skald `plan.rs:234-305`, `route.rs:281-428, 464-480, 543-590`; client `workflow/gateway.rs:50-124, 217-287` | Original task records the focused public-caller test and Skald route suite PASS | PASS |
| Native calls send one model POST and every observed `401` renews before body collection without replay | `transport/http.rs:351-411`; `auth.rs:503-544` | R3 evidence records complete, truncated, and pending-body renewal cases; r2/r3/r4 source closure is preserved | PASS |
| Native refusal projection retains only safe status/code/message/field/remediation and uses catalog text for recognized codes | `workflow/gateway.rs:157-214, 313-340` | Original/R1 evidence records OpenAI, Anthropic, Google, uncoded-category, canary, and recognized-code cases | PASS |
| AC-011A/012: authenticated bounded fallback-header consumption on OpenAI Chat/Responses, Anthropic, and Gemini, with typed forwarding, non-forwarding, absent-header compatibility, and served documentation | `gateway/policy.rs:88-184`; `gateway/ingress.rs:101-139, 189-224`; affected ingress/route assembly; `architecture/wyrd-design.md:434-443` | Original task records the PG ingress module and served OpenAPI contract PASS | PASS |
| REQ-058, AC-029/031, INV-004/011/012: one shared route-first local composition, selected-secret-only run-time resolution, retained Cards client, native injection, and no load/apply execution-secret read | `workflow/mod.rs:36-43, 78-160, 190-237, 301-347`; `workflow/local.rs:30-149`; Python and TypeScript wrappers delegate to the shared owner | Original, R1, R4, and R5 records include shared-client, retained-client Python, TypeScript, and boundary evidence | PASS |
| R4/R5 `FIND-TASK-003-10`: one client-less mixed-route run uses the real run-start owner and one configuration snapshot for both external binding and Wyrd client | Production path above; `workflow/mod.rs:766-833` drives `Workflow::run_with` with both route families and observes both consumers | R5 records exact selector `test(=workflow::tests::selected_local_dependencies_use_shared_config)`, 1 selected/1 passed/221 skipped, plus mutation evidence; retained-client Python journey 1 passed | PASS |
| Rust, Python, and TypeScript project the same client-bearing Workflow owner without a second runtime/config parser; Rust remains Python-free | Shared facade; `sdks/wyrd-sdk-python/src/workflow.rs:204-215, 510-580`; `sdks/wyrd-sdk-python/src/state/mod.rs:2557-2639`; `sdks/wyrd-sdk-ts/native/src/workflow.rs:21-49, 109-159`; Rust SDK re-export | Original/R1/R5 records shared, SDK, PyO3/client-tier, Python retained-client, TypeScript native/type evidence | PASS |
| Public contracts and operation documentation match the implemented run-start behavior | `architecture/wyrd-design.md:434-443`; shared Workflow/local rustdoc; Python source and assembled stubs at `agent.pyi:716-736` | R5 records source comparison, clippy, stub regeneration/codegen, Python formatting/lints/typecheck PASS | PASS |
| Prior repository import, type-name, panic, cancellation, and ownership rules remain closed | Candidate modules keep imports at module scope, bare imported declaration types, owner methods, and the cited panic/cancellation contracts | R1-R3 recorded focused source and lane evidence; cumulative source retains each correction | PASS |
| Non-goals and standing DRIFT rule: no new ingress, Vertex endpoint, credential mutation API, provider-body context, polling option, arbitrary-header public API, duplicate transport/graph/executor, cache, watcher, generation token, checker, allowlist, or language-specific config owner | Complete cumulative diff and caller trace | Static cumulative review | PASS |

## Prior-finding closure

| Finding | Candidate closure |
|---|---|
| `FIND-TASK-003-1` | CLOSED. The shared Workflow stores the loading client; Python stores that complete owner through loading, mutation, and run. The recorded journey distinguishes the loading client from hostile and absent ambient configuration. |
| `FIND-TASK-003-2` | CLOSED. `post_native` sends once, starts refresh immediately after a known `401`, does not replay, and gives renewal failure precedence over body collection. Recorded complete/truncated/pending-body cases correspond to the source. |
| `FIND-TASK-003-3` | CLOSED. Recognized codes use catalog title/remediation; provider-controlled body messages and arbitrary details are discarded. |
| `FIND-TASK-003-4` | CLOSED. The plaintext file helper is private; public cross-module resolution returns `SecretString`. |
| `FIND-TASK-003-5` | CLOSED. The cited function-local imports remain at module/test-module scope. |
| `FIND-TASK-003-6` | CLOSED. Changed declarations use imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | CLOSED. Previously identified panic-capable helpers/tests retain substantive `# Panics` contracts. |
| `FIND-TASK-003-8` | CLOSED. Remote create/cancel, native POST, public caller, and shared local run operations state applicable cancellation and post-dispatch progress. |
| `FIND-TASK-003-9` | CLOSED. Source cancellation races the in-flight call without replay; recorded proof cancels only after dispatch is observed. |
| `FIND-TASK-003-10` | CLOSED. `load_local_setup` derives both consumers from one `GlobalConfig`, and the R5 test now traverses the real client-less mixed-route Workflow path. The exact selector/count and retained-client Python rerun are recorded. |
| `FIND-TASK-003-11` | CLOSED. `AuthMiddleware::force_refresh` distinguishes replay-safe transports from the native send-once consumer. |
| `FIND-TASK-003-12` | CLOSED. `Workflow::into_skald` documents loss of retained loading-client and automatic dependency-composition context. |
| `FIND-TASK-003-13` | CLOSED. The active design authority records encoding, limits, authentication order, pre-dispatch refusal, consumption/non-forwarding, and absent-header behavior. |
| `FIND-TASK-003-14` | CLOSED. `Workflow::run`, `run_with`, `SelectedRoutes::dependencies`, and `resolve_binding` now accurately describe ambient reads, drop behavior, completed reads, and possible post-dispatch progress. |
| `FIND-TASK-003-15` | CLOSED. The hand-authored and assembled Python declarations describe Native, retained/ambient public gateway, and selected external-secret behavior consistently with the PyO3 owner. |

## Review findings

No proposed invariant finding. I found no reachable producer-to-sink violation, regression, or unsupported novel mechanism in the cumulative candidate.

## Open questions

None.

## Verification notes

- No command was run in this review, as required by the strict read-only instruction.
- The candidate records successful focused proofs for remote lifecycle, per-call gateway context/errors, post-dispatch cancellation, native `401` renewal including pending/truncated bodies, authenticated fallback ingress, served OpenAPI, shared mixed-route run-start composition, and Python retained-client behavior.
- It also records the relevant shared/Rust SDK, gateway, server, codegen, client-tier, SDK-tier, PyO3, Python, TypeScript, formatting, lint, and diff checks. The cited tests and assertions exist in candidate source and exercise the stated seams.
- Real accepted server-run lifecycle and full compiled CLI/SDK route journeys remain explicitly assigned to dependent TASK-004/TASK-005, not omitted TASK-003 obligations.
