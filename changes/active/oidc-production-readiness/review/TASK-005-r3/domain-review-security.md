# OAuth/OIDC Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior remediations: `TASK-005-R1-doc-contract-accuracy.md` and
  `TASK-005-R2-doc-contract-closure.md`

The candidate remained at the stated commit before and after inspection. The
repository has no `.codegraph/` directory, so this review used the cumulative
diff, the R2 remediation diff, `rg`, and direct source inspection. The closed
decisions supplied by the caller were not reopened: RFC 8693 API-key exchange
with `urn:wyrd:oauth:token-type:api_key`, ingress-owned device-page rate
limiting, best-effort RFC 7009 logout revocation, origin-normalized client base
URLs, and access-token validity through expiry.

## Reviewed security boundary

The review traced the documented contracts to the deployed owners for:

- tenant and platform token issuance, OAuth client identification, and the
  separate authorization planes;
- authorization-code, device, refresh, revocation, RFC 8693, and RFC 7523
  request and refusal paths;
- tenant OIDC connection client authentication, provider discovery, PKCE,
  state, nonce, issuer binding, ID-token verification, testing, and activation;
- the BFF encrypted-cookie session, refresh custody, logout, and access-token
  lifetime;
- CLI saved-login custody and renewal; and
- workload-only issuers, sealing-key requirements, and role/tenant boundaries.

No runtime, manifest, or lockfile change occurs in the R2 remediation. The
security question is therefore whether the changed authorities and public docs
accurately describe the shipped trust boundary.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer evidence | Result |
| --- | --- | --- | --- |
| OAuth/OIDC wire and client authentication | REQ-004, REQ-021, AC-009 | `auth/oauth.rs`; tenant `token`; device authorization; revocation; platform token; SSO and API docs | **FAIL** — `SEC-R3-001` |
| Tenant/platform separation | INV-003, REQ-018, AC-009 | tenant and platform routers, `PlatformSessions`, platform OIDC completion, identity/auth concept pages | PASS except the grant-specific overgeneralization in `SEC-R3-001` |
| Provider trust and secret handling | REQ-003–005, REQ-007, INV-001, INV-004 | `HumanClientAuth`, `RelyingParty`, connection testing/activation, sealing and operator docs | PASS |
| Browser and CLI credential custody | REQ-009–012, REQ-016, AC-004, AC-007 | BFF `BrowserSessions`, shared client/CLI login, authentication and SSO docs | PASS |
| Human/workload separation | REQ-013, INV-003, INV-005 | trusted issuer and workload-binding owners, machine documentation | PASS |
| Prohibited and closed scope | TASK-005 non-goals and caller direction | cumulative diff and public docs | PASS |

## Prior-finding closure

| Prior finding | Source-backed result |
| --- | --- |
| `FIND-TASK-005-1` | **CLOSED.** Protocol-native OAuth errors remain distinct from catalog Problem Details, and only catalog conversions promise Wyrd-code logging. |
| `FIND-TASK-005-2` | **CLOSED.** Human trust is routed through tenant connections; workload issuers remain workload-only; stored issuer secrets retain the sealing-key requirement. |
| `FIND-TASK-005-3` | **INCOMPLETE.** Platform exchange is now correctly separated from tenant exchange, but the replacement wording still says the entire tenant token endpoint identifies a registered OAuth client. Machine token-exchange and JWT-bearer grants intentionally do not. See `SEC-R3-001`. |
| `FIND-TASK-005-4` | **CLOSED.** The design, generator, generated pages, agent guide, and SSO guide now distinguish the four form endpoints from browser redirect/HTML/Problem outcomes and document the shipped status set. |
| `FIND-TASK-005-5` | **CLOSED.** The Wyrd API remains stateless while the BFF alone owns the encrypted cookie and in-memory access-token cache. |
| `FIND-TASK-005-6` | **CLOSED.** Activation uses the persisted exact-revision test stamp and performs no provider liveness probe. |
| `FIND-TASK-005-7` | **CLOSED.** Generic provider setup now permits both confidential secret methods and the supported public PKCE client with no secret. |
| `FIND-TASK-005-8` | **CLOSED.** The Rust example uses the shipped `ClientConfig::credential` field. |

## Security Audit

### Critical

None.

### High

None.

### Medium

- `SEC-R3-001` — `crates/wyrd/wyrd-server/src/auth/oauth.rs:6-9` and
  `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152` state that the
  tenant token endpoint identifies one of the registered OAuth clients. The
  shipped handler only requires a client for `authorization_code`,
  `refresh_token`, and `device_code`; RFC 8693 API-key/delegation exchanges and
  RFC 7523 JWT-bearer assertions proceed with no registered client
  (`components/auth/routes.rs:119-180`). The same correction sweep leaves the
  broader statements that every API request is authenticated by a signed token
  and that a Wyrd JWT is presented on every request
  (`docs/src/content/docs/self-hosting/authentication.svx:11,19`), although the
  authorization, callback, device, metadata, token, revocation, and OpenAPI
  bootstrap surfaces are intentionally unauthenticated and accept their own
  standard inputs. A workload or API-key integrator can therefore infer that a
  `wyrd-cli`/`wyrd-ui` registration or pre-existing bearer token is required,
  and a maintainer or ingress operator can apply bearer enforcement to the
  bootstrap routes and make standard login or machine exchange unreachable.
  Scope registered-client identification to the human grants at
  `/auth/token`, keep device authorization and revocation client-authenticated,
  and describe signed tokens as the credential on authenticated resource
  requests. Preserve the existing handlers, optional `OAuthClients::identify`
  result for machine grants, two registered tenant clients, separate platform
  route, and all standard wire behavior; add no client, alias, middleware, or
  compatibility path.

### Low / Defense In Depth

None. Wording, placement, structure, optional completeness, and speculative
hardening were excluded.

### Positive Controls

- Tenant and platform credential exchanges have separate routes and owners;
  neither plane's credential is accepted by the other.
- The platform token route authenticates the presented platform API-key
  subject directly and issues access only; platform OIDC completion likewise
  issues no refresh token.
- Human grants enforce the correct registered client: confidential `wyrd-ui`
  uses Basic authentication, public `wyrd-cli` names itself, and authorization
  codes remain bound to the exact redirect and PKCE verifier.
- `HumanClientAuth::Public` requires the secret to be absent; secret methods
  require one, and the relying party delegates protocol mechanics to
  `openidconnect` through the screened transport.
- Provider callback state is one-time and server-bound; issuer response
  binding precedes redemption where applicable, and ID-token issuer,
  audience, nonce, time, signature, algorithm, and key validation remain in
  the relying-party owner.
- The BFF uses `openid-client`, PKCE S256, and a `jose`-encrypted Secure,
  HttpOnly, SameSite=Lax cookie. Logout clears local state and performs
  best-effort revocation without promising instant access-token withdrawal.
- Connection activation requires the exact current tested revision and a live
  tenant recovery credential without storing or logging that credential.
- Provider and workload issuer secrets remain encrypted at rest and absent
  from response projections. No dependency manifest or lockfile changed.

## Material proposed finding

### SEC-R3-001 — INCORRECT: registered-client and bearer authentication are still generalized across clientless bootstrap grants

- **Violated obligation:** REQ-018, REQ-021, INV-003, AC-009, TASK-005
  Approach 3, and TASK-005-R2 acceptance criteria 1 and 5 require public and
  source documentation to match the shipped standard grant-specific
  authentication contract.
- **Locations:**
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:6-9`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`
  - `docs/src/content/docs/self-hosting/authentication.svx:11,19`
- **Evidence:** `token` calls `OAuthClients::identify`, preserves its result as
  optional, and evaluates `required?` only for authorization-code, refresh,
  and device-code grants. Both token-exchange branches and the JWT-bearer
  branch execute without it (`components/auth/routes.rs:119-180`). The auth
  router and served OpenAPI also expose unauthenticated authorization,
  callback, device, metadata, token, and revocation/bootstrap operations.
- **Observable consequence:** Standard machine clients can be told to invent
  or borrow a tenant OAuth client identity they do not need, while operators
  can infer that the bootstrap surfaces require a bearer token and place them
  behind enforcement that makes authentication impossible. The implementation
  currently fails closed correctly; the public trust-boundary contract is the
  defect.
- **Required testable correction:** At the existing documentation owners,
  state that `/auth/token` consults a registered tenant client when supplied
  and requires one for authorization-code, refresh, and device-code grants;
  RFC 8693 API-key/delegation and RFC 7523 JWT-bearer grants do not register or
  require one. State that signed Wyrd access tokens authenticate protected
  resource/API requests, not every auth bootstrap or public metadata request.
  Keep device authorization and revocation client-authenticated and keep the
  platform endpoint clientless. Do not change runtime behavior or add a client,
  route, token, middleware exception, or compatibility surface.
- **Focused closure proof:** Compare the corrected text to the complete
  `token` match, `OAuthClients::identify`/`require`, device authorization,
  revocation, the two router builders, and the served security declarations.
  Run only `mise run docs:check`, `mise run fmt`, and `mise run lints` for the
  resulting docs/rustdoc write set; add `mise run codegen:check` only if a
  generated source owner is touched. No journey or aggregate is required.

## Verification limits

The task packet records successful `mise run docs:check`, `mise run
codegen:check`, `mise run fmt`, `mise run lints`, and `git diff --check` for
the remediation write set. Those lanes establish rendering, generated-page
parity, formatting, and lint consistency; they do not establish semantic
agreement between prose and grant dispatch. Direct source tracing supplies
that comparison here.

No full journey, language suite, broad aggregate, live IdP, database, or
browser suite was run or required. The remediation changed documentation,
generated documentation, and module rustdoc only.

## Overall result

**FAIL**

Seven prior findings are closed. `FIND-TASK-005-3` remains incomplete at the
registered-client boundary, and the same correction sweep leaves a materially
false bearer requirement in the operator authentication guide. The required
correction is documentation-only and preserves the shipped standard OAuth/OIDC
implementation.
