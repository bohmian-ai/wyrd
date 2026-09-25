# Security Domain Review

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The candidate matched the stated commit before and after this review.

## Reviewed boundary

This review traced the cumulative base-to-candidate security boundary from the
verified tenant caller through `identity_connections:write`, canonical audit,
recovery-key verification, provider discovery and candidate probes, screened
outbound HTTP, callback session issuance, and refresh rotation. It reviewed the
original task and the R1 corrections rather than accepting their implementation
evidence as conclusions.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Bearer-derived tenant and dedicated administration permission | `AGENTS.md` §§2, 9; security posture identity/authorization; TASK-001 packet-local contract | `components/admin/identity.rs`; `Permission::identity_connections_write`; built-in roles; `HumanConnections` tenant transactions | PASS |
| Authorization and canonical audit before provider IO and at the stamp | REQ-017; agent audit rules; TASK-001/R1 | `identity.rs:60-98,195-260`; `connections.rs:388-423,614-630`; focused stamp-decision tests | PASS |
| Recovery-key boundary | TASK-001 activation contract; security posture API-key lifecycle | `connections.rs:426-476,903-936`; shared `verify_api_key`; current-role resolution | PASS |
| Callback and client-auth qualification | REQ-004; R1 `FIND-TASK-001-1`/`-2` | `connections.rs:698-893,1012-1165`; `callback::authorization_code_request` | **FAIL** (`SEC-R2-001`) |
| TLS, SSRF, DNS pinning, proxy exclusion, and network bounds | INV-004; agent SSRF rules; security posture “Source credentials and SSRF defense” | `screening.rs`; `provider.rs:93-157`; `jwks.rs:129-180`; all discovery/JWKS/authorization/token callers in `connections.rs`, `login.rs`, and `callback.rs` | **FAIL** (`SEC-R2-002`, `SEC-R2-003`) |
| Active-revision cutoff for initial issuance and refresh | REQ-016; R1 `FIND-TASK-001-5` | login-state and refresh-token bindings; `callback.rs:68-253`; `issuance.rs:453-528`; `refresh.rs:82-230`; SQL queries and cutoff journey source | PASS |
| Input and secret handling | REQ-004/005; security posture secret rules | `ConnectionInput::from_json`/`validate`; redacted views; tracing annotations; sealed-secret owner; `Public` secret-presence test | PASS |

## Prior-finding closure

| Prior finding | Result | Evidence |
|---|---|---|
| `FIND-TASK-001-1` exact callback qualification | **NOT CLOSED** | Origin, path, state, and individual response fields are checked, but a response containing both `code` and `error` still qualifies; see `SEC-R2-001`. |
| `FIND-TASK-001-2` only `invalid_grant` proves client auth | CLOSED | `client_auth_outcome` accepts only a 4xx JSON `invalid_grant`; success, malformed data, other errors, and authentication refusals fail closed. |
| `FIND-TASK-001-3` configured callback at runtime | CLOSED | `try_initiate_login` obtains the callback from `HumanConnections::require_callback`; request host/scheme no longer construct it. |
| `FIND-TASK-001-4` ambient proxy pinning bypass | CLOSED | The shared screened builder calls `.no_proxy()`, and the child-process regression test proves ambient proxy variables do not receive the request. |
| `FIND-TASK-001-5` old-connection callback/refresh cutoff | CLOSED | Login state and refresh rows carry the exact connection id/revision; initial issuance and successor rotation take the shared slot lock and require that revision to remain Active. |
| `FIND-TASK-001-7` real second stamp decision | CLOSED | Candidate testing evaluates and records the pre-network decision, then evaluates `identity.oidc.candidate.tested` and appends that decision transactionally with the stamp. |
| `FIND-TASK-001-13` client-secret presence contract | CLOSED | `Public` rejects every `Some`, including empty, while secret methods require nonempty `Some`. |

## Material proposed findings

### SEC-R2-001 — INCORRECT: an ambiguous callback response still qualifies the candidate

- **Violated obligation:** R1 `FIND-TASK-001-1` requires only an exact state-matching callback response to qualify and says every ambiguous response remains untested. REQ-004 and TASK-001 require callback validation before activation.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:836-864`; proof gap at `connections.rs:1048-1101`.
- **Evidence:** `callback_redirect_qualifies` computes `code_is_valid || error_is_valid`. It does not require exclusivity or reject the opposite parameter. Therefore `?code=abc&error=login_required&state=s1` qualifies, as does a valid single `code` accompanied by duplicate or unknown `error` parameters. OAuth/OIDC authorization responses select a success response or an error response; carrying both is ambiguous. The focused test covers duplicate state and each response arm separately but has no mixed `code`/`error` case.
- **Observable consequence:** A nonconformant or intermediary-generated ambiguous response can stamp and activate a candidate even though it did not return the exact standard authorization response that the remediation makes the qualification boundary. This can retire a working connection for a provider whose real callback behavior is invalid.
- **Required testable correction:** Keep the existing helper and require exactly one response arm: one nonempty `code` and no `error`, or one recognized `error` and no `code`; reject duplicates and every mixed success/error response. Extend the focused table with mixed and duplicate-opposite cases that must not qualify.

### SEC-R2-002 — VIOLATION: discovery can downgrade provider endpoints to cleartext HTTP

- **Violated obligation:** INV-004 requires TLS to remain fail closed. The security posture requires adapter-approved schemes and mandatory TLS verification before every tenant-supplied provider fetch. REQ-005 prohibits provider-secret exposure.
- **Location:** `crates/shared/wyrd-auth-oidc/src/provider.rs:44-78`; `crates/shared/wyrd-auth-oidc/src/screening.rs:97-128`; reachable secret-bearing requests at `crates/wyrd/wyrd-auth/src/connections.rs:756-792` and `crates/wyrd/wyrd-auth/src/callback.rs:311-368`.
- **Evidence:** The external `IssuerUrl` requires HTTPS, but `parse_raw_metadata` accepts `authorization_endpoint`, `token_endpoint`, and `jwks_uri` as arbitrary `url::Url` values. `ScreenedHttp::client_for` screens addresses and disables redirects/proxies but never checks the scheme. A valid HTTPS discovery document can therefore name `http://public.example/...`; candidate testing and real callback exchange send `SecretBasic` or `SecretPost` material to it over cleartext.
- **Plausible exploit and consequence:** A compromised or misconfigured provider publishes an HTTP token endpoint, or an on-path attacker observes that cleartext hop. Wyrd sends the stored provider client secret and authorization code without TLS, allowing credential theft and login-code interception for the tenant even though the configured issuer itself was HTTPS.
- **Required testable correction:** Enforce HTTPS for every production provider endpoint before any request is built, at the shared screened/provider boundary so discovery, JWKS, authorization, candidate probing, and real token exchange cannot diverge. Preserve the existing test-only/local-provider mechanism rather than adding a compatibility path. Add a focused discovery/probe case whose HTTPS issuer advertises an HTTP endpoint and prove no request is sent and the candidate remains untested.

### SEC-R2-003 — VIOLATION: provider-controlled responses are buffered without body or decompression bounds

- **Violated obligation:** The security posture requires response, body-size, decompression, and total-operation bounds for every tenant-supplied URL. TASK-001 requires screened provider network behavior, and tenant administrators are semi-untrusted relative to shared service availability.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:787-792`; shared discovery at `crates/shared/wyrd-auth-oidc/src/provider.rs:115-135`; shared JWKS at `crates/shared/wyrd-auth-oidc/src/jwks.rs:157-180`; the reqwest workspace client enables gzip and brotli decompression.
- **Evidence:** `probe_client_auth` calls `response.bytes()` and discovery/JWKS call `response.json()` without checking `Content-Length` or enforcing a streamed decoded-byte ceiling. `ScreenedHttp` supplies a ten-second timeout but no response limit. Chunked or compressed responses can therefore be accumulated to available memory before parsing fails.
- **Plausible exploit and consequence:** An authorized but malicious tenant administrator stages an issuer they control and invokes the test route repeatedly; its discovery, JWKS, or token endpoint streams a large/chunked or decompression-amplified body. Each request makes a serving replica buffer attacker-controlled bytes, enabling memory exhaustion and shared-service denial of service without crossing the SSRF address filter.
- **Required testable correction:** Reuse one shared bounded response-reading mechanism for OIDC JSON/error bodies and enforce a conservative decoded-byte ceiling before deserialization; reject oversized declared, chunked, and compressed bodies while preserving the existing timeouts and cancellation. Add one focused oversized/chunked provider response test at the shared owner and prove candidate testing fails without a stamp or unbounded buffering.

## Security Audit

### Critical

- None.

### High

- `SEC-R2-002`: discovered HTTP endpoints can expose client secrets and authorization codes on a cleartext network hop.
- `SEC-R2-003`: an authorized tenant can drive unbounded provider-body buffering and exhaust a shared replica.

### Medium

- `SEC-R2-001`: mixed success/error authorization responses are accepted despite the fail-closed exact-response contract.

### Low / Defense In Depth

- None.

### Positive Controls

- Every connection administration route derives tenancy from the verified `Caller`, uses the dedicated permission, and enters tenant SQL through `WyrdPostgres`/`TenantConn`.
- Recovery-key verification reuses the constant-cost API-key verifier, binds the embedded tenant, checks key/principal status, and resolves current permissions inside the activation transaction.
- Provider requests now disable ambient proxies, reject any blocked DNS answer, pin allowed addresses, and disable redirects.
- Provider secrets remain redacted in views, errors, tracing, and audit, and `PrivateKeyJwt` is absent from the accepted human-connection contract.
- Initial and refreshed human sessions are transactionally bound to the exact Active connection revision, preserving cross-replica lifecycle cutoff.

## Verification limits

- This was a time-bounded static security review. I ran only
  `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=connections::probe_tests::only_an_exact_state_matching_callback_redirect_qualifies) | test(=connections::probe_tests::only_invalid_grant_proves_client_authentication)'`; both existing tests passed. I did not rerun provider/Postgres journeys or broad gates, so the report relies on source inspection for their claimed results.
- The current tests do not exercise mixed `code`/`error`, discovered cleartext endpoints, or oversized/chunked/compressed provider bodies; these are verification gaps as well as the reachable defects above.
- Controlled Okta and Entra qualification remains outside TASK-001 and was unavailable to this review.

## Overall result

**FAIL**

The R1 proxy, client-auth, configured-callback, decision-audit, input, and
session-cutoff corrections are present, but one callback qualification defect
remains and the cumulative candidate still violates the mandatory TLS and
bounded-provider-response trust boundary.
