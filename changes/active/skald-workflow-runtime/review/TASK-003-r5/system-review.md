# TASK-003-r5 system-resilience review

## Review Findings

### Critical

None.

### Important

- **SYS-R5-001 — the required client-less mixed-route run proof was replaced
  with a helper-only construction test**
  (`changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md:128-139,166-178`,
  `crates/shared/wyrd-client/src/workflow/mod.rs:136-149,783-826`).
  **Classification: MISSING (required verification evidence).** R4 requires
  the existing selected-local-dependencies proof to exercise one actual
  `Workflow` selecting both `ext_gateway` and `wyrd_gateway` without a retained
  client, then records an exact focused command for that proof. The new
  `mixed_routes_use_one_config_snapshot` case does not build a Workflow, run
  `SelectedRoutes::of`, enter `Workflow::run_with`, or call
  `load_local_setup`. It passes a pre-parsed `GlobalConfig` and manually chosen
  `(true, true, None)` arguments directly to `local_setup_from`. It therefore
  proves that the new pure helper can derive two values from one supplied
  snapshot, but not that the reachable mixed-route run detects both route
  families and enters that helper through the one-load run-start path. The
  recorded broad five-test command also does not provide the required exact
  selected-local-dependencies result or selected count. Source inspection
  supports the correction, but the explicit recovery-path proof remains
  absent; under the review direction, missing required evidence is a rerun
  finding rather than a residual limit. Extend the existing
  `selected_local_dependencies_use_shared_config` fixture, or revise the new
  case using that fixture, so an actual client-less mixed-route Workflow
  reaches the run-start setup owner and demonstrates both dependencies come
  from its single snapshot. Rerun and record the remediation's exact focused
  selector and selected count. Reuse the existing fixture and test file; add no
  new harness, timing assertion, configuration mechanism, or setting.

### Suggestions

None.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediations: `TASK-003-r1` through `TASK-003-r4` under
  `changes/active/skald-workflow-runtime/review/`

The human-approved `WyrdGatewayCall.model` amendment, the R1 same-spec native
`401` wording correction, and the approval to keep configuration, client, and
selected-secret reads at run start on Tokio's blocking pool were treated as
authority. The complete base-to-candidate range and all four remediation
packets were inspected. The candidate remained the stated commit.

## Deployed-Path Evidence

| Runtime path | Source evidence | Deployed effect |
|---|---|---|
| Shared local Workflow | `crates/shared/wyrd-client/src/workflow/mod.rs:36-175`; `sdks/wyrd-sdk-python/src/workflow.rs`; `sdks/wyrd-sdk-python/src/state/mod.rs` | Rust, Python, TypeScript, and CLI local execution converge on the shared facade. Registered and authored-external Workflows retain the Cards client that loaded them; wholly local Workflows have no retained client. |
| Run-start configuration and client assembly | `workflow/mod.rs:131-149,179-239`; `workflow/local.rs:39-138`; `config.rs:100-168` | Route selection precedes one approved blocking setup task. A client-less mixed route now performs at most one `GlobalConfig::load`; `ClientConfig::from_global_with_env` and `global.workflow` consume that same value. Retained clients bypass ambient client construction, and a native-only run loads no ambient config. |
| Selected external secrets | `workflow/local.rs:83-138`; `wyrd-utils/src/secret.rs` | Only selected external bindings are resolved, at run start, through blocking tasks. Missing or invalid selected material fails before Skald dispatch; unselected secrets are not read. |
| Public Wyrd gateway call | `workflow/gateway.rs:32-125`; `transport/http.rs:351-411` | Each call carries immutable model, fallback, timeout, cancellation, and correlation. The shared authenticated transport sends one native model POST and does not replay ambiguous provider work. |
| Native `401` renewal | `transport/http.rs:398-410`; `auth.rs:503-543` | Renewal begins after the `401` response head and before body collection. Successful renewal affects later calls only; renewal failure wins; pending or truncated bodies cannot prevent the refresh attempt. |
| Public authenticated ingress | `wyrd-server/src/components/gateway/ingress.rs:101-224`; native route handlers in `ingress.rs` and `routes.rs` | Existing protocol authentication remains ahead of handler execution. The bounded fallback header is consumed before governed dispatch and never forwarded to a provider. Invalid overrides refuse only the request. |
| Remote Workflow lifecycle | `workflow/remote.rs:23-121`; shared HTTP retry path | Create reuses one idempotency key within its retrying request; get and cancel return snapshots; wait polls without taking ownership of the server run. TASK-004/005, not this task, own the actual server run service and complete lifecycle journeys. |

## Failure, Recovery, and Availability Assessment

| Failure or recovery path | What stops and what survives | Assessment |
|---|---|---|
| Cancellation before public model dispatch | The biased cancellation branch wins before the send is polled. No gateway request starts; sibling client and server capabilities remain available. | PASS. |
| Cancellation or timeout after dispatch | Local response waiting stops. The already accepted gateway/provider operation is not rolled back or replayed; the failure remains scoped to this call/run. | PASS; source and recorded post-dispatch evidence agree. |
| Native `401` with complete, truncated, or pending body | One renewal is attempted before body collection, and the model POST is not resent. The caller deadline/cancellation still bounds a pending body or renewal. | PASS; source ordering agrees with the recorded R3 cases. |
| Wyrd endpoint, auth exchange, or network outage | The public call returns its bounded timeout/cancellation or redacted provider/transport category. It adds no background retry loop, process failure, or recursive server call. | PASS. |
| Config, credential, or selected-secret read failure | The affected local run fails during run-start preparation before model dispatch. Task-added filesystem work stays off the async polling thread; no durable Workflow state is partially written. | PASS under the explicit human run-start decision. |
| Config replacement during a client-less mixed route | Current source performs one `GlobalConfig::load` and derives both Workflow bindings and client config from its returned value, eliminating the prior two-generation split. | PASS in source; direct required run proof is missing under SYS-R5-001. |
| Retained Cards client plus hostile or absent ambient configuration | `load_local_setup` reuses the retained client and does not assemble an ambient gateway client; external bindings still trigger only their required config read. The recorded Python journey exercises retained endpoint/credential context after ambient mutation/removal. | PASS; source and recorded Python evidence agree. |
| Pure native run | Both setup flags are false, so default in-memory config is used and no client or execution secret is read. | PASS in source. |
| Remote create answer lost | Transport retries inside the same `create` retain the stable idempotency key. A separately invoked `create` remains a new submission by contract. | PASS. |
| Wait/get/cancel interruption | Dropping wait stops polling only. A lost cancel result can be reconciled with get; no client-side cancellation of the server run is inferred. | PASS. |
| Client/server process exit during a public call | Ambiguous model work is not replayed. The candidate adds no queue, WAL, health loop, detached worker, or cross-process recovery claim, and sibling server capabilities are not intentionally crashed. | PASS; requiring such machinery would be DRIFT. |

## Affected Capabilities and Recovery Boundary

The cumulative candidate affects the shared Rust Workflow facade projected to
Rust, Python, TypeScript, and CLI local execution; the Rust/CLI remote-run
handle; and existing public OpenAI Chat/Responses, Anthropic Messages, and
Gemini GenerateContent ingress. Failures remain request- or local-run-scoped.
No new server process, durable queue, audit path, provider-credential owner,
health mechanism, public ingress, or transport was added.

The R4 source correction closes the previous split-configuration recovery
defect at its producer. SYS-R5-001 concerns acceptance evidence for that exact
reachable mixed-route path: the helper-only test does not establish the
caller-to-owner wiring that R4 explicitly required. The smallest closure is to
exercise the existing shared path with an actual mixed Workflow and rerun the
already-prescribed selector, not to introduce new machinery.

## Open Questions

None.

## Verification Notes

- This review was strictly source-only. It ran no build, compile, test, lane,
  Cargo, mise, pnpm, pytest, formatter, linter, test-listing, or other
  verification command.
- The implementer's recorded evidence was inspected but not rerun. It records
  five passing Rust cases from a broad selector, clippy, formatting, Python
  setup, the public Python retained-client integration (`1 passed`), and diff
  checking.
- Current source closes the prior two-snapshot defect: the one conditional
  `GlobalConfig::load` at `workflow/mod.rs:196-200` supplies both
  `global.workflow` and `ClientConfig::from_global_with_env(&global)`.
- The Python retained-client rerun is recorded after the shared `run_with`
  change and directly covers the retained client branch under hostile and then
  absent ambient configuration.
- The R4 record does not contain the mandated exact focused command/result,
  and its new Rust test bypasses `Workflow`, route discovery, `run_with`, and
  `load_local_setup`. This missing direct recovery proof is SYS-R5-001, not a
  generic verification limitation.
- TASK-004 and TASK-005 own accepted server-run implementation, process
  shutdown/drain, affinity, and complete cross-language journeys; their absent
  future evidence is not charged to TASK-003.

## Overall Result

**FAIL**

The cumulative source preserves request-scoped failure isolation, send-once
native model calls, pending-body renewal, selected-secret handling, retained
client authority, and remote-run ownership. The R4 implementation also closes
the split ambient configuration defect in source. Acceptance is still
incomplete because its required client-less mixed-route Workflow proof was not
implemented or recorded; the helper-only test cannot establish the reachable
run-start path.
