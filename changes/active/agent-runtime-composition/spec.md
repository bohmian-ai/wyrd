---
id: SPEC-agent-runtime-composition
revision: 2
status: draft
---

# Agent runtime composition

## Objective and user value

A Wyrd Agent is one runtime: a Prompt, tools and a run configuration, running
one lifecycle. Where it runs should change only how its model calls and tools
are wired, not whether it can have them, and not which code path it takes.

| | Model calls | Tools |
|---|---|---|
| **Client** (SDK `Agent`) | Through the tenant gateway, using the Agent's `WyrdClient` | Local `@tool` functions, plus Wyrd's server tools reached over Wyrd MCP with the same `WyrdClient` |
| **Server** (server-hosted Agent) | Through the gateway in process | Server built-ins bound in process |

Today:

- **Runtime:**
  - Skald has two entry points with drifted copies of the same setup:
    `run_with` (user text, session, `before_agent`) and `run_prompt`
    (template variables, no session).
  - `run_with` always validates the request as a conversation, so a Jev
    `system_one` Prompt never reaches its single-call path. The live sweep
    recorded this as JEV-1.
  - The single-call path in `run_prompt` skips the model callbacks,
    `after_agent` and the zero-iteration check. `run_prompt` itself skips
    `before_agent`.
- **Client:**
  - The SDK `Agent` cannot hold a `WyrdClient`.
  - A `wyrd_gateway` route is refused before any IO.
  - No MCP tool source exists.
  - The only SDK path that attaches a gateway caller is `Run.invoke`, which
    refuses any Agent with tools.
  - The Rust and TypeScript SDKs expose no Agent runtime at all.
- **Server:** built-in tools exist, but they are bound only inside Verifier
  step execution. This spec keeps that, and see D5.

`wyrd-design.md` already says an Agent's tool names resolve through "host
tools + MCP server registrations". This change makes the implementation match
that design.

The target shape matches the installed agent SDKs we reviewed (OpenAI Agents
0.23.1, Pydantic AI 2.56.0, Google ADK 2.12.0):

- the Agent is configuration only;
- one loop, with run variants that differ only in sync, async or streaming;
- one run input;
- the gateway is a model provider;
- MCP is a toolset beside function tools.

User value:

- An ML engineer runs the same Agent locally and through their governed
  gateway, with tools, from Rust, Python or TypeScript.
- Every model call and every server tool call carries the caller's identity,
  authorization and audit, because both go through existing server surfaces.
- One tool name means the same contract on the client and the server.
- Every Prompt format, Jev included, runs through the same Agent and the
  same callbacks.

## Requirements

### Runtime (Skald)

- **REQ-001: One run entry point.**
  - An Agent runs through one `run(input)`. The run input carries:
    - optional user text or messages;
    - template variables;
    - an optional session or history;
    - per-run overrides: the execution route, and a Prompt override.
  - `run_with` and `run_prompt` are folded into it, with no alias.
  - All current callers use it:
    - Verifier step Agents;
    - the simulator;
    - delegation;
    - the Python `Agent`.
  - Language variants differ only in their execution model (Python `run`
    and `run_sync`), never in the input they accept.
- **REQ-002: One lifecycle.**
  - Every run goes through the same prepare, dispatch and finish steps.
    Prepare covers the route registry, span, journal and timeout.
  - `before_agent`, the model callbacks and `after_agent` run on every
    dispatch.
  - The zero-iteration check applies on every dispatch.
- **REQ-003: Single-call dispatch is explicit.**
  - Dispatch is chosen once, from the Prompt format:
    - a single call for `system_one`;
    - the conversation loop for every other format.
  - A single-call Agent refuses, before any model call and with a stable
    error, if it is given:
    - tools;
    - a session or history;
    - appended user text.
  - Conversation-request validation reports its real reason, such as an
    unsupported dialect, rather than a "tool-loop dispatch" message.

### Client

- **REQ-004: An Agent can hold a client.** It follows the shared handle
  convention (§Client model):
  - Rust: `connect()` and `connect_with(&client)`, both eager.
  - TypeScript: `static async connect({ client? })`, eager.
  - Python: `client=None` on an IO-free constructor, which connects once on
    the first operation that needs the server.
- **REQ-005: A gateway route works from the client, with correlation.**
  - An Agent on a `wyrd_gateway` route sends every model call to
    `<deployment>/<Prompt model>` through the tenant gateway, as its client.
    Ordinary gateway authorization, capture and audit apply.
  - The refusal "no gateway caller is configured" is removed.
  - A run may name a subject: a Run, and optionally an Agent Card. When it
    does, every gateway call in that run records the subject. This is the
    correlation `Run.invoke` provides today.
  - A `native` route is unchanged.
- **REQ-006: Wyrd MCP is a client toolset.**
  - An Agent accepts Wyrd's server tools as a toolset, for example
    `toolsets=[WyrdMcp(client)]`. The tools are discovered from the
    server's `/mcp` catalog and authenticated as the Agent's client.
  - Each call is an ordinary MCP tool call, so the server applies that
    tool's permissions and audit.
  - The tools available are exactly those in the caller's catalog: read
    tools always, write tools by scope.
- **REQ-007: Local and MCP tools compose.**
  - Local tools and toolsets resolve into one registry and run in one loop.
  - A duplicate is refused before any model or tool call, never silently
    shadowed. Names are compared both as catalog names and as the names the
    provider sees after conversion.
- **REQ-008: Declared server tool names resolve on the client.**
  - An Agent with a client resolves every declared tool name it does not
    serve locally through Wyrd MCP, before any model or tool call (D1b).
  - A name the client cannot serve fails before any model or tool call, with
    a stable error naming the tool. Examples are `fs.*` and `code.run`, which
    need a server workspace.

### Server

- **REQ-009: One contract per tool name.**
  - A built-in that also appears in the MCP catalog (`cards.*`, `bifrost.*`)
    keeps one name, one description and one argument schema across both
    surfaces, as `HostedTools` already does for Cards and Bifrost.

### Cross-surface

- **REQ-010: A full Agent runtime in all three languages.**
  - Rust, Python and TypeScript each expose REQ-001 and REQ-004 to REQ-008
    by projecting `wyrd-client`, including:
    - Prompt construction and loading;
    - local tools and toolsets;
    - run results;
    - stable errors.
  - No language implements its own gateway transport, MCP client or loop.
- **REQ-011: `Run.invoke` is replaced, then deleted.**
  - `Run.invoke` and `Run.invoke_on` are removed from `wyrd-client` and
    from all three SDKs, with no compatibility alias.
  - Current callers move to the SDK `Agent` first:
    - the support-desk examples (Rust, Python, TypeScript);
    - the gateway-inference journeys;
    - the local-development journeys.
  - The deletion happens in this change, only once the replacement journeys
    pass with correlation preserved (REQ-005).
  - Hydrated state is unchanged: `state.agent(...)` keeps returning an
    `AgentCard`. No conversion from `AgentCard` to `Agent` is added.

## Invariants

- **INV-001:** Durable behavior stays on the server: gateway authorization,
  capture, accounting, tool permissions and audit. The client only routes
  calls to existing server surfaces.
- **INV-002:** A client Agent never receives a provider credential. Gateway
  calls carry only the caller's Wyrd token.
- **INV-003:** Tool permissions are never inferred:
  - from `gateway:invoke`;
  - from the Agent declaration;
  - from holding a client;
  - from a tool appearing in a catalog.
- **INV-004:** Tool permission models differ by who acts, and stay that way.
  - A server-hosted Agent acts on a caller's behalf, so a built-in needs
    `tool:invoke` and then the domain permission.
  - A client MCP call is the caller acting directly, so it needs only the
    domain permission.
- **INV-005:** Server-controlled execution still refuses a `native` route.
- **INV-006:** Skald stays Wyrd-agnostic. It sees only `AgentTool` and
  `WyrdGatewayCaller`.
- **INV-007:** It works on macOS and Linux, on x86_64 and arm64, with no
  platform-specific mechanism.

## Scope and non-goals

In scope:

- the unified Skald run entry point and lifecycle, including Jev parity;
- the SDK `Agent` with a client and correlation;
- the client gateway route;
- Wyrd MCP as a toolset;
- a full Agent runtime in Rust and TypeScript;
- the `Run.invoke` migration and deletion;
- journeys in all three languages.

Out of scope:

- **A public "run this Agent on the server" API.** Wyrd verifies agents and
  is not a general Agent runtime. Server-hosted Agents stay inside server
  workflows, which today means Verifiers.
- **Moving the server's built-in tool binder out of `verification/`.** It
  stays until a second server consumer exists (D5).
- **Third-party MCP servers**, including `Mcp` Card registrations, stdio and
  SSE transports. Only Wyrd's own `/mcp` over Streamable HTTP is in scope.
- **Exposing `fs.*` or `code.run` over MCP.** They need a server workspace.
- **Streaming run variants.** Add them when a caller needs one; they must
  reuse REQ-001's input.
- Tool approval and per-tool blocks. These belong on `Policy`.
- Vertex Prompts on a client gateway route. The existing refusal stays.

## Decisions that are expensive to reverse

- **Skald public API:** one run input type replaces two public entry
  points. Every Skald consumer migrates.
- **Ownership:**
  - The authenticated Streamable HTTP MCP client moves from `wyrd-mcp` into
    `wyrd-client` as `wyrd_client::mcp`, beside `bifrost`. The Wyrd MCP
    toolset lives there too.
  - `wyrd-cli` consumes it from `wyrd-client`. This removes the cycle an
    adapter in `wyrd-mcp` would create, since `wyrd-mcp` depends on
    `wyrd-client`.
  - The client tier gains `rmcp`, an MCP client dependency rather than a
    server-tier one.
  - The connected Agent and the gateway caller (`PublicWyrdGatewayCaller`)
    live in `wyrd-client`.
  - `vala-eval` keeps its `AgentToolset` injection point.
- **Public SDK surface:**
  - the `Agent` client parameter and the Rust and TypeScript `connect`
    forms;
  - the run subject used for correlation;
  - the Wyrd MCP toolset;
  - new stable errors for a declared tool the client cannot serve, a
    duplicate tool name, and a refused single-call input.
- **Tool naming:** client MCP tools use the server's catalog names unchanged,
  so a tool name means the same thing on both sides.

## Resolved decisions

- **D1. How MCP tools attach. Resolved: (a) and (b).**
  - (a) Explicit composition as a toolset (REQ-006).
  - (b) Implicit resolution of declared tool names (REQ-008). This is
    specific to Wyrd, because only Wyrd has registered Agent Cards.
- **D2. When a client is resolved. Resolved: the shared handle convention.**
  - Eager `connect` in Rust and TypeScript, and lazy `client=None` in
    Python (REQ-004).
  - The Agent introduces no Agent-specific client resolution.
- **D3. The fate of `Run.invoke`. Resolved: replace, then delete in this
  change (REQ-011).** No `AgentCard`-to-`Agent` conversion replaces it.
- **D4. Correlation. Resolved: included (REQ-005).**
  - Revision 1 deferred it. Deferring would make deleting `Run.invoke` a
    regression.
  - `PublicWyrdGatewayCaller::with_subject` already provides the
    mechanism.
- **D5. Server tool binder. Resolved: unchanged.**
  - Revision 1 moved it (the old REQ-006). Without a second consumer, moving
    it is speculative.
  - The move also risked dropping code budgets, resource guards and result
    limits.
- **D6. Single-call dispatch. Resolved: one lifecycle with explicit
  rules (REQ-002, REQ-003).**
  - Letting Jev through the conversation validator is rejected: message
    rebuilding has no Jev implementation.
  - A Jev branch inside the conversation entry point is rejected.

Signal that would reopen REQ-001: if Verifier steps need semantics that one
run input with a Prompt override can't express. Then the fallback is two thin
entry points over the shared lifecycle, never two lifecycles.

## Acceptance criteria and evidence

- **AC-001** (REQ-001, REQ-002, REQ-003):
  - `run_with` and `run_prompt` no longer exist.
  - The Verifier building-block journeys (`test_verifier_building_blocks.py`
    and the Rust and TypeScript equivalents) pass unchanged.
  - The live sweep's native Jev `system_one` case passes through the SDK
    `Agent`.
  - Unit tests show:
    - callbacks fire on single-call dispatch;
    - a single-call Agent with tools, a session or user text is refused
      before any model call.
- **AC-002** (REQ-004, REQ-005, REQ-010): In each of Rust, Python and
  TypeScript, a journey against a real server and a mock upstream runs an
  SDK `Agent` on `wyrd_gateway`.
  - The gateway capture records the caller, and the Run and Agent Card when
    the run names a subject.
  - An under-privileged caller gets the gateway's own
    `WYRD_PERMISSION_403_DENIED_RBAC`.
- **AC-003** (REQ-006, REQ-007, INV-004): A journey shows an SDK `Agent`
  that:
  - calls `cards.get_by_ref` and `bifrost.query` over Wyrd MCP and a local
    tool in one loop;
  - fails a tool call cleanly when a caller lacks the tool's domain
    permission, with the server's code;
  - is refused before any model or tool call when names collide, including
    a local `cards_list` against MCP `cards.list`.
- **AC-004** (REQ-008):
  - An SDK `Agent` with a client that declares `cards.list` runs it over
    Wyrd MCP.
  - One that declares `code.run` is refused before any model or tool call,
    with the new stable error.
- **AC-005** (REQ-011):
  - `Run.invoke` no longer exists in `wyrd-client`, in any SDK stub or in
    any declaration.
  - The migrated examples and journeys pass on the SDK `Agent`.
  - Support-desk still records Run and Card correlation on its gateway
    calls.
- **AC-006** (live): Part 3 of the provider sweep (`TASK-005-S1`), for each
  provider with a key, runs portable and native Prompts, local tools and
  Wyrd MCP tools through the gateway. It adds:
  - expected-rejection cases for the settings a model refuses;
  - closed, nested, nullable and `$ref` tool schemas, and the real built-in
    descriptors;
  - Gemini thought-signature replay across tool turns.
- **Gates:**
  - `mise run check:deps`:
    - Skald gains no Wyrd dependency;
    - client crates gain no server dependency;
    - `wyrd-client` gains only the `rmcp` client.
  - `mise run codegen:check` for any wire or error-catalog change.
  - `py:typecheck` for the stubs.

## Authority

- `architecture/wyrd-design.md`:
  - §Agent ("host tools + MCP server registrations"; route semantics);
  - §Verifier "Agents, tools, and workspaces";
  - §Client model.
- `architecture/wyrd-doctrine.mdx`: tools are Skald runtime names, not
  Cards.
- `AGENTS.md` §2:
  - MCP is first-class;
  - the client/server split;
  - Skald and Vala dependency direction;
  - Rust, Python and TypeScript are first-class.
- Reviewed external precedent, from installed sources:
  - `openai-agents` 0.23.1: `Agent`, `Runner.run`, `RunConfig.model_provider`
    and `mcp_servers`;
  - `pydantic-ai-slim` 2.56.0: `Agent.run` and `iter`, `gateway_provider`
    and `MCPToolset`;
  - `google-adk` 2.12.0: `LlmAgent`, `Runner.run_async` and `McpToolset`.

## Revision history

| Rev | Status | Change |
|---|---|---|
| 2 | draft | From the independent review and the external SDK review. Changes: one Skald run entry point and lifecycle with explicit single-call dispatch (REQ-001 to REQ-003); eager `connect` in Rust and TypeScript (REQ-004); correlation included (D4); MCP client ownership moved to `wyrd_client::mcp` to avoid a cycle; collisions checked on provider-visible names; the permission difference written as INV-004; server binder move dropped (D5); full Rust and TypeScript runtime (REQ-010); `Run.invoke` deletion gated on replacement journeys (REQ-011); "before IO" changed to "before any model or tool call"; sweep hardening added to AC-006. |
| 1 | draft | Correlation deferred; D4 dropped as a reversible implementation choice. No open decisions. |
| 1 | draft | D3 resolved: `Run.invoke` deleted, no AgentCard-to-Agent conversion; REQ-005 reworded to the SDK Agent; D5 opened. |
| 1 | draft | D2 resolved: the Agent follows the shared connect, connect_with and lazy-Python handle convention. |
| 1 | draft | D1 resolved: explicit and implicit MCP tool attachment. |
| 1 | draft | Initial draft from the TASK-005-S1 review, which found that Agent tools and gateway routing are composed only inside Verifier execution. |
