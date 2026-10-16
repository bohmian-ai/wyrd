---
id: SPEC-agent-runtime-composition
revision: 1
status: draft
---

# Agent runtime composition

## Objective and user value

A Wyrd Agent is one runtime: a Prompt, tools and a run configuration, running a
bounded model/tool loop. Where it runs should change only how its model calls
and tools are wired, not whether it can have them.

| | Model calls | Tools |
|---|---|---|
| **Client** (SDK `Agent`) | Through the tenant gateway, using the Agent's `WyrdClient` | Local `@tool` functions, plus Wyrd's server tools reached over Wyrd MCP with the same `WyrdClient` |
| **Server** (server-hosted Agent) | Through the gateway in process | Server built-ins bound in process |

Today only one cell of each row works together:

- **Client:**
  - The SDK `Agent` cannot hold a `WyrdClient`.
  - A `wyrd_gateway` route is refused before any IO.
  - No MCP tool source exists.
  - The only client path that attaches a gateway caller is
    `Run.observe.invoke`, which refuses any Agent with tools.
- **Server:**
  - Built-in tools exist, but they are bound only inside Verifier step
    execution.
  - The binding interface takes Verifier step types (step call, step
    workspace, step attempt).
  - Its one implementation lives under the server's verification module.

`wyrd-design.md` already says an Agent's tool names resolve through "host
tools + MCP server registrations". This change makes the implementation match
that design.

User value:

- An ML engineer runs the same registered Agent locally and through their
  governed gateway, with tools, from Rust, Python or TypeScript.
- Every model call and every server tool call carries the caller's identity,
  authorization and audit, because both go through existing server surfaces.
- One tool name means the same contract on the client and the server.

## Requirements

### Client

- **REQ-001: An Agent can hold a client.**
  - The SDK `Agent` accepts a `WyrdClient` using the shared construction
    vocabulary: Rust `connect()` and `connect_with(&client)`, TypeScript
    `connect({ client? })`, Python `client=None`.
  - Construction stays IO-free. The client is resolved, ambient or supplied,
    on the first operation that needs the server.
- **REQ-002: A gateway route works from the client.**
  - An Agent whose route is `wyrd_gateway` sends every model call of its loop
    to `<deployment>/<Prompt model>` through the tenant gateway, as its client.
  - It uses ordinary gateway authorization, capture and audit, the same
    behavior server-hosted execution has today.
  - The refusal "no gateway caller is configured" is removed.
  - A `native` route is unchanged.
- **REQ-003: Wyrd MCP is a client tool source.**
  - An Agent can add Wyrd's server tools to its tool registry. They are
    discovered from the server's `/mcp` catalog and authenticated as the
    Agent's client.
  - Each call is an ordinary MCP tool call, so the server applies that
    tool's permissions and audit.
  - The tools available are exactly the ones the caller's MCP catalog
    exposes: read tools always, write tools by scope.
- **REQ-004: Local and MCP tools compose.**
  - Local `@tool` functions and MCP tools live in one registry and run in
    one loop.
  - Two tools with the same name is a refusal before the first model call,
    never a silent shadow.
- **REQ-005: Declared server tool names resolve on the client.**
  - An SDK `Agent` with a client that declares tool names it does not serve
    locally resolves them through Wyrd MCP before the first model call (D1b).
  - A name that the client cannot serve fails before the first model call,
    with a stable error naming the tool. Examples are `fs.*` and `code.run`,
    which need a server workspace.

### Server

- **REQ-006: Built-in tools are a server Agent capability, not a Verifier
  one.**
  - Binding built-in tools takes only what the tools need:
    - the accepted caller authority;
    - the deadline;
    - an optional workspace;
    - the declared names.
  - Verifier step execution is one consumer of that capability.
  - Observable Verifier behavior does not change: authorization, audit,
    tool-call records, limits and workspace semantics stay the same.
- **REQ-007: One contract per tool name.**
  - A built-in that also appears in the MCP catalog (`cards.*`, `bifrost.*`)
    keeps one name, one description and one argument schema across both
    surfaces, as `HostedTools` already does for Cards and Bifrost.

### Cross-surface

- **REQ-008: All three languages.**
  - Rust, Python and TypeScript expose REQ-001 to REQ-005 by projecting
    `wyrd-client`.
  - No language implements its own gateway transport or MCP client.
- **REQ-009: `Run.invoke` is deleted.**
  - `Run.invoke` and `Run.invoke_on` are removed from `wyrd-client` and from
    all three SDKs (`run.invoke`, `run.observe.invoke`), with no
    compatibility alias.
  - The SDK `Agent` (REQ-001 to REQ-005) is the one way to run an Agent.
  - Hydrated state is unchanged: `state.agent(...)` keeps returning an
    `AgentCard`. No conversion from `AgentCard` to `Agent` is added.
  - Current callers move to the SDK `Agent`:
    - the support-desk examples (Rust, Python, TypeScript);
    - the gateway-inference journeys;
    - the local-development journeys.

## Invariants

- **INV-001:** Durable behavior stays on the server: gateway authorization,
  capture, accounting, tool permissions and audit. The client only routes
  calls to existing server surfaces.
- **INV-002:** A client Agent never receives a provider credential. Gateway
  calls carry only the caller's Wyrd token.
- **INV-003:** Tool permissions are never inferred:
  - from `gateway:invoke`;
  - from the Agent declaration;
  - from holding a client.
- **INV-004:** Server-controlled execution still refuses a `native` route.
- **INV-005:** It works on macOS and Linux, on x86_64 and arm64, with no
  platform-specific mechanism.

## Scope and non-goals

In scope:

- the SDK `Agent` with a client;
- the client gateway route;
- Wyrd MCP as a tool source;
- moving built-in tool binding out of the Verifier;
- deleting `Run.invoke`;
- journeys in all three languages.

Out of scope:

- **A public "run this Agent on the server" API.** Wyrd verifies agents and
  is not a general Agent runtime. Server-hosted Agents stay inside server
  workflows, which today means Verifiers.
- **Third-party MCP servers**, including `Mcp` Card registrations, stdio and
  SSE transports. Only Wyrd's own `/mcp` over the existing Streamable HTTP
  client in `wyrd-mcp` is in scope.
- **Exposing `fs.*` or `code.run` over MCP.** They stay server-only because
  they need a server workspace.
- Tool approval and per-tool blocks. These belong on `Policy`.
- Vertex Prompts on a client gateway route. The existing refusal stays.
- **Deferred:** Run and Agent Card correlation on Agent gateway calls,
  which `Run.invoke` provided. Gateway calls from an Agent record the caller
  only, until a later revision adds correlation.

## Decisions that are expensive to reverse

- **Ownership:**
  - The MCP-to-`AgentTool` adapter lives on the client side of the boundary,
    with `wyrd-mcp` as the existing owner of the Wyrd MCP client.
  - The gateway caller stays `PublicWyrdGatewayCaller` in `wyrd-client`.
  - Skald stays Wyrd-agnostic: it sees only `AgentTool` and
    `WyrdGatewayCaller`.
  - `vala-eval` keeps its injection point for built-ins. The server-side
    binder moves out of `verification/`.
- **Public SDK surface:**
  - the `Agent` client parameter, or the Rust and TypeScript `connect`
    forms;
  - a way to add Wyrd MCP tools;
  - one new stable error for a declared tool the client cannot serve.
- **Tool naming:** client MCP tools use the server's catalog names unchanged,
  so a tool name means the same thing on both sides.

## Resolved decisions

- **D1. How MCP tools attach. Resolved: (a) and (b).**
  - (a) Explicit composition, e.g. `agent.add_tools(WyrdMcp.connect(client))`.
  - (b) An Agent with a client resolves every declared `tool_names` entry it
    cannot serve locally through Wyrd MCP, before the first model call.
    REQ-005 relies on this.
- **D2. When a client is resolved. Resolved: the shared handle convention.**
  - The Agent follows the same construction vocabulary as `Cards`,
    `Bifrost` and every other connected handle (§Client model).
  - Rust: `connect()` for the ambient client, `connect_with(&client)` for a
    supplied one.
  - TypeScript: `static async connect({ client? })`.
  - Python: an IO-free constructor taking `client=None`, which connects once
    on the first operation that needs the server.
  - The Agent introduces no Agent-specific client resolution.
- **D3. The fate of `Run.invoke`. Resolved: delete it (REQ-009).**
  - No `AgentCard`-to-`Agent` conversion replaces it.

## Acceptance criteria and evidence

- **AC-001** (REQ-001, REQ-002, REQ-008): In each of Rust, Python and
  TypeScript, a journey against a real server and a mock upstream runs an
  SDK `Agent` on `wyrd_gateway`. The gateway capture records the call under
  the caller, and an under-privileged caller gets the gateway's own
  `WYRD_PERMISSION_403_DENIED_RBAC`.
- **AC-002** (REQ-003, REQ-004): A journey shows an SDK `Agent` that:
  - calls `cards.get_by_ref` and `bifrost.query` over Wyrd MCP and a local
    `@tool` in one loop;
  - fails a tool call cleanly when a caller lacks the tool's domain
    permission, with the server's code;
  - is refused before the first model call when it registers a duplicate
    name.
- **AC-003** (REQ-005): An SDK `Agent` with a client that declares
  `cards.list` runs it over Wyrd MCP. One that declares `code.run` is refused
  before IO, with the new stable error.
- **AC-004** (REQ-006, REQ-007): The existing Verifier building-block
  journeys (`test_verifier_building_blocks.py` and the Rust and TypeScript
  equivalents) pass unchanged. A server test binds built-ins with no
  Verifier types involved.
- **AC-005** (REQ-009): `Run.invoke` no longer exists in `wyrd-client` or
  any SDK stub or declaration. The migrated examples and journeys pass on
  the SDK `Agent`.
- **AC-006** (live): Part 3 of the provider sweep (`TASK-005-S1`) runs
  portable and native Prompts with Wyrd MCP tools through the gateway for
  each provider with a key.
- **Gates:**
  - `mise run check:deps` proves Skald gains no Wyrd dependency and client
    crates gain no server dependency;
  - `mise run codegen:check` for any wire or error-catalog change;
  - `py:typecheck` for the stubs.

## Authority

- `architecture/wyrd-design.md`:
  - §Agent ("host tools + MCP server registrations"; route semantics);
  - §Verifier "Agents, tools, and workspaces";
  - §Client model.
- `architecture/wyrd-doctrine.mdx`: tools are Skald runtime names, not
  Cards.
- `AGENTS.md` §2: MCP is first-class, the client/server split, and Skald and
  Vala dependency direction.

## Revision history

| Rev | Status | Change |
|---|---|---|
| 1 | draft | Correlation deferred; D4 dropped as a reversible implementation choice. No open decisions. |
| 1 | draft | D3 resolved: `Run.invoke` deleted, no AgentCard-to-Agent conversion; REQ-005 reworded to the SDK Agent; D5 opened. |
| 1 | draft | D2 resolved: the Agent follows the shared connect, connect_with and lazy-Python handle convention. |
| 1 | draft | D1 resolved: explicit and implicit MCP tool attachment. |
| 1 | draft | Initial draft from the TASK-005-S1 review, which found that Agent tools and gateway routing are composed only inside Verifier execution. |
