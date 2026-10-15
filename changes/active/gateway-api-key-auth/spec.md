---
id: SPEC-gateway-api-key-auth
revision: 1
status: approved
---

# The gateway accepts a Wyrd API key when the client has no token hook

## Objective and user value

A tool that can only be configured with a base URL and a static key (Gemini's
own API through `google-genai`, environment-only setups, curl, any language
without a Wyrd SDK) reaches the Wyrd gateway with `WYRD_API_KEY` and no Wyrd
import:

```bash
export OPENAI_BASE_URL=$WYRD_SERVER_URL/v1
export OPENAI_API_KEY=$WYRD_API_KEY
```

The short-lived access token (SPEC-ambient-auth) stays the happy path. This is
the fallback.

## Approval

Approved by the human owner on 2026-10-14 ("The happy path is using the
primary wyrd auth mechanism (no long-lived keys) with the fallback to
API_KEY"). It narrows the design rule that an API key never travels on the
wire to one exception: public gateway ingress.

## Required behavior

### REQ-001 Gateway ingress exchanges a presented API key

- On the public gateway ingress routes only (OpenAI-compatible, Anthropic
  Messages, Gemini GenerateContent), a credential whose value starts with
  `wyrd_sk_`, presented in any header the route already accepts, is an API
  key. Any other value is verified as an access token, as today.
- The server exchanges the key through the existing API-key grant, the same
  path `POST /auth/token` uses, and authorizes the request as the resulting
  principal. A key that fails the exchange is refused in the route's own
  error envelope with the exchange's error.
- The exchanged token is cached in process for 60 seconds, keyed by a SHA-256
  hash of the key. Revoking a key takes effect within 60 seconds.
- The raw key is never logged, captured in a gateway call row, or returned.
- `/mcp`, `/v1` management routes, OTLP, and Bifrost accept only access
  tokens, unchanged.

### REQ-002 Documented

- The framework guide documents the fallback after the access-token path,
  with the 60-second revocation window and the reason to prefer the access
  token.
- `architecture/wyrd-design.md` "Cross-service delegation" states the gateway
  ingress exception.

## Invariants

- INV-001 Tenant isolation holds: the key's tenant comes only from the parsed
  key, and the cache is keyed by the key hash, never shared across keys.
- INV-002 The exactly-one-credential-header rule is unchanged.
- INV-003 The exchange records its authorization decision as today; a cache
  hit records none.

## Acceptance obligations

- AC-001 Journeys through `WyrdTestServer`, with no Wyrd import in the client:
  stock OpenAI client with `api_key = WYRD_API_KEY` (Python and TypeScript),
  stock Anthropic client with the key in `x-api-key`, and a Gemini
  GenerateContent call with the key in `x-goog-api-key`.
- AC-002 Negative: a revoked key is refused once the cache entry expires; a
  key presented to `/mcp` and to a `/v1` route is refused; a malformed
  `wyrd_sk_` value is refused with the API-key error.
- AC-003 Unit: the access token versus API key decision, and cache expiry.
- AC-004 Format, lints, and the owning `mise` lanes pass.

## Open material decisions

None.
