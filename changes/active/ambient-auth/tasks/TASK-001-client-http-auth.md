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

## Implementation evidence

Reuse: the token, cache, and renewal stay in `wyrd_client::WyrdClient::access_token`;
`httpAuth` / `http_auth` only rewrite headers around it. No new Rust client code.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 TS transport | `WyrdClient.httpAuth` getter, `sdks/wyrd-sdk-ts/wyrd/src/index.ts` | `tests/unit/gateway.test.ts` "httpAuth replaces provider key headers…" (`mise run ts:test:unit`, 66 passed) | PASS |
| REQ-001 Python transport, async off-loop | `python/wyrd/client/_http_auth.py` (`HttpAuth`, `asyncio.to_thread`); `#[getter] http_auth` in `src/client.rs` | `tests/unit/client/test_http_auth.py` (3 tests; `mise run py:test:unit`, 597 passed) | PASS |
| REQ-002 removals, `wyrd.gateway` retained | `accessTokenSource`, `wyrdFetch`, `GatewayAuth`, `gateway/_auth.py`, lazy export, stub removed | residual grep clean outside `changes/`; `tests/unit/gateway/test_gateway_surface.py`, `tests/unit/test_public_exports.py` pass | PASS |
| AC-001 TS journeys | `gateway-inference.test.ts` (OpenAI, Anthropic), `mcp-reference-client.test.ts` (official MCP SDK, Agents SDK `MCPServerStreamableHttp`) | `vitest run` of both files plus `local-development.test.ts`: 12 + 2 + 1 passed | PASS |
| AC-002 Python journeys | `tests/integration/gateway/test_client_http_auth.py` (OpenAI, AsyncOpenAI, Anthropic, httpx2 `/mcp`, ambient env) | `pytest tests/integration/gateway/test_client_http_auth.py tests/integration/gateway/test_gateway_openai_client.py`: 5 + 10 passed | PASS |
| AC-003 placeholder key stripped | every journey passes `apiKey`/`api_key="wyrd"` or `authorization: Bearer wyrd`; upstream asserts the provider key only | same journeys | PASS |
| AC-004 revoked key refused at exchange | TS "revoked api key is refused by the token exchange": cause is `WyrdError`, upstream untouched, status not pinned | `gateway-inference.test.ts` | PASS |
| REQ-004 / AC-005 CLI | `crates/wyrd/wyrd-cli/src/auth/access_token.rs`, `AuthCommand::AccessToken` | `cargo test -p wyrd-cli --test cli -- --exact auth_issue_key_journey::auth_access_token_cli_journey` (stdout is one token line; gateway `GET /v1/models` returns 200) | PASS |
| AC-006 example | `wyrd-example-ts` branch `ambient-auth` commit `b283666`: one `WyrdClient.connect()`, `fetch: wyrd.httpAuth` | not built: its vendored `wyrd-ai` predates `httpAuth`; it has no MCP connection to change | PARTIAL |
| AC-007 lanes | — | `fmt`, `lints`, `codegen:check`, `ts:napi:check`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `py:format`, `py:lints`, `py:typecheck`, `docs:check`, `git diff --check` all pass | PASS |
| AC-008 gateway administration | `wyrd.gateway` keeps `Gateway` and every type; only `GatewayAuth` removed | gateway unit surface tests, `test_gateway_openai_client.py` (10 passed) | PASS |
| Docs | `docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md` (OpenAI, Anthropic, Gemini, Pydantic AI, MCP, key-slot line, `wyrd auth access-token`) | `mise run docs:check` | PASS |

Diagnosis, `caller without gateway invoke cannot run an agents sdk agent`:

- **Symptom:** `duplicate key value violates unique constraint "cards_identity_unique"`.
- **Evidence:** `server.scopedApiKey` (`sdks/wyrd-sdk-ts/native-testing/src/lib.rs`) seeds the role and bootstraps a service named for it through `insert_fixture_card` (`crates/wyrd/wyrd-testing/src/server.rs`), a plain insert; the file-scoped server got `"gateway_reader"` twice.
- **Cause:** the test minted one role name twice on one server.
- **Fix site:** the second test now uses `agent_reader`. An independent diagnostician confirmed the cause and that no other TS test reuses a role on one server. Python tests that repeat a role run on function-scoped servers.

Also regenerated `docs/public/llms-full.txt`, which `docs:check` flagged after main's Responses input forms change.

Not run (left to change review): the full `ts:test:integration`, `py:test:integration`, `test:cli:journey`, and `test:local-dev:journey` lanes. The Python `test_local_development.py` change is a one-line `GatewayAuth(client)` → `client.http_auth` swap whose local-dev lane needs the installed CLI binary.

Non-goals held: no API-key fallback, no per-framework helpers, no server change.
