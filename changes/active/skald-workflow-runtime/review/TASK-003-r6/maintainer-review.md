# TASK-003-r6 maintainer review

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/references/languages/maintainer-style.md`,
  `architecture/references/languages/spec-driven-development.md`, the approved
  Revision 12 specification, TASK-003, TASK-003-R1 through TASK-003-R5, and the
  applicable Wyrd design/doctrine sections.

The candidate remained the stated commit throughout this review. This was a
strictly source-only maintainer audit. I did not build, compile, run tests,
invoke Cargo, mise, pnpm, pytest, formatting, linting, code generation, or any
verification lane. Recorded evidence was treated as evidence to compare with
source, not rerun.

## Changed-surface coverage

| Surface | Symbols and callers inspected | Maintainer assessment |
|---|---|---|
| Shared Workflow owner | `workflow/mod.rs` `Workflow::{from_path,run,run_with,as_skald,as_skald_mut,into_skald}`, `load_local_setup`, `WorkflowCards::load`; callers in the Python wrapper, Cards view, and focused source tests | Cohesive dependency-owning facade. It retains the loading client without duplicating the Skald graph, parser, validator, or executor. Run-start IO is on the documented blocking boundary and the R5 rustdoc now matches the conditional configuration and partial-progress behavior. |
| Local dependency assembly | `workflow/local.rs` `SelectedRoutes::{of,dependencies}`, `resolve_binding`; consumption by `Workflow::run_with` and Skald `WorkflowExecutionDependencies` | Route selection, selected-only secret resolution, and dependency assembly remain with one clear owner. Documentation states sequencing, failure, cancellation, and no-dispatch behavior. No extra cache, watcher, generation token, option, or harness was added. |
| Remote and public-gateway handles | `workflow/remote.rs` `Workflows`; `workflow/gateway.rs` `PublicWyrdGatewayCaller`, `Ingress`, `NativeCall`; Rust SDK projection and transport tests | The handles are concrete, state-owning types with discoverable operations. They reuse `WyrdClient`, native DTOs, protocol codecs, and the existing transport/auth owners instead of adding another client or public arbitrary-header API. Names and return types expose the operation directly. |
| HTTP/auth/config | `HttpTransport::post_native`, `AuthMiddleware::force_refresh`, `GlobalConfig`, `LocalWorkflowConfig`, and `ClientConfig` consumers | The send-once native operation is located beside the shared HTTP owner and documents the distinct renewal/no-replay contract. The Workflow configuration field has active consumers and does not add a speculative knob. One run-start snapshot feeds both mixed-route consumers. |
| Shared secret reader | `wyrd_utils::secret::{read_secret_ref,read_secret_file}` and consumers in the client, gateway credential resolver, and server key loading | The public boundary returns `SecretString`; the plain file helper is private. Three real consumers justify the shared owner, and no parallel reader abstraction remains in the changed paths. |
| Skald route/runtime seams | `ExecutionPlan` route resolution, `WorkflowExecutionDependencies::resolve_route`, `StepRoute::attempt_registry`, `WyrdGatewayProvider`, `ExternalGatewayProvider`, and Workflow executor call sites | Immutable per-attempt route state is carried by the existing runtime owners. The trait-object caller is justified by the established injectable gateway boundary and real implementations. Route adapters do not acquire registry, tenancy, or transport ownership. |
| Typed fallback contract | `GatewayFallbackOverride::{validate,validate_for,to_header_value,from_header_value}` and exported constants | Encoding and validation stay on the invariant-bearing DTO, with typed inputs and stable errors. The mechanism uses the approved standard JCS/base64url contract; it is not accompanied by a bespoke checker, setting, compatibility route, or second schema. |
| Server ingress and gateway | `GatewayIngress`, `requested_fallback`, Anthropic/Gemini handlers, OpenAI route assembly, `GatewayCallRequest` construction, credential resolution, and relevant PG/OpenAPI source tests | Authentication, header consumption, typed lowering, and provider non-forwarding remain at the existing ingress/invocation owners. No new ingress or duplicate governed-call path was introduced. Operation docs and served-OpenAPI annotations name the same contract. |
| Architecture | `architecture/wyrd-design.md:413-443` compared with the typed codec and ingress | The active authority now records encoding, bounds, authentication order, refusal, consumption, and absent-header behavior in the existing Workflow section. It adds no new architecture file or permanent check. |
| Rust SDK projection | `sdks/wyrd-sdk-rust/src/lib.rs` root projection | Thin re-export only; no language-specific transport or Python feature activation. |
| Python runtime and declarations | `PyWorkflow` ownership/mutations/load/run, Cards handoff, hand-authored `stubs/agent.pyi`, assembled `agent/__init__.pyi`, and the retained-client journey source | The shared `ClientWorkflow` survives loading and mutation, so Python does not duplicate dependency assembly. Signatures and return types match, and the two `.pyi` declarations agree. One runtime-doc mismatch remains below. |
| Test layout and readability | Inline Workflow tests, external shared-client HTTP transport test, server PG test, OpenAPI contract test, and Python integration journey | Tests live in appropriate owners: pure/client behavior inline, real HTTP/server and cross-boundary behavior external. The long selected-dependency case is a single scenario matrix with explicit sections and reuses established fixtures; line count alone does not justify a new harness. |

## Material finding

### MNT-R6-001 — Python runtime documentation retains the obsolete local-only headline

- **Changed location:**
  `sdks/wyrd-sdk-python/src/workflow.rs:537-544`, compared with
  `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi:716-725` and
  `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:717-726`.
- **Governing rule:** Maintainer Style, “Python and TypeScript: document the
  typed contract,” requires the public declaration and documentation to
  describe the same operation; `AGENTS.md` requires materially modified Rust
  documentation to explain the operation accurately.
- **Evidence:** The PyO3 method documentation still begins, “Run this workflow
  against the process-local provider registry.” The method actually delegates
  to `wyrd_client::Workflow::run`, which conditionally uses the native registry,
  a retained or ambient authenticated Wyrd client, and selected external
  bindings. The following paragraph partially corrects the headline, while the
  shipped source and assembled stubs use the accurate route-selected wording,
  “Run this workflow, preparing only what its step routes select.” Thus Python
  runtime introspection and the public declarations present different summaries
  for the same method.
- **Concrete maintenance cost:** A maintainer or user reading `help(Workflow.run)`
  can conclude that the method is process-local even though selected routes
  perform authenticated or external network IO. The contradiction also leaves
  two public descriptions to keep synchronized after the R5 contract repair.
- **Smallest testable correction:** Change the PyO3 `run` opening sentence to
  the same route-selected description used by the hand-authored source stub,
  and state there that selected external secrets are read at run start. Keep the
  method, runtime behavior, signatures, stubs, and existing documentation
  pipeline unchanged. Confirm source/runtime-doc/stub parity through the
  repository's existing Python documentation/type-generation checks; add no
  new test harness, checker, setting, or option.

## Uncertain preferences

None. In particular, the size of
`selected_local_dependencies_use_shared_config` is not independently material:
its sections exercise one shared route-configuration contract and splitting it
would add fixture/lifecycle complexity without improving the required proof.

## Drift assessment

No additional DRIFT mechanism was found. The candidate extends established
owners and standard mechanisms: a concrete shared-client handle, the existing
HTTP/auth transport, standard JCS plus unpadded base64url for the approved typed
header, Tokio's existing blocking boundary, the shared configuration file, and
the existing Python stub assembly. It adds no unsupported checker, file class,
setting, retry option, compatibility path, transport, parser, graph, executor,
or language-specific configuration owner. The finding above requires only
correction of an already-existing public doc source.

## Overall result

**FAIL**

The implementation is otherwise maintainable and repository-native, but the
public Python runtime documentation and shipped declarations do not yet state
one consistent execution contract.
