---
id: SPEC-ambient-auth
revision: 2
status: approved
---

# Stock clients authenticate with the Wyrd access token

## Objective and user value

A stock provider SDK (OpenAI, Anthropic, Gemini, and frameworks built on
them such as the OpenAI Agents SDK and Pydantic AI) and a stock MCP client
reach the Wyrd gateway and `/mcp` with the short-lived Wyrd access token the
client already exchanges and renews. The developer learns two names, the same
in TypeScript and Python, and writes no adapter per framework.

## Approval

Approved by the human owner on 2026-10-14. r2 replaces r1 (REQ-008 of
SPEC-example-journey-fixes r6) by owner direction: one self-evident name per
concept, methods on the client, Wyrd's own header, and no per-framework
helpers. The long-lived API key fallback for clients with no hook is
SPEC-gateway-api-key-fallback.

## Decisions

- D1 The credential primitive is the client's current access token, already
  owned by `wyrd_client` in Rust: `WyrdClient.accessToken()` (TypeScript) and
  `WyrdClient.access_token()` (Python). It needs no new code.
- D2 The transport is a method on the client with one name:
  `WyrdClient.httpAuth` (TypeScript, a `fetch`) and `WyrdClient.http_auth`
  (Python, an `httpx.Auth` and `httpx2.Auth`, sync and async).
- D3 The transport is the documented path for every provider SDK and every
  MCP client in both languages. Passing `accessToken` to an SDK's callable key
  slot remains valid and is documented in one line.
- D4 Wyrd's header is `X-Wyrd-Access-Token: Bearer <token>`. The gateway's
  rule of exactly one credential header is unchanged.
- D6 `wyrd auth access-token` prints the current access token, for
  `apiKeyHelper`-style tools and shells.

## Required behavior

### REQ-001 The transport

On every request, `httpAuth` / `http_auth`:

- removes `Authorization`, `x-api-key`, and `x-goog-api-key`, which on a
  client wired to Wyrd can only hold a placeholder provider key;
- sets `X-Wyrd-Access-Token: Bearer <token>` from the client's
  `accessToken()` / `access_token()` for that request, so a long-lived
  process keeps working after any one token expires.

Python's async flow never blocks the event loop on token renewal.

### REQ-002 One name per concept, no standalone helpers

`accessTokenSource`, the standalone `wyrdFetch`, the standalone
`GatewayAuth`, and the `wyrd.gateway` Python module are removed with no
alias. Nothing has shipped.

### REQ-003 Credential resolution

Every credential source the client chain supports works through these
methods: `WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, and the saved
login. An unresolvable credential fails with the existing client error
(`WYRD_CLIENT_401_NO_CREDENTIALS`), not an opaque provider error.

### REQ-004 CLI

`wyrd auth access-token` resolves the client like every other CLI command
and prints only the token followed by a newline.

## Invariants

- INV-001 Tenant isolation holds on every changed path.
- INV-002 Access tokens, API keys, and provider keys are never logged.
- INV-003 No durable behavior in an SDK: the token, its cache, and its
  renewal stay in `wyrd_client`.

## Acceptance obligations

- AC-001 TypeScript journeys through `WyrdTestServer`, each using only
  `httpAuth`: stock OpenAI client, stock Anthropic client, the official MCP
  SDK against `/mcp`, and the Agents SDK `MCPServerStreamableHttp`.
- AC-002 Python journeys, each using only `http_auth`: `OpenAI`,
  `AsyncOpenAI`, `Anthropic`, and an `httpx` call to `/mcp`.
- AC-003 A request that carries a placeholder provider key and uses the
  transport succeeds (the placeholder header is removed).
- AC-004 A revoked API key is refused by the client's token exchange, before
  the gateway.
- AC-005 A CLI journey: `wyrd auth access-token` prints a token the gateway
  accepts.
- AC-006 `wyrd-example-ts` builds one `WyrdClient` and uses `httpAuth` for its
  OpenAI client and its MCP connection.
- AC-007 Format, lints, codegen check, and the owning `mise` lanes pass.

## Open material decisions

None.
