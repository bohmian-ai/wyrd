# Named deployments, authored Prompts, and Agent execution routes

- Change: `deployment-addressing`
- Specification: `SPEC-deployment-addressing`, approved revision 6
- Completed: 2026-10-15
- Reviewed target: `2bc92a277`. TASK-001 task review returned PASS with an
  empty finding ledger. The human owner approved completion in place of a
  final change review.
- Delivery reference: not supplied

A tenant configures provider credentials and named gateway deployments once.
A Prompt names its provider and model once and carries no connection details.
The Agent that uses the Prompt chooses where it executes. One Prompt can run
natively, through a named Wyrd deployment, or through an external
OpenAI-compatible gateway onboarded as a deployment. Portable and native
Prompt content share one Rust compiler. The Workflow product capability is
removed.

## Shipped behavior

- **Deployments.** A deployment is tenant gateway configuration, not a Card.
  Its name selects one adapter and credential binding, and the name is
  independent of the vendor. Gateway requests address `<deployment>/<model>`,
  split at the first slash. The model suffix reaches the provider unchanged;
  there is no model renaming, alias, or weighted selection. `models` and
  `capabilities` are optional allowlists that only narrow. `access` is
  `open | granted`, default `open`.
- **Onboarding.** `wyrd gateway credential put` stores a sealed credential.
  `wyrd gateway deployment put --file` accepts one object or a nonempty list.
  The whole file is validated before any write, and duplicates or an empty
  list write nothing. Partial failures are reported truthfully and reruns
  converge. `wyrd gateway capture-policy get|put` manages opt-in call capture;
  the default is `disabled`.
- **Authorization.** `gateway:invoke` scopes are `all`, `{deployment}`, and
  `{deployment, model}`. `all` does not reach `granted` deployments. Explicit
  grants survive role merging alongside `all`. A delegated exchange whose exact
  intersection cannot be represented is refused, and no token is issued. The
  requested target and each fallback are authorized before upstream IO.
- **External gateways** are deployments with an `openai_compatible` adapter.
  Destinations are screened and pinned before secrets attach, and Wyrd
  credentials are never forwarded.
- **Prompts** require top-level `provider` and `model` in both forms.
  - The portable form has `system`, `messages` (text, image, and document
    blocks with typed `${media:name}` bindings), eight typed `settings`, and a
    `response_format` JSON Schema.
  - The native form has a short `format` (`chat_completion`, `responses`,
    `messages`, `generate_content`, `system_one`) and a `body` without a model.
  - Authored JSON is stored and hashed as written. Variables, media variables,
    and response metadata are derived from the authored form and never stored.
  - Literal media names never collide with authored media inputs.
- **Agents** require `execution_route`: `{kind: native}` or
  `{kind: wyrd_gateway, deployment}`. Explicit, authorized invocation-time
  overrides leave the stored Agent and its hash unchanged. A loaded Agent
  exports its authored spec and hash unchanged, including omitted
  run-configuration fields and verification bindings.
- **Local Vertex gateway calls** fail before IO; native and server Vertex
  remain.
- **Workflow** is removed across contracts, runtime, server, SDKs, CLI, MCP,
  and UI. Agents, judges, and independent Eval task DAGs keep their behavior.
- **Local development.** `wyrd server dev --tenant acme` starts the server and
  creates the first tenant. The installed-CLI journey configures OpenAI,
  Anthropic, Gemini, and Jev, then runs downloaded Agents through each one
  before and after a restart.

## Invariants

- Tenancy comes from verified identity. TenantConn/RLS ownership is preserved
  across credentials, lookup, caches, authorization, capture, and listing.
- Secrets stay in the existing keyring and binding owners. They never appear on
  Cards, in logs, or in upstream requests.
- There are no compatibility routes, legacy decoders, provider-scope shims,
  weighted selection, or hidden native fallback.
- Permissions block. Audit and capture go through the existing non-blocking
  Scribe outbox.
- Prompt identity is stable across route changes. Fallbacks record each actual
  target without rewriting the authored Prompt or its hash.

## Decisions

- The provider is never inferred from a deployment name. The Prompt provider
  and the deployment adapter are checked for compatibility explicitly.
- The model is sent upstream unchanged, so deployments do not rename models
  (revision 3).
- The portable core is text, image, and document. Audio and video stay native.
  Local Vertex gateway support is deferred (revision 4).
- Deployment put takes one object or a list. Credential upload and header
  injection are unchanged (revision 5).
- An unrepresentable delegation intersection is refused rather than attenuated
  (revision 6).

## Revisions and deviations

- Revisions 1–6 are recorded in the spec. This change supersedes the draft
  `SPEC-prompt-authoring` r1.
- Three remediation rounds (R1–R3) closed FIND-001-1 to FIND-001-17.
  FIND-001-14 (an actual downloaded-Agent journey) and FIND-001-17 (faithful
  loaded-Agent export) were added by the owner's direction.
- The spec names `mise run gate` as the final aggregate. It was not run: the
  owner approved completion without the final change review. Evidence is the
  focused journeys, owner lanes, and static checks the task review inspected.

## Acceptance closure

| Criterion | Evidence |
| --- | --- |
| AC-001–003 deployments, access, external gateways | Gateway journeys for named, slash, same-adapter, restricted, and external targets, credential protection, delegation, and visibility; delegation-refusal tests |
| AC-004 authored Prompt lifecycle | Rust, Python, and TypeScript `prompt_forms_round_trip_as_authored` over four forms, with authored JSON, hash, variables, schema, and nonempty media names |
| AC-005, AC-007, AC-008 Agent execution, judges, native ingress | Native, named, and external Agent journeys; tool, structured, timeout, cancellation, and override cases; judge journeys; native-ingress and Vertex refusal tests |
| AC-006 compilation | `skald-spec` authoring tests, including `portable_literals_never_alias_authored_media`, media validation, and offline schema meta-validation |
| AC-009 surfaces and docs | Architecture Prompt and Agent sections, Connect docs, examples, stubs, and generated and served contracts; `codegen:check`, `docs:check` |
| AC-010 Workflow removal | Removed-kind and catalog tests; surviving Agent, judge, and Eval behavior |
| AC-011–012 local development and deployment files | `test_installed_cli_gateway_setup` with its Rust child `local_development::cli_gateway_agents`, which checks the full spec and hash before and after a restart; deployment-file negative and convergence cases |

## Links

- Architecture: [Wyrd design: Prompt and Agent](../../../architecture/wyrd-design.md#prompt)
- Docs: [Set up the LLM gateway](../../../docs/src/content/docs/connect/set-up-the-llm-gateway.md),
  [Make gateway requests from a framework](../../../docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md)
- Example: [`examples/gateway-setup`](../../../examples/gateway-setup)
- Owners: [`crates/wyrd/wyrd-gateway`](../../../crates/wyrd/wyrd-gateway),
  [`crates/skald/skald-spec`](../../../crates/skald/skald-spec),
  [`crates/skald/skald-agent`](../../../crates/skald/skald-agent),
  [`crates/shared/wyrd-client`](../../../crates/shared/wyrd-client)
- Journeys:
  [installed CLI](../../../sdks/wyrd-sdk-python/tests/integration/test_local_development.py),
  [register and hydrate (TypeScript)](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/register-and-hydrate.test.ts)
