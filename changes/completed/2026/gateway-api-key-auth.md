# Gateway API-key fallback for stock clients

- Change: `gateway-api-key-auth`
- Specification: `SPEC-gateway-api-key-auth`, approved revision 1
- Completed: 2026-10-15
- Reviewed target: `a57d34d85`, approved by the human owner in place of a final change review
- Delivery reference: not supplied

Some tools can be configured only with a base URL and a static key:
`google-genai` against Gemini's own API, environment-only setups, curl, and
languages without a Wyrd SDK. They now reach the Wyrd gateway with
`WYRD_API_KEY` and no Wyrd import. The short-lived access token remains the
preferred path; the API key is the fallback.

## Shipped behavior

- On public gateway ingress only (OpenAI-compatible, Anthropic Messages, and
  Gemini GenerateContent), a credential starting with `wyrd_sk_` is treated as
  an API key. The rule covers any header the route already accepts. Every
  other value is verified as an access token, as before.
- The key is exchanged through the existing API-key grant, the same path as
  `POST /auth/token`. The minted token is then verified like any presented
  access token.
- A failed exchange is refused in the route's own error envelope with the
  exchange's error, for example `WYRD_AUTH_401_API_KEY_INVALID`. It never
  falls through to token verification.
- The exchanged token is reused for up to 60 seconds from the start of its
  exchange. The cache is per process, keyed by the SHA-256 of the full key,
  and bounded at 4,096 entries. Reads never extend an entry, and failures are
  never cached.
- A retired key is refused once its entry expires, so it stops working within
  60 seconds.
- `/mcp`, the `/v1` management routes, and Bifrost still refuse API keys.
- The raw key never reaches provider requests, responses, gateway call rows,
  or spans.

## Invariants and constraints

- A key's tenant comes only from the parsed key. Cached authority is never
  shared across keys or tenants.
- The rule of exactly one credential header is unchanged. Query-string keys,
  wrong native headers, and multiple credential headers are still refused.
- A cold exchange records `auth.token.exchange` exactly once. A cache hit
  records no exchange; gateway authorization still decides and audits every
  call.
- A completion whose 60-second window has already closed serves its own
  request but is never cached. Concurrent misses for one key each exchange
  and are each audited.

## Decisions and deviations

- The design rule that an API key never travels on the wire is narrowed to
  one exception: public gateway ingress.
- The cache lives in `GatewayApiKeys` on `AppState`, built on the installed
  `lru` and `sha2` crates. It is not distributed: with several replicas, each
  replica exchanges a key once.
- Task review found that the reuse window started at cache admission. A
  retirement that committed during a slow exchange could therefore exceed the
  60-second bound. Remediation moved the window's origin to before the
  exchange.
- The spec lists OTLP among access-token-only surfaces, but OTLP already
  accepts its own `x-wyrd-api-key`. That behavior was left unchanged. The
  guide and design text name only `/mcp`, `/v1` management, and Bifrost.

## Acceptance closure

| Obligation | Evidence |
|---|---|
| AC-001 stock clients | Python stock clients (OpenAI buffered and streaming, Anthropic `x-api-key`, Google GenAI `x-goog-api-key`) and TypeScript OpenAI (buffered) configured with only a URL and a key |
| AC-002 negatives | J1: credential retirement with the principal still active; cached reuse within the window; refusal at 61s without dispatch; malformed key refused in all three envelopes; reader denied; tenant isolation; no key leakage. J2: `/mcp`, `/v1` management, and Bifrost query refuse the key |
| AC-003 unit | Credential classification; cache isolation, expiry, and eviction; deadline propagation for delayed and late exchanges |
| AC-004 lanes | `fmt`, `lints`, `check:deps`, `check:tenant-isolation`, Python and TypeScript lints, TypeScript typecheck, `docs:check`, `test:principals:integration` |

## Links

- Design exception: [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md) ("Cross-service delegation")
- Guide: [`docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md`](../../../docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md)
- Owners: [`gateway/api_keys.rs`](../../../crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs), [`gateway/ingress.rs`](../../../crates/wyrd/wyrd-server/src/components/gateway/ingress.rs)
- Journeys: [`wyrd-testing/tests/gateway/native.rs`](../../../crates/wyrd/wyrd-testing/tests/gateway/native.rs), [`test_gateway_openai_client.py`](../../../sdks/wyrd-sdk-python/tests/integration/gateway/test_gateway_openai_client.py), [`test_gateway_native_clients.py`](../../../sdks/wyrd-sdk-python/tests/integration/gateway/test_gateway_native_clients.py), [`gateway-inference.test.ts`](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts)
