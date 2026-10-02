# OIDC and Security Domain Review

## Review result

**FAIL** for candidate `6578921d8ced6316c850e4d8f16bd101630a7056`
against base `35a53faa2`.

The screened transport, OIDC library integration, state binding, tenant/platform
separation, and workload-verifier separation are substantially sound. The
candidate nevertheless has two material trust-boundary defects and does not
provide the required end-to-end proof for platform-administrator login.

## Boundary and authority coverage

This review covered the full base-to-candidate diff and traced the materially
changed trust boundary through:

- `crates/shared/wyrd-auth-oidc/src/relying_party.rs` and its
  `ScreenedHttp` dependency;
- tenant login, callback, connection-test, and platform-login consumers in
  `crates/wyrd/wyrd-auth`;
- server boot, platform identity administration, and public auth routes;
- identity and platform-administration tests; and
- the retained workload JWT-bearer path using `ExternalVerifier`.

The controlling repository authorities were the approved revision-11 spec,
TASK-009, `research/auth-standards-recommendation.md` section 3.1/T1,
AGENTS.md, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, and
`architecture/wyrd-security-posture.md`. The routed TASK-004 round-2 direction
and `FIND-TASK-004-13` were applied. Protocol review used OIDC Core sections
3.1 and 3.1.3.7, OIDC Discovery section 4, RFC 7636 sections 4.3--4.6,
RFC 6749 section 2.3.1, RFC 9207 section 2.4, and RFC 8725's algorithm and
key-selection guidance.

The standing human direction is controlling here: an option or mechanism not
required by those standards or established comparable implementations is
DRIFT, and remediation must remove it rather than add another Wyrd-specific
control.

| Trust boundary | Result |
|---|---|
| Discovery issuer equality and discovery/JWKS destination trust | PASS: discovery is library-owned and exact issuer matching is retained; every provider request goes through the screened adapter. |
| Provider HTTP | PASS: all resolved addresses are screened, DNS is pinned, production is HTTPS-only, responses are capped at 1 MiB, proxies are disabled, and redirects are disabled. The routed 307/308 secret-replay finding is closed and the target-zero-hits test is present. |
| State, nonce, and PKCE | PASS: the library generates state/nonce and S256 PKCE; server-owned state stores the exact issuer, connection, redirect URI, verifier, and nonce and is consumed once before provider IO. |
| Token endpoint authentication | PASS: `client_secret_basic`, `client_secret_post`, and public clients use the library's standard modes; unsupported `private_key_jwt` fails closed. |
| ID-token verification | PASS except `OIDC-SEC-001`: signature, issuer, algorithm, time, nonce, audience, `azp`, subject, and one forced rediscovery for an unknown key are covered. |
| Tenant/connection binding and RFC 9207 | PASS: callback lookup starts from the state digest, re-resolves the exact bound connection, and enforces the response issuer according to advertised support without falling back to another tenant or the platform connection. |
| Platform/tenant separation | PASS in source: platform login uses the operator pool, platform connection, platform principals, and platform sessions; no tenant authority is inferred. End-to-end proof is missing (`OIDC-SEC-003`). |
| Secrets and redaction | FAIL: public responses are redacted, but a provider-controlled token error can enter server logs (`OIDC-SEC-002`). |
| Workload/human verifier separation | PASS: the human OIDC path no longer uses the handwritten `ExternalVerifier`; the retained verifier remains scoped to workload JWT bearer authentication. |

## Security Audit

### Critical

None.

### High

None.

### Medium

#### OIDC-SEC-001 — DRIFT — platform ID-token audience is a second operator-entered value

- **Obligation:** Revision-11 REQ-004 and OIDC Core section 3.1.3.7 require a
  human ID token to name the relying party's registered `client_id`. The human
  direction forbids an extra Wyrd-only audience option.
- **Evidence:** `crates/wyrd-spec/src/auth/platform_identity.rs:41-52` exposes
  both `expected_audience` and `client_id`. Platform redemption then sends the
  OAuth request as `client_id` but verifies the ID token against the separately
  configured `expected_audience` at
  `crates/wyrd/wyrd-auth/src/platform_login.rs:264-271`.
- **Consequence:** A valid, ordinary provider configuration can be made to fail
  solely because the duplicate values differ. More importantly, the relying
  party can be configured to accept an ID token addressed to another client at
  the same issuer rather than requiring its own client identifier. Signature,
  nonce, and issuer checks reduce exploitability, but they do not make the
  wrong audience trust rule conformant.
- **Required correction:** Remove the separate audience from the human
  `CodeRedemption` contract and always build the ID-token verifier from
  `client_id`. Remove the platform request/view/persistence
  `expected_audience` option rather than adding a consistency toggle or a new
  check. Prove that `aud = client_id` succeeds and any other audience fails.

#### OIDC-SEC-002 — VIOLATION — provider-controlled OAuth error text is persisted in logs

- **Obligation:** Revision-11 REQ-005 and the repository security posture
  require provider secrets to be absent from logs, traces, and errors. Remote
  response text is untrusted input.
- **Evidence:** `token_error` converts every non-transport token failure into
  `TokenRejected(error_chain(&error))` at
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:614-626`, and
  `error_chain` retains every displayed source at lines 640-649. In oauth2
  5.0.0, `StandardErrorResponse`'s `Display` includes the provider-supplied
  `error_description` and `error_uri`. `relying_party_error` writes that full
  value with `%error` at `crates/wyrd/wyrd-auth/src/error.rs:69-73`.
- **Exploit path and impact:** A malicious or compromised token endpoint sees
  the client's Basic/body secret and PKCE verifier. It can reflect either in
  `error_description`; Wyrd then stores it in its server logs, expanding a
  provider compromise into durable credential disclosure to log processors
  and log readers. The one-MiB response bound limits size, not disclosure.
- **Required correction:** Classify a standards error by a closed local reason
  and discard provider descriptions, URIs, and response bytes before creating
  a loggable error. Do not add a new redaction framework or configurable
  allowlist. Add a focused test whose token endpoint returns canary secret text
  in `error_description` and assert that neither the public error nor captured
  tracing output contains it.

#### OIDC-SEC-003 — MISSING — the platform OIDC callback is bypassed by the claimed journey

- **Obligation:** TASK-009 makes platform-administrator login a direct
  consumer of the new relying party, and AGENTS.md requires a user journey for
  new user-facing behavior. This proof must cross the actual client/server and
  provider boundary; an in-process issuance helper is not a substitute.
- **Evidence:** The platform e2e test configures the connection but obtains the
  human session with `federated_platform_session` at
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1491-1502`.
  `crates/wyrd/wyrd-testing/src/server.rs:2685-2704` explicitly says that the
  helper stands in for the provider round trip and directly calls the session
  owner after reading the pre-registration. No candidate test drives
  `POST /auth/platform/login`, provider authorization/token exchange, and the
  platform callback through `PlatformLogin::complete` as one journey.
- **Consequence:** The recorded `test:identity:journey` result does not prove
  that platform code exchange, client authentication, nonce/audience checks,
  verified-email pinning, public error projection, or platform-only authority
  works through the shipped route. It also cannot detect OIDC-SEC-001.
- **Required correction:** Extend the existing identity/platform journey lane,
  without a new bespoke check or setting, to complete a standard platform
  authorization-code login through the public routes and provider. Assert the
  resulting session has platform authority and no tenant authority, and cover
  the wrong-audience refusal in that same standard path.

### Low / Defense In Depth

None. No additional nonstandard hardening mechanism is required.

### Positive Controls

- `ScreenedHttp` centralizes scheme screening, all-address refusal, DNS
  pinning, redirect refusal, proxy refusal, timeouts, and bounded reads for
  discovery, JWKS, and token requests.
- The token-endpoint 307/308 regression test verifies that the redirect target
  receives zero requests, closing routed `FIND-TASK-004-13`.
- The relying party uses the vetted library for discovery, S256 PKCE,
  state/nonce generation, token exchange, and ID-token validation; the oauth2
  crate's own reqwest feature is not enabled.
- Unknown-key handling performs one, and only one, forced rediscovery before
  failing closed.
- Tenant callbacks consume state once, retain exact connection/issuer binding,
  and enforce RFC 9207 without cross-tenant or platform fallback.
- Platform and tenant session owners remain separate, and workload bearer
  verification remains separate from human OIDC verification.

## Verification assessment and limits

The implementation report records the required targeted and broad lanes as
green at `b97fa1c46`. Commit `9ef532660` subsequently regenerated only
`Cargo.lock`'s workspace-hack dependency list and
`crates/shared/workspace-hack/Cargo.toml` for the already-selected additive
crypto feature union; it changed no production or test source and no resolved
package versions. Rerunning `mise run lints` after that regeneration is
sufficient for this narrow delta because the task runs workspace Clippy with
`--all-features --all-targets` and therefore compiles the regenerated feature
union, while `mise run check:workspace-hack` proves the generated Hakari state
is current. Repeating every behavioral lane solely because of that generated
feature-list change is not required.

This was a static, immutable-candidate audit; the recorded commands were not
rerun by this reviewer. That is not the cause of the FAIL. The material
verification defect is OIDC-SEC-003: the reported green identity lane does not
exercise the platform login/callback path it is cited as proving. No required
reviewer was unavailable, so the result is not BLOCKED.
