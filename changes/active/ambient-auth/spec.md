---
id: SPEC-ambient-auth
revision: 1
status: approved
---

# Stock clients authenticate through the Wyrd credential chain

## Objective and user value

A stock OpenAI client and a stock MCP client reach the gateway and `/mcp` with the access token the Wyrd SDK already exchanges and renews, configured from the environment alone.

## Approval

Approved by the human owner on 2026-10-14 as part of SPEC-example-journey-fixes
r6. Split into its own change by owner direction on 2026-10-14 so each problem
is reviewed and verified on its own. The requirement text is carried over
unchanged.

## Required behavior

### REQ-008 Stock clients authenticate through the standard credential chain (r5)

Owner directive 2026-10-14: "stop TASK-007 and do the accessToken exports".
The server accepts no new credential form. Stock OpenAI clients and stock MCP
clients reach the gateway and `/mcp` with the Wyrd access token the SDK already
exchanges, caches, and renews.

- TypeScript `wyrd-ai` exports `accessTokenSource(options?: ClientOptions)`.
  It resolves the client once (the ambient client when `options.client` is
  omitted) and returns `() => Promise<string>`, the current access token on
  each call. It plugs directly into the OpenAI SDK:
  `new OpenAI({ baseURL, apiKey: accessTokenSource() })`.
- TypeScript `wyrd-ai` exports `wyrdFetch(options?: ClientOptions)`, a `fetch`
  that resolves the client once and sets `X-Wyrd-Access-Token: Bearer <token>`
  on every request. It plugs into any client that takes a `fetch`, including
  the Agents SDK `MCPServerStreamableHttp`.
- `gatewayFetch` is removed with no alias; `accessTokenSource` covers its use.
- Python `GatewayAuth(client=None)` resolves the ambient client when `client`
  is omitted, once, at construction.
- Every credential source the client chain supports works through these
  helpers: `WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, saved login.
- An unresolvable credential fails with the existing client error
  (`WYRD_CLIENT_401_NO_CREDENTIALS`), not an opaque provider error.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Provider credentials and keys are never logged or returned.

## Acceptance obligations

- AC-001 Each REQ has a user-journey test (Rust, Python, and TypeScript where
  the surface ships in all three; MCP/HTTP where agent-facing) that fails on
  the base and passes on the candidate, booting the server the way users do
  where the defect was harness-masked (REQ-003 uses the official TypeScript
  MCP SDK; REQ-007 drives the CLI).
- AC-003 Format, lints, codegen check, and the owning `mise` lanes pass.
- AC-005 (r5) The example constructs no `WyrdClient`: the gateway OpenAI
  client uses `accessTokenSource()` and the MCP connection uses `wyrdFetch()`.
  Journeys prove a stock OpenAI client (TypeScript and Python) and the
  official TypeScript MCP SDK succeed with only the ambient environment, and
  that a revoked API key is refused.

## Open material decisions

None.
