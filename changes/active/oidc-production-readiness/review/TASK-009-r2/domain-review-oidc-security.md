# OIDC and security domain review

## Result

**FAIL** for candidate `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
against base `35a53faa216b10651d85c96ce12e34f382cac637`.

The remediation closes the prior platform callback, audience-configuration,
process-owned cache, and workload-discovery defects. One human ID-token
audience rule remains broader than OpenID Connect permits: Wyrd explicitly
trusts every additional audience and tests that behavior as success.

`FIND-TASK-009-5` remains withdrawn by
`review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`. This review does not
reopen it and proposes no provider-error logging change.

## Boundary, authority, and source coverage

The immutable cumulative diff and the remediation diff were reviewed through
the following reachable boundaries:

- `wyrd-auth-oidc::RelyingParty`: discovery, screened transport, authorization
  request, token exchange, ID-token verification, and unknown-key refresh;
- tenant login, candidate connection testing, and the common callback through
  `HumanConnections` and `AuthorizationCodeExchange`;
- platform begin/callback through the boot-owned `PlatformLogin` and
  `ServerAuth`;
- workload issuer creation and boot discovery, which remain separate from the
  human relying party after remediation;
- platform connection persistence and the migration removing the duplicate
  human audience; and
- the relying-party unit tests, tenant identity journeys, and the served
  platform callback journey recorded by the remediation evidence.

The governing authority was revision 11 of the approved specification,
TASK-009 and TASK-009-R1, `AGENTS.md`, `architecture/agent-rules.md`, the Wyrd
security posture, OpenID Connect Core 1.0 section 3.1.3.7, OpenID Connect
Discovery 1.0 sections 3 and 4.3, RFC 6749 section 2.3.1, RFC 7636 sections
4.3--4.6, RFC 8725 sections 3.1, 3.8, and 3.9, RFC 9207 sections 2.4 and 3,
and the installed `openidconnect 4.0.1` implementation. In particular,
`openidconnect` defaults to rejecting every additional audience as untrusted
and documents that an application must explicitly identify any trusted
additional audience. Wyrd has no such trust configuration.

## Trust-boundary assessment

| Boundary | Evidence | Result |
|---|---|---|
| Discovery issuer binding | `ProviderMetadata::discover_async` and the metadata-only workload read both compare the discovered issuer exactly to the configured issuer. | PASS |
| Provider URL trust | `ScreenedHttp` resolves and screens every address, pins the accepted resolution, disables redirects and proxies, requires the deployment's allowed scheme, caps decoded bodies at 1 MiB, and applies a request timeout. | PASS |
| Authorization request binding | `openidconnect` generates state, nonce, and S256 PKCE. Server-owned, bounded, one-use state records issuer, connection/revision where applicable, redirect URI, verifier, nonce, and initiation. | PASS |
| RFC 9207 pre-token check | Tenant and platform callbacks compare a present `iss` exactly and require it when cached metadata advertises support, before token exchange. The platform wire contract now carries `iss`. | PASS |
| Client authentication | `SecretBasic`, `SecretPost`, and public PKCE clients map to the library's standard token request modes; unsupported `private_key_jwt` fails closed. | PASS |
| ID-token signature, issuer, algorithm, time, nonce, key, and subject | The library verifier is pinned to issuer, advertised asymmetric algorithms, JWKS, client ID, expiry, nonce, and signature; Wyrd adds bounded clock skew, future-`iat`, subject-shape, and `azp` checks. Unknown keys cause one coalesced rediscovery. | PASS |
| ID-token audience | The client ID must be present, but every other audience is declared trusted by `.set_other_audience_verifier_fn(|_| true)`. | **FAIL — OIDC-SEC-R2-001** |
| Tenant/platform separation | Tenant callbacks derive the tenant only from consumed state; platform login uses the operator boundary and issues only a platform session. The served platform journey proves platform authority and refusal at tenant routes. | PASS |
| Workload/human separation | Workload setup performs metadata-only screened discovery and leaves keys and RFC 7523 assertions to `ExternalVerifier`; human login alone uses `RelyingParty`. | PASS |
| Secrets and public errors | Client secrets remain sealed and redacted; code and request payloads are skipped by route instrumentation; public relying-party errors reveal no token, verifier, nonce, or secret. Provider-standard error logging is governed by the explicit withdrawal. | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

#### OIDC-SEC-R2-001 — DRIFT — every additional ID-token audience is trusted

- **Violated obligation:** Revision-11 REQ-007 and TASK-009 require complete
  OIDC Core ID-token audience validation. OIDC Core section 3.1.3.7 step 3
  requires rejection when an ID token contains additional audiences the
  client does not trust. The installed library implements that rule with a
  secure default that rejects every other audience until the caller identifies
  a trusted one. The standing human direction forbids replacing that standard
  behavior with an ungrounded Wyrd mechanism.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:632-646`, especially
  `.set_other_audience_verifier_fn(|_| true)` at line 638; the asserted success
  case is `crates/shared/wyrd-auth-oidc/src/relying_party.rs:1038-1043`.
- **Evidence:** `id_token_verifier` first opts every non-client audience into
  trust. `verify_authorized_party` then checks only that multi-audience tokens
  carry `azp = client_id`; it never establishes that any additional `aud` is a
  trusted recipient. The focused test constructs `aud = [CLIENT, "other"]`
  with `azp = CLIENT` and requires acceptance. The installed
  `openidconnect-4.0.1/src/verification/mod.rs` states that its default rejects
  other audiences because they can impersonate the user when presenting their
  copy of the claims. The authoritative OIDC rule is also explicit in
  [OIDC Core 3.1.3.7](https://openid.net/specs/openid-connect-core-1_0.html#IDTokenValidation).
- **Exploit path and consequence:** If an issuer or an issuer-side extension
  produces a token jointly addressed to Wyrd and another client, Wyrd treats
  that otherwise untrusted co-recipient as trusted. A compromised co-audience
  that can participate in or influence the provider exchange therefore gains a
  cross-client token-substitution surface the library default and OIDC rule are
  intended to close. At minimum, Wyrd accepts a token profile its configured
  trust contains no basis to accept.
- **Required testable correction:** Remove the all-audiences override and use
  `openidconnect`'s existing default rejection. Keep the current `azp` check so
  a present `azp` must still equal `client_id`; do not add an audience allowlist,
  setting, profile, or second verifier. Change the focused case so
  `aud = [client_id, "other"]` is refused even when `azp = client_id`, while a
  single `aud = client_id` with either no `azp` or matching `azp` succeeds and
  a mismatched `azp` fails.

### Low / Defense In Depth

None. No nonstandard hardening mechanism is required.

### Positive Controls

- The remediation derives the human ID-token audience solely from `client_id`
  and removes the duplicate platform request, view, and persistence field.
- Platform begin and callback now share one boot-owned relying party, so the
  callback can reuse screened metadata and keys without a second cache.
- RFC 9207 issuer mismatch and advertised-but-missing cases fail before the
  authorization code reaches the token endpoint.
- The screened adapter prevents DNS rebinding after screening, redirects,
  proxy bypass, internal-address access under production policy, and unbounded
  response buffering.
- Unknown-key refresh is bounded to one rediscovery and concurrent refreshes
  coalesce through the installed Moka cache.
- The human relying party uses `openidconnect 4.0.1`; the `oauth2` reqwest
  feature is not enabled and no unscreened second provider client exists.

## Prior-finding closure relevant to this domain

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | `PlatformLogin` is constructed once in boot state and reused by both served handlers. | CLOSED |
| `FIND-TASK-009-2` | `PlatformCallbackRequest.iss` is forwarded to `verify_response_issuer`; the served callback journey covers matching, missing, and mismatched values before token IO. | CLOSED |
| `FIND-TASK-009-3` | Workload setup uses one screened typed metadata read without fetching JWKS; workload verification remains on `ExternalVerifier`. | CLOSED |
| `FIND-TASK-009-4` | Cache misses and unknown-key refresh re-enter Moka through `try_get_with`; focused concurrent tests are recorded green. | CLOSED |
| `FIND-TASK-009-5` | Withdrawn by lead direction; not reviewed as an open finding. | WITHDRAWN |
| `FIND-TASK-009-6` | `CodeRedemption` and platform storage derive audience from `client_id`; the duplicate field and column are removed. | CLOSED |
| `FIND-TASK-009-7` | The unused `RelyingParty::http` accessor is absent. | CLOSED |
| `FIND-TASK-009-8` | State and nonce use the library's conventional random constructors. | CLOSED |

## Verification assessment and limits

I ran the exact focused command
`mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib -E 'test(=relying_party::tests::id_token_refusals_fail_closed)'` on the immutable
candidate. It selected one test and passed. That result confirms the finding,
because the test deliberately requires the arbitrary additional-audience token
to succeed. The remediation record supplies exact green selectors for the
other OIDC/security cases and the broader lanes; this static domain review did
not rerun all aggregate or external-provider journeys.

The candidate remained at
`1ddc10e21054ddc158f461e8c6d8aa862c32a067` throughout this review. No source
or report other than this assigned domain report was changed.
