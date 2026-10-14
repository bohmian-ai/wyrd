---
id: SPEC-mcp-compat
revision: 2
status: approved
---

# Wyrd MCP works with common MCP clients

## Objective and user value

A developer registers Wyrd's `/mcp` endpoint with a standards-compliant agent
framework, and the framework discovers, converts, and calls Wyrd tools without
Wyrd-specific adapters. This is an interoperability repair of the existing
endpoint, `wyrd mcp install`, and `wyrd mcp proxy`. It adds no new
registration or authorization infrastructure.

Proof covers the official TypeScript MCP SDK as the reference client and three
widely used Python agent frameworks: OpenAI Agents SDK, Pydantic AI, and
Google ADK. It does not claim compatibility with every framework.

## Required behavior

### REQ-003 MCP compatibility

- `tools/list` results satisfy MCP protocol revision 2026-07-28, including
  `ttlMs: 0` and `cacheScope: private`, because the catalog depends on the
  caller's permissions (implemented).
- Every advertised tool, read and write, has an input schema accepted by
  common tool-calling consumers: a top-level `type: object` with no top-level
  `oneOf`, `anyOf`, `allOf`, `enum`, `const`, or `not`, and output schemas
  accepted by the official MCP SDK (implemented;
  `operator_connections.create` now nests its provider union under `request`).
- The official TypeScript MCP SDK connects to `/mcp`, negotiates protocol
  2026-07-28, lists the full catalog for an administrator, and calls a
  structured read tool whose result it validates against the advertised output
  schema.
- Each of the OpenAI Agents SDK (Python, strict schema conversion enabled),
  Pydantic AI, and Google ADK connects to `/mcp` through its own MCP client
  and:
  - converts every tool advertised to the caller, dropping none;
  - invokes `wyrd.guide` through its own tool-invocation path and receives the
    structured guide;
  - for a caller without write permissions, exposes no write tool; and
  - surfaces a refused call to a tool hidden from the caller as a tool error,
    not a client crash or a successful result.
- Each tool name a framework forwards to a model provider matches
  `^[a-zA-Z0-9_-]{1,64}$`.

## Constraints and invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Server-owned authentication, authorization, audit, tenancy, and
  secret handling are unchanged. Advertising a tool grants no permission.
- INV-003 `/mcp` stays stateless; no session affinity between replicas.
- Agent framework packages are test-only dependencies of the Python SDK
  project, isolated from the default development install and the fast Python
  lane. They never become `wyrd-ai` runtime dependencies.

## Non-goals

- MCP resources, prompts, sampling, or workflow execution (see
  SPEC-mcp-investigation).
- A second MCP implementation, authorization path, or SDK-side MCP adapter.
- Older direct MCP protocol revisions.
- Credentials embedded in host configuration.
- Renaming tools. Wyrd tool names are a public contract. If a covered framework
  forwards a name that fails the provider rule, this spec is revised before any
  rename.

## Acceptance obligations

- AC-001 REQ-003 has user-journey tests that boot a real `WyrdTestServer` and
  drive each named client through its own public MCP API: TypeScript for the
  reference client, Python for the three frameworks. No in-process engine
  fixtures and no model credentials; frameworks run with offline or test
  models where a model is required.
- AC-002 The schema-root rule over the full catalog is proved once, by the
  Rust catalog test and discovery journey. Client journeys prove what only a
  real client can prove, and do not restate that rule with hand-written
  checks.
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass,
  including a dedicated lane for the framework journeys.

## Open material decisions

None.

## Revision history

- r1 (2026-10-14): carried over from SPEC-example-journey-fixes r6 REQ-003.
- r2 (2026-10-14): owner review. The TypeScript journey is reduced to the
  official MCP SDK reference client. OpenAI Agents SDK proof moves to Python
  with strict conversion, and Pydantic AI and Google ADK are added.
  Negative-path and provider-name obligations are added, and the non-goals are
  made explicit. Approved by the human owner on 2026-10-14 along with
  direction to plan the remaining work.

## Authority

- `AGENTS.md` §2 (MCP is first-class), §11 (journey tiers)
- `architecture/wyrd-design.md`
- `docs/src/content/docs/reference/mcp-tools.md`
