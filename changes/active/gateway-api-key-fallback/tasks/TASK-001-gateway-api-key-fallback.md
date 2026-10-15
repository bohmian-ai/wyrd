# TASK-001 Exchange a presented API key on gateway ingress

Spec: `changes/active/gateway-api-key-fallback/spec.md` r1. Branch
`change/gateway-api-key-fallback` off `origin/main` 5bcd9e487. Independent of
SPEC-ambient-auth; merge after it so the docs order reads correctly.

## Where

- `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs`
  `authenticate` (~line 112): today it calls `extract_gateway_access_token`,
  then `verify_access_token`. Branch on the extracted value: `wyrd_sk_` goes
  to the API-key path, anything else to `verify_access_token` unchanged.
- The API-key exchange lives behind `api_key_exchange`
  (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:296`) and
  `ExchangeApiKey`. Reuse that grant; do not write a second exchange. Expose
  from the auth component the smallest call that turns a key into an
  `AuthenticatedPrincipal` (exchange, then `verify_access_token` on the
  minted token, so the principal is built exactly as for any token).
- Cache: one small struct owned by the gateway ingress state, a map from
  SHA-256 of the key to `(token, inserted_at)`, 60-second TTL, bounded size.
  `ponytail:` per-replica cache; revocation latency is the TTL.
- Doc-comment `extract_gateway_access_token`: the value is an access token or
  a Wyrd API key, never a provider key.

## Tests

- Unit tests for the branch and the cache expiry in the new code's module.
- Journeys: extend the existing gateway journeys (Rust
  `crates/wyrd/wyrd-testing` gateway journeys, TS
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts`, Python
  `sdks/wyrd-sdk-python/tests/integration/gateway/`) with AC-001 and AC-002.
  The revocation journey needs the TTL configurable in the test server only;
  take it from `AppState` config, default 60 s.

## Docs

- Framework guide section "Fallback: a Wyrd API key" after the access-token
  path.
- `architecture/wyrd-design.md`, "Cross-service delegation": one paragraph
  naming the gateway ingress exception.

## Evidence

- `mise run fmt`, `mise run lints`, `mise run test:gateway:gate`,
  `mise run test:wyrd`, `mise run ts:test:integration`,
  `mise run py:test:integration`, `mise run check:tenant-isolation`,
  `mise run docs:check`.
