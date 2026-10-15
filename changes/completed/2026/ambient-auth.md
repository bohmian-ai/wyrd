# Stock clients authenticate with the Wyrd access token

- Change ID: `SPEC-ambient-auth`, approved revision 3.
- Completed: 2026-10-14.
- Task review: TASK-001 FIX_REQUIRED at `824f0c491` (two findings); remediation TASK-001-R1 re-reviewed PASS at `0cfbf6422`.
- Completion target: `c34c592af`, after integrating main `14eff1e53`.
- Delivery reference: branch `split/ambient-auth`; external example `wyrd-example-ts` branch `ambient-auth` at `000ed1f`.
- Completion authority: explicit owner approval without a final change review. No change-review PASS is claimed.

## Intent and shipped behavior

A stock provider SDK (OpenAI, Anthropic, Gemini, and frameworks built on them)
or a stock MCP client reaches the Wyrd gateway and `/mcp` with the short-lived
access token the Wyrd client already exchanges and renews. Developers learn one
name per language and write no per-framework adapter.

- `WyrdClient.httpAuth` (TypeScript, a `fetch`) and `WyrdClient.http_auth`
  (Python, an `httpx.Auth` that is also an `httpx2.Auth`, sync and async) remove
  `Authorization`, `x-api-key`, and `x-goog-api-key`, then set
  `X-Wyrd-Access-Token: Bearer <token>` from the client's access token, fetched
  for each request. Python's async flow renews in a worker thread.
- `accessTokenSource`, the standalone `wyrdFetch`, and `wyrd.gateway.GatewayAuth`
  are removed with no alias. `wyrd.gateway` keeps `Gateway` and its
  administration types.
- `wyrd auth access-token` resolves the client like every CLI command and prints
  only the token and a newline.
- The draft guide `connect/call-wyrd-from-a-stock-client` covers OpenAI,
  Anthropic, Gemini, Pydantic AI, MCP, the callable-key alternative, and the CLI.
- `wyrd-example-ts` builds one `WyrdClient` and passes its `httpAuth` to both its
  OpenAI gateway client and its MCP investigator.

## Lasting constraints and decisions

- The token, its cache, and its renewal stay in `wyrd_client::WyrdClient::access_token`;
  the SDK transports only rewrite headers. No SDK holds durable auth state.
- Wyrd's credential header is `X-Wyrd-Access-Token`; the gateway's rule of
  exactly one credential header is unchanged. Provider-key headers on a client
  wired to Wyrd hold only placeholders and are always dropped.
- One method per concept on the client, not per-framework helpers. Clients with
  no transport hook pass `accessToken` to a callable key slot; the long-lived
  API-key fallback is `SPEC-gateway-api-key-auth`.
- r3 retained `wyrd.gateway` administration and removed only the auth helper.

## Acceptance evidence and limits

| Obligation | Evidence retained in the implementation |
|---|---|
| REQ-001 transports | TS `tests/unit/gateway.test.ts`; Python `tests/unit/client/test_http_auth.py` (httpx, httpx2, async freshness). |
| REQ-002 removals | Residual search; Python gateway surface and public-export tests. |
| AC-001 TS journeys | `gateway-inference.test.ts` (OpenAI, Anthropic), `mcp-reference-client.test.ts` (official MCP SDK, Agents SDK). |
| AC-002 Python journeys | `tests/integration/gateway/test_client_http_auth.py` (OpenAI, AsyncOpenAI, Anthropic, `/mcp`, ambient env). |
| AC-003 placeholder stripped | Placeholder keys in every journey; upstream asserts only the provider key. |
| AC-004 revoked key | TS journey revokes through `Principals`; the refusal comes from the token exchange and the upstream is untouched. Status not pinned. |
| AC-005 CLI | `wyrd-cli` `auth_access_token_cli_journey` spends the printed token on `GET /v1/models`. |
| AC-006 example | Example typecheck and `npm run investigate` over MCP, against a scratch SDK built from this change plus the unmerged `split/agents-trace-processor`, which the example also uses. |
| AC-007 gates | Format, lints, codegen, docs, TS/Python type and unit lanes after main integration. |
| AC-008 gateway administration | Gateway surface tests and `test_gateway_openai_client.py`. |

The full TypeScript and Python integration lanes, the CLI journey lane, and the
installed-CLI local-development lane were not run as aggregates, at the owner's
direction; only the named journeys ran. After main integrated the authored-stub
change, `HttpAuth` gained method docstrings for `check_stubs.py`, and
`http_auth` moved into the authored `client/__init__.pyi`.

## Current authority, owners and tests

- [Rust client token owner](../../../crates/shared/wyrd-client/src/client.rs), [TypeScript SDK](../../../sdks/wyrd-sdk-ts/wyrd/src/index.ts), [Python transport](../../../sdks/wyrd-sdk-python/python/wyrd/client/_http_auth.py), [Python getter](../../../sdks/wyrd-sdk-python/src/client.rs), [CLI command](../../../crates/wyrd/wyrd-cli/src/auth/access_token.rs).
- [Stock-client guide](../../../docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md).
- [TS gateway journeys](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts), [TS MCP journeys](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-reference-client.test.ts), [Python journeys](../../../sdks/wyrd-sdk-python/tests/integration/gateway/test_client_http_auth.py), [CLI journey](../../../crates/wyrd/wyrd-cli/tests/auth_issue_key_journey.rs).
