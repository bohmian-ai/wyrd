# Maintainer review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior remediations:
  - `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`
  - `changes/active/oidc-production-readiness/review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`

The candidate remained checked out at the stated commit throughout this review.
The repository has no `.codegraph/` directory, so I used the complete cumulative
diff, the latest remediation diff from
`323ce32118ec72752a7736b8d42dd957abf6a094`, and direct inspection of the owning
modules, callers, generated projections, and relevant checks. I applied the
standing direction that documentation blocks only when it is materially false,
misleading about shipped behavior, or omits a task requirement. Placement,
naming, structure, phrasing, additional precision, and completeness beyond the
task were not treated as blocking.

## Changed-surface coverage

| Changed surface | Owning source, callers, and parity inspected | Maintainer result |
|---|---|---|
| `architecture/wyrd-design.md` | Compared the API-key renewal and Human sign-in authority with tenant and platform routers, authorization/device/callback handlers, `OAuthError`, refresh issuance, shared-client auth, and the BFF session owner. The authority now limits form-plus-OAuth-JSON behavior to the four form endpoints and separately describes browser redirects, HTML, and Problem Details (`wyrd-design.md:555-581`). | PASS |
| `architecture/wyrd-security-posture.md` | Traced access/refresh lifetime, registered clients, endpoint-specific success and refusal shapes, BFF custody, ingress-owned device-page rate limiting, sealing scope, and issuer binding to their runtime owners. It preserves conventional OAuth/OIDC behavior and the fixed logout/access-token decisions. | PASS |
| Shared `ClientConfig` rustdoc | Read `ClientConfig::from_global`, `resolve_credential`, `SavedLogins::select`, canonical-origin matching, and the saved-login selection tests. The changed `tenant` docs accurately distinguish newest-login default, explicit tenant selection, and mismatch refusal. | PASS |
| CLI auth help | Read `AuthCommand`, logout, status, and their output models. The help correctly describes local deletion followed by best-effort RFC 7009 revocation and does not claim status prints a principal or token. | PASS |
| OAuth module rustdoc | Read `OAuthForm`, `OAuthClients`, `OAuthError`, `no_store`, all four form handlers, and their return sites. The module now distinguishes registered tenant-client identification from direct platform API-key exchange and names the actual success shapes and reachable `400`/`401`/`500`/`503` statuses (`auth/oauth.rs:1-17`). | PASS |
| Server boot sealing rustdoc and diagnostic | Read `rewrap_sealed_secrets`, `SealedSecretRewrap`, its callers, and the provider/workload-issuer secret writers. The text now names the durable ciphertext the operation actually rewraps and no longer implies a server-side browser-session secret. | PASS |
| SQL refresh-chain rustdoc | Read `revoke_refresh_chain`, its family-lock contract, and refresh/revocation callers. The changed wording preserves the one-login-chain boundary without implying a retired server-side browser-session model. | PASS |
| UI README | Compared production sign-in setup with `BrowserSessions`, login callback, recovery sign-in, logout, and the BFF dependencies. It accurately locates the encrypted HttpOnly cookie in the BFF and keeps Wyrd/provider tokens out of browser data and JavaScript. | PASS |
| API documentation generator and generated pages | Compared `generate_api_docs.py` with `api/openapi.md` and `api/errors.md`, then traced its taxonomy to the served handler declarations. Generator/output parity is intact; JSON API Problem Details, the four OAuth form endpoints, and browser interactions are no longer collapsed into one envelope. | PASS |
| Authentication and identity concept pages | Traced both planes, grant dispatch, refresh behavior, BFF custody, API-key/JWT-bearer paths, and endpoint shapes to the token issuer, platform sessions, refresh owner, shared client, and BFF. Tenant `/auth/token` is now explicitly separated from `/auth/platform/token` and access-only platform OIDC completion (`identity-and-auth.svx:87-124`). | PASS |
| Cloud identity, deployment configuration, and Kubernetes guidance | Compared trusted-issuer scope and secret requirements with boot seeding, admin creation, Human refusal, sealing, and workload assertion exchange. The guides advertise only the accepted workload path and correctly distinguish public from secret-bearing issuer configuration. | PASS |
| Agent error-remediation guide | Compared Problem Details guidance, browser outcomes, OAuth `error` handling, status mapping, SDK projection, and credential remediation with `OAuthError`, shared-client `oauth_error`, and `ClientConfig`. It gives raw HTTP and SDK consumers the right branching identity without inventing another error mechanism. | PASS |
| Client configuration and CLI public guides | Read the Rust example, credential precedence, origin-scoped saved-login selection, device login, logout, and status guidance against the shared client and CLI owners. The example now uses the shipped `ClientConfig::credential` field (`client-configuration.svx:91-99`); selection and best-effort logout claims remain aligned. | PASS |
| Self-hosting authentication and SSO/OIDC guides | Traced tenant and platform routes, callback, provider client-auth choices, connection lifecycle, test stamp, activation, BFF cookie, recovery, RFC 8693 API-key exchange, and workload form example to runtime owners. The generic setup now permits the shipped confidential and public IdP clients, and the endpoint table preserves separate platform behavior (`sso-and-oidc.svx:57-65,139-154`). | PASS |
| Python source documentation and generated stubs | Compared `PyWyrdClient::__new__`, both generated `.pyi` files, `client_from_options`, and saved-login error mapping. Signatures and documentation remain aligned; generated output matches the Rust source contract. | PASS |
| TypeScript public source, N-API source, and generated declarations | Compared `WyrdClient.connect`, native `connect_wyrd_client`, `index.d.ts`, and `index.d.cts` with the shared client. Option types, newest-login behavior, mismatch semantics, and error projection agree across layers. | PASS |
| TASK-005 evidence and both prior review packets | Reconciled the original acceptance evidence, R1's six findings, R2's four retained findings, both remediation tasks, and their recorded focused checks with the cumulative source. The product sources contain no task IDs or implementation-history notes; the packet remains review evidence only. | PASS |

## Prior-finding closure

| Finding | Maintainer source check | Result |
|---|---|---|
| `FIND-TASK-005-1` — OAuth error identity and logging | Generator/output, agent guide, security posture, SSO guide, and OAuth rustdoc distinguish protocol-native errors from `WyrdError` conversions. | CLOSED |
| `FIND-TASK-005-2` — trusted workload issuer scope and sealing | Cloud-identity and configuration guides match Human refusal and secret-bearing workload issuer sealing. | CLOSED |
| `FIND-TASK-005-3` — tenant/platform plane separation | Identity concepts and OAuth rustdoc now name the separate platform route, direct credential exchange, lack of registered platform OAuth client, and access-only platform login. | CLOSED |
| `FIND-TASK-005-4` — endpoint-specific OAuth wire | Design authority, generator/output, agent guide, SSO guide, and OAuth rustdoc distinguish form endpoints from browser interactions and preserve each success/refusal shape. | CLOSED |
| `FIND-TASK-005-5` — BFF cookie custody | Operator and concept docs distinguish the stateless Wyrd API boundary from the BFF's encrypted HttpOnly cookie. | CLOSED |
| `FIND-TASK-005-6` — activation liveness | The SSO guide states that activation checks the exact-revision 15-minute stamp and performs no provider probe. | CLOSED |
| `FIND-TASK-005-7` — public IdP client support | Generic provider setup permits confidential `SecretBasic`/`SecretPost` and secretless `Public`, preserving PKCE and current sealing requirements. | CLOSED |
| `FIND-TASK-005-8` — Rust example field | The example assigns `ClientConfig::credential`, the actual re-exported shared-client field. | CLOSED |

## Material findings

None. The cumulative changed surfaces are discoverable, consistent with their
owners, and safe for a maintainer to follow. No changed signature, generated
declaration, example, architecture statement, or operator instruction allows an
invalid call or materially contradicts shipped behavior.

## Uncertain preferences

None recorded. A few paragraphs and rustdoc lines could be reflowed, and the
pre-existing constructor paragraph adjacent to the corrected Rust example is
grammatically incomplete, but neither changes or obscures the task-owned OAuth,
credential-selection, or public API contract. Under the standing direction,
these are not findings and do not justify another documentation round.

## Verification assessment

The supplied evidence records successful `mise run docs:check`, `mise run
codegen:check`, `mise run fmt`, and `mise run lints` for the latest
documentation, generator/generated-page, and Rust-rustdoc write set. Those are
the narrowest relevant lanes, and no user-journey suite, language suite, live
provider test, browser suite, or broad aggregate is required for this
documentation-only remediation.

A cumulative `git diff --check
134f605367e65b41f1977d6c70ac8ca8b277a69e..bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
still reports only the previously documented extra blank line at EOF in
`review/TASK-005-r1/verdict.md`. That review-artifact whitespace has no shipped
behavior, public-contract, security, or task-documentation consequence and is
non-blocking under the standing direction. I did not rerun broad or journey
lanes.

The RFC 8693 API-key exchange type, ingress-owned `POST /auth/device` rate
limiting, best-effort RFC 7009 logout revocation, origin-normalized client base
URL, and self-contained access-token validity through expiry were treated as
closed shipped decisions and were not reopened.

## Overall result

**PASS**

All materially changed surfaces were covered, all eight prior findings are
closed in the cumulative candidate, and no material maintainer finding remains.
