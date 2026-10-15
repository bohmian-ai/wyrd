# TASK-001 `httpAuth` / `http_auth` on the client, and `wyrd auth access-token`

Spec: `changes/active/ambient-auth/spec.md` r3. Branch `split/ambient-auth`,
rebased on `origin/main` 5bcd9e487. The branch's code commit implements r1;
this task rewrites it to r3.

## TypeScript (`sdks/wyrd-sdk-ts/wyrd/src/index.ts`)

- On `class WyrdClient` (line ~1482), add `get httpAuth(): typeof fetch`. It
  returns a `fetch` that copies the request headers, deletes `authorization`,
  `x-api-key`, and `x-goog-api-key`, sets
  `x-wyrd-access-token: Bearer ${await this.accessToken()}`, and calls the
  global `fetch`.
- Delete `accessTokenSource` (~3150) and the standalone `wyrdFetch` (~3175)
  and their exports.
- `src/otel.ts` already uses `client.accessToken()`; leave it.

## Python

- New `sdks/wyrd-sdk-python/python/wyrd/client/_http_auth.py`: class
  `HttpAuth(httpx.Auth, httpx2.Auth)` holding the client. `sync_auth_flow`
  and `async_auth_flow` remove the three headers and set
  `X-Wyrd-Access-Token`; the async flow gets the token with
  `await asyncio.to_thread(client.access_token)`. Import `httpx` and `httpx2`
  inside the module and keep today's `ImportError` message naming the
  `wyrd[gateway]` extra (rename the extra only if trivial).
- `WyrdClient` is a frozen native pyclass (`sdks/wyrd-sdk-python/src/client.rs:20`).
  Add a `#[getter] fn http_auth` that imports `wyrd.client._http_auth`,
  calls `HttpAuth(self)`, and returns it. This keeps it a method on every
  client, including one returned by `on_behalf_of`.
- Update `client/__init__.pyi` / the generated stub through codegen, never by
  hand.
- Delete only `python/wyrd/gateway/_auth.py` and every import, lazy export,
  and stub declaration of `GatewayAuth`. Retain `wyrd.gateway`, `Gateway`,
  and all existing public administration types and operations. Update the
  stub source and regenerate the public stub; retain both gateway stub files.

## CLI (`crates/wyrd/wyrd-cli/src/auth/`)

- Add `AccessToken` to `AuthCommand` (`auth/mod.rs:22`), a new
  `auth/access_token.rs` that builds the client the way the other commands do
  and prints `client.access_token().await?` plus a newline, nothing else.

## Tests

Rewrite or replace every test using the removed names (find with
`grep -rln 'GatewayAuth\|wyrdFetch\|accessTokenSource' sdks docs`):

- TS: `tests/unit/gateway.test.ts`, `tests/integration/gateway-inference.test.ts`,
  `tests/support/local-development.ts`, `tests/support/server.ts`, and add the
  MCP journeys to `tests/integration/mcp-reference-client.test.ts` (main
  replaced `mcp-framework-compatibility.test.ts` with it).
- Python: `tests/unit/gateway/test_gateway_auth.py` (move to
  `tests/unit/client/`), `tests/integration/gateway/test_gateway_openai_client.py`,
  `tests/integration/test_local_development.py`.
- Cover AC-001 to AC-005. The revoked-key journey asserts the refusal comes
  from the token exchange; do not pin the status code (SPEC-journey-defects
  REQ-001 changes it to 401).
- CLI journey for `wyrd auth access-token` beside the existing auth CLI
  journeys.
- Cover AC-008 with the existing gateway public-export tests, typed-contract
  fixtures, and administration journeys; preserve their administration coverage.

## Docs and example

- Docs: replace every mention of the removed names; the framework guide shows
  the transport for OpenAI, Anthropic, Gemini, Pydantic AI, and MCP, plus one
  line on the key-slot alternative.
- `wyrd-example-ts/src/agent.ts` (separate repository): one
  `const wyrd = WyrdClient.connect()`, then
  `new OpenAI({ baseURL, apiKey: "wyrd", fetch: wyrd.httpAuth })`.

## Evidence

- `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run ts:napi:check`, `mise run ts:test:unit`,
  `mise run ts:test:integration`, `mise run py:format`, `mise run py:lints`,
  `mise run py:typecheck`, `mise run py:test:unit`,
  `mise run py:test:integration`, `mise run test:cli:journey`,
  `mise run docs:check`.
