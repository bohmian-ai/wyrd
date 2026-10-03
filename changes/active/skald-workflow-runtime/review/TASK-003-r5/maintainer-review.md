# TASK-003 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation tasks: `TASK-003-R1` through `TASK-003-R4` under the prior review directories
- Human approvals applied: the Revision 12 `WyrdGatewayCall.model` amendment, the corrected native-`401` wording, and run-start `spawn_blocking` for configuration, client, and selected-secret reads

The candidate remained `HEAD` throughout this review. This was a source-only, read-only review. I ran no build, test, lint, code-generation, package-manager, Cargo, or mise command.

## Authority

- `AGENTS.md`, especially the client ownership, struct-centered Rust, async, Python/stub, testing, and completion rules
- `architecture/agent-rules.md`, especially the hard Rust documentation and owner-shape requirements
- `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`, especially the singular shared-client Workflow facade and first-class SDK projection
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/spec-driven-development.md`
- Approved Revision 12 specification, original task, and R1-R4 remediation packets

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Assessment |
|---|---|---|
| Shared local Workflow facade | `workflow/mod.rs::{Workflow, load_local_setup, local_setup_from, WorkflowCards}`; Cards hydration owner; Rust, Python, and TypeScript callers; local unit and SDK journey sources | Owner and method shape are discoverable and reuse the existing Cards/Skald owners. R1 retained-client behavior, R3 blocking boundaries and `into_skald` ownership documentation, and R4 one-snapshot composition are visible in source. One public rustdoc statement contradicts the actual run-start load path (`MNT-001`). |
| Selected local dependencies | `workflow/local.rs::{SelectedRoutes, resolve_binding}`; `GlobalConfig::workflow`; Skald dependency and route owners | Cohesive private owner; selected-binding preparation stays beside the shared Workflow facade. Names and types expose the route-selection intent. |
| Remote Workflow handle | `workflow/remote.rs::Workflows`; shared transport calls; `workflow_transport.rs`; Rust SDK re-export | The handle follows the repository's dependency-owning facade pattern. Method names, argument types, return types, and cancellation/partial-progress docs are clear. |
| Public gateway caller and native transport | `workflow/gateway.rs::{PublicWyrdGatewayCaller, NativeCall, Ingress}`; `HttpTransport::post_native`; `AuthMiddleware::force_refresh`; Skald `WyrdGatewayCall` producer and adapter | The call flow can be followed from Prompt model production through one native POST and protocol normalization. R2/R3 send-once renewal behavior and cancellation notes are documented at the owners. |
| Skald model propagation | `plan.rs::ExecutionPlan::build`; `route.rs::{WorkflowExecutionDependencies::resolve_route, gateway_model, StepRoute, WyrdGatewayProvider}`; route tests | The approved model field is carried through the existing route owner without a parallel runtime. Changed fields and helpers have substantive documentation. |
| Fallback wire contract and ingress | `GatewayFallbackOverride::{to_header_value, from_header_value}`; `requested_fallback`; OpenAI, Anthropic, and Gemini handlers; served OpenAPI test source | Encoding and ingress interpretation stay with the contract and ingress owners. Handler argument and helper documentation expose the affected routes and refusal boundary. |
| Secret reading | `wyrd_utils::secret`; gateway credential resolver; server wrapping-key configuration; local Workflow binding resolver | One shared reader replaces duplicated rules. Callers retain their domain-specific error projection and async/blocking boundary. |
| Language projections and declaration parity | Python `PyWorkflow`, registered-load caller, integration journey, hand-authored source stub and assembled public stub; TypeScript native/public Workflow wrappers; Rust SDK exports | Runtime delegation remains thin and preserves the shared Workflow. The Python public stub was not updated for the newly remote-capable run contract (`MNT-002`). |
| Manifests and exports | Four changed crate manifests, lockfile entries, `wyrd-client`/Rust SDK exports | Dependency moves correspond to production callers; no new feature or duplicate transport owner was introduced. |

## Material findings

### MNT-001 — `Workflow::run_with` documents the wrong ambient configuration trigger

- **Changed location:** `crates/shared/wyrd-client/src/workflow/mod.rs:108-119`
- **Governing rule:** `architecture/agent-rules.md` requires materially modified Rust documentation to explain relevant workflow behavior and side effects; `maintainer-style.md` requires documentation to clarify non-obvious IO and ownership behavior rather than contradict it.
- **Evidence:** The rustdoc says, "Only when a step resolves to an `ext_gateway` route is the shared client configuration loaded." The implementation at lines 136-143 passes both `needs_config` and `needs_gateway` to `load_local_setup`. That owner loads `GlobalConfig` when `needs_config || (needs_gateway && loaded.is_none())` at lines 195-203. A locally authored or otherwise client-less Workflow with only a `wyrd_gateway` step therefore also loads ambient configuration at run start so it can build the gateway client.
- **Concrete maintenance cost:** A maintainer or Rust caller relying on the public method contract will incorrectly conclude that a client-less `wyrd_gateway` run performs no ambient configuration/filesystem read. That is precisely the run-start ownership boundary R3 and R4 made deliberate, so the false statement makes later cancellation, configuration, or secret-loading work unsafe to reason about.
- **Smallest testable correction:** Change only this rustdoc to state the two actual triggers: selected external bindings, or a selected public Wyrd gateway without a retained loading client. Preserve the existing statement that selected secrets are resolved only for selected external bindings and preserve all runtime behavior. Ordinary documentation/lint evidence is sufficient; add no checker or runtime test.

### MNT-002 — Python's public Workflow stub still describes the pre-change local-only run contract

- **Changed location:** `sdks/wyrd-sdk-python/src/workflow.rs:537-562`; stale declaration source at `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-733`, assembled at `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-734`
- **Governing rule:** `AGENTS.md` section 8 requires generated stubs and public Python exports to project Python-visible behavior; `maintainer-style.md` requires public documentation and generated declarations to describe the same operation and treats contradiction with behavior as a maintainer finding.
- **Evidence:** The changed PyO3 owner now stores `ClientWorkflow`, retains the loading client, and delegates `run` to `self.inner.run(input)`. Its source rustdoc explicitly describes public-Wyrd-gateway calls, retained/ambient client configuration, external bindings, and selected-secret reads. The hand-authored public stub still says only "Run this workflow against the process-local provider registry" and "Dependencies order steps only," omitting the newly reachable server call, ambient configuration read, and selected external-secret resolution. `scripts/assemble_stubs.py:189-199` copies that source stub into the public module, so both shipped declaration locations preserve the stale contract. The recorded codegen result cannot establish semantic parity when its source stub itself was not updated.
- **Concrete maintenance cost:** Python maintainers and users reading the shipped type surface cannot discover that `Workflow.run()` may perform authenticated network IO and read selected execution secrets, or that a loaded Workflow keeps its original client. The Rust boundary documentation and public Python declaration now tell materially different stories about the same method.
- **Smallest testable correction:** Update the hand-authored `python/wyrd/stubs/agent.pyi` `Workflow.run` documentation to match the changed PyO3 owner: native routes use the process registry; public gateway routes use the retained or ambient Wyrd client; external routes resolve only selected configured secrets at run start. Regenerate the assembled public stub through the existing stub assembly owner and use the existing codegen/typecheck proof. Do not hand-edit the assembled `agent/__init__.pyi` and add no new generator, check, or documentation mechanism.

## Uncertain preferences

None. I did not elevate file size, helper decomposition, test aggregation, naming alternatives, or other preference-level observations.

## Verification evidence and limits

The task and remediation packets record focused Workflow transport, gateway ingress, OpenAPI, SDK, Python retained-client, formatting, lint, boundary, and code-generation results. Per the standing direction, I did not rerun them. This review judges their claims only where current source supports them. The two findings above are direct source/documentation contradictions and therefore are not closed by the recorded green commands.

## Overall result

**FAIL**

The runtime owners and cross-language delegation are maintainable, and the prior remediation ownership transitions are visible in source. Acceptance is blocked by two public contract-documentation defects: the Rust facade states the wrong ambient-load condition, and the shipped Python declaration remains on the pre-change local-only contract.
