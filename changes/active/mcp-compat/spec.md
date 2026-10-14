---
id: SPEC-mcp-compat
revision: 3
status: approved
---

# Wyrd MCP works with common MCP clients

## Objective and user value

A developer registers Wyrd's `/mcp` endpoint with a standards-compliant agent
framework, and the framework discovers, converts, and calls Wyrd tools through
its own MCP implementation. Framework configuration and small local
model-name aliases are permitted as described below. This is an
interoperability repair of the existing endpoint, `wyrd mcp install`, and
`wyrd mcp proxy`. It adds no new registration or authorization infrastructure.

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
  schema. Full catalog means equality of tool names with an independently
  requested, unfiltered server catalog for the same administrator.
- Each of the OpenAI Agents SDK (Python, strict schema conversion enabled),
  Pydantic AI, and Google ADK connects to `/mcp` through its own MCP client
  and:
  - converts every tool advertised to the caller, dropping none;
  - invokes `wyrd.guide` through its own tool-invocation path and receives the
    structured guide;
  - for a caller without write permissions, exposes no write tool; and
  - sends an explicitly named hidden-tool call to Wyrd using the caller's
    credentials and surfaces Wyrd's refusal, including its problem code, as a
    framework tool error, not a client crash or a successful result. A local
    undeclared-name rejection alone does not prove this requirement; a
    descriptor obtained through an authorized caller may describe the
    negative call but grants the invoking caller no permission.
- Each tool name a framework forwards to a model provider matches
  `^[a-zA-Z0-9_-]{1,64}$`. These model-facing aliases may differ from Wyrd's
  dotted MCP wire names. Prefer the framework's built-in naming facilities;
  where none exists, a small local override that changes only the model-facing
  name is allowed. The framework's MCP client continues to invoke the original
  wire tool. Tests and documentation must identify the configuration or local
  aliasing required; they must not claim unconfigured native compatibility.

## Constraints and invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Server-owned authentication, authorization, audit, tenancy, and
  secret handling are unchanged. Advertising a tool grants no permission.
- INV-003 `/mcp` stays stateless; no session affinity between replicas.
- INV-004 Model-facing aliases are unique within the caller's discovered
  catalog and preserve the original MCP tool identity, arguments, permissions,
  results, and error handling. Local aliasing owns no transport, session,
  schema conversion, authorization, retry, or durable behavior. It stays in
  framework journey code and documented integration examples; no new public
  Wyrd SDK adapter or alias registry is introduced.
- Agent framework packages are test-only dependencies of the Python SDK
  project, isolated from the default development install and the fast Python
  lane. They never become `wyrd-ai` runtime dependencies.

## Non-goals

- MCP resources, prompts, sampling, or workflow execution (see
  SPEC-mcp-investigation).
- A second MCP implementation, authorization path, or shipped SDK-side MCP
  adapter. The local model-name aliasing permitted by REQ-003 and INV-004 is
  presentation at the framework boundary, not a replacement MCP client.
- Older direct MCP protocol revisions.
- Credentials embedded in host configuration.
- Renaming Wyrd's public MCP wire tools or adding server aliases. Framework
  model-facing aliases are permitted without changing that public contract.

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
- AC-004 Framework journeys prove the complete provider-facing catalog has
  unique valid names and that an aliased guide call reaches the original Wyrd
  tool. The ADK negative journey must additionally prove an actual Wyrd RBAC
  refusal and a successful subsequent read through the same framework
  connection owner. No alias implementation is required when the framework's
  built-in naming configuration already satisfies the contract.

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
- r3 (2026-10-14): owner-directed revision following T01 review at
  `6fbd60f19`, with explicit direction to revise the specification and provide
  a complete remediation packet. Allows bounded local model-name aliasing
  when a framework has no built-in mapping, while preserving dotted Wyrd wire
  names and all server/client ownership boundaries. Requires complete catalog
  equality and actual server-refusal proof, rather than accepting partial
  catalog checks or local unknown-tool errors. This revision resolves the
  adapter-policy conflict in FIND-T01-3; it does not mark any implementation
  finding closed or change the historical r2 review verdict.

## Authority

- `AGENTS.md` §2 (MCP is first-class), §11 (journey tiers)
- `architecture/wyrd-design.md`
- `docs/src/content/docs/reference/mcp-tools.md`
