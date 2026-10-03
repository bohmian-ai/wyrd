# Maintainer review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The candidate remained checked out at the stated commit while this review was
performed. The repository has no `.codegraph/` directory, so I used the complete
base-to-candidate diff, the remediation diff from
`e3a47a05d931c010f4c70c75edea2d23c447108b`, and direct reads of the owning
modules, callers, generated projections, and relevant tests. I treated the
standard OAuth/OIDC mechanisms and the caller's locked decisions as fixed.

## Changed-surface coverage

| Changed surface | Owning source, callers, and parity inspected | Maintainer result |
|---|---|---|
| `architecture/wyrd-design.md` | Compared the changed API-key renewal and Human sign-in authority with the approved spec, `architecture/wyrd-doctrine.mdx`, the shared client, OAuth routers, OAuth wire owner, refresh owner, and BFF session owner. The client/library/session split is discoverable, but the final wire-format sentence still contradicts the shipped endpoint-specific behavior. See `MAINT-R2-001`. | **FAIL** |
| `architecture/wyrd-security-posture.md` | Compared refresh rotation, client registration, endpoint requests and responses, BFF custody, ingress-owned device-page rate limiting, sealing scope, and issuer binding with `oauth.rs`, auth/platform routers, `cli_login.rs`, refresh issuance, and `BrowserSessions`. The remediation now distinguishes token, device-authorization, and revocation successes and accurately scopes Wyrd-code logging. | PASS |
| Shared-client `ClientConfig::tenant` rustdoc | Read `ClientConfig::resolve_credential`, `SavedLogins::select`, `canonical_origin`, and the selection tests. The comment accurately states newest-login selection and refusal only when the selected server has saved logins for other tenants. | PASS |
| CLI `AuthCommand` help | Read `login::logout`, `login::status`, and their output types. The help matches delete-first, best-effort RFC 7009 revocation and the token-free status fields. | PASS |
| Server OAuth module rustdoc | Read the full `OAuthError`, `OAuthForm`, `OAuthClients`, `no_store`, token/device/revocation handler return paths, and `wyrd-client::auth::oauth_error`. The remediation accurately describes the three success shapes and the split between extractor-native refusals and `WyrdError` conversions. | PASS |
| Server boot sealing rustdoc and diagnostic | Read `rewrap_sealed_secrets`, `SealedSecretRewrap`, its production/test callers, and the current secret writers. The text now names only provider and workload-issuer client secrets and no longer implies persisted browser-session credentials. | PASS |
| SQL refresh-chain rustdoc | Read `revoke_refresh_chain`, its lock requirement, callers, and refresh rotation/revocation ownership. The changed comment preserves the one-login-chain boundary and other-login isolation. | PASS |
| UI README | Compared its production-auth paragraph with `BrowserSessions`, login callback, recovery sign-in, logout, and the integration-test owner. It correctly locates the encrypted cookie in the BFF and keeps tokens out of page data and JavaScript. | PASS |
| API documentation generator and generated pages | Compared `docs/scripts/generate_api_docs.py` with generated `api/errors.md` and `api/openapi.md`, then traced the claims to `OAuthError` and the four form handlers. Generator/output parity is intact, and the four endpoints now direct raw HTTP consumers to the RFC OAuth error body while noting SDK mapping. | PASS |
| Authentication and identity concept pages | Read all changed passages in `concepts/authentication.svx` and `concepts/identity-and-auth.svx` against the token issuer, refresh owner, BFF cookie, authorization-code/device/API-key/JWT-bearer paths, and plane separation. The pages accurately distinguish public-client rotation, confidential-client fixed lifetime, API-key recovery, form encoding, and access-token validity through expiry. | PASS |
| Cloud identity and deployment configuration | Read `concepts/cloud-identity.svx`, `self-hosting/configuration.svx`, and the Kubernetes OIDC section against both trusted-issuer entry points, `refuse_human_trusted_issuer`, issuer secret sealing/rewrap, and workload assertion exchange. Human trusted issuers are no longer advertised as usable, and secret-bearing workload issuers now carry the correct sealing requirement. | PASS |
| Agent remediation guide | Compared the general Problem Details flow, OAuth table, SDK projection note, retry advice, and credential remediation with `OAuthError`, `oauth_error`, and `ClientConfig` resolution. Raw OAuth and SDK-facing identities are now separated without inventing another error contract. | PASS |
| Client configuration and CLI public guides | Read the changed credential chain, saved-login selection, login/logout/status descriptions, and CLI flags against shared-client and CLI owners. They agree on explicit-credential precedence, newest-login default, tenant mismatch, origin-scoped storage, device login, and best-effort logout. | PASS |
| Self-hosting authentication and SSO/OIDC guides | Traced grant routes, client types, callback, connection lifecycle, test stamp, activation, provider examples, failure table, BFF cookie, operator recovery, RFC 8693 API-key type, and workload curl to the server and BFF owners. The remediation closes the platform-route, cookie-custody, endpoint-success, logging, and activation-liveness contradictions. | PASS |
| Python source documentation and generated stubs | Compared `PyWyrdClient::__new__` with `client/__init__.pyi`, `stubs/client.pyi`, shared `client_from_options`, and saved-login error mapping. Signatures and documentation remain aligned; the generated projections match their source. | PASS |
| TypeScript source, N-API source, and generated declarations | Compared `WyrdClient.connect`, native `connect_wyrd_client`, `index.d.ts`, and `index.d.cts` with the shared client. Option types, newest-login semantics, tenant mismatch, and error projection agree across layers. | PASS |
| TASK-005 evidence and the TASK-005-r1 review/remediation packet | Checked the changed task evidence, prior verdict, six validated findings, reviewer records, remediation requirements, and remediation evidence against the cumulative source. The recorded narrow checks fit the write set. The evidence correctly records the R1 edits, but the original task's architecture-accuracy obligation remains open at `MAINT-R2-001`. | **FAIL** |

## Prior-finding closure

| Prior finding | Maintainer source check | Result |
|---|---|---|
| `FIND-TASK-005-1` — OAuth error identity and logging | The generator/generated pages, agent guide, security posture, SSO guide, and OAuth rustdoc now agree with `OAuthForm`, `OAuthClients`, `OAuthError`, and `oauth_error`. | CLOSED at the remediated locations |
| `FIND-TASK-005-2` — trusted workload issuer scope and sealing | The cloud-identity table and deployment configuration now agree with Human refusal and the supported secret-bearing issuer variants. | CLOSED |
| `FIND-TASK-005-3` — platform and tenant token routes | The operator guide now names `/auth/platform/token` separately and limits `/auth/token` to tenant grants. | CLOSED |
| `FIND-TASK-005-4` — endpoint-specific success responses | Security posture, SSO guide, and OAuth rustdoc now name the token response, RFC 8628 device response, and empty RFC 7009 revocation response separately. The cumulative design-authority overgeneralization in `MAINT-R2-001` prevents complete task-level architecture closure. | PARTIAL |
| `FIND-TASK-005-5` — BFF cookie custody | The operator guide now distinguishes the stateless API request boundary from the encrypted BFF cookie and preserves best-effort logout and access-token lifetime. | CLOSED |
| `FIND-TASK-005-6` — activation and provider liveness | The SSO guide now states that activation checks the current exact-revision test stamp without contacting the provider. | CLOSED |

## Material finding

### MAINT-R2-001 — architecture still describes the entire OAuth surface as form-plus-JSON

- **Location:** `architecture/wyrd-design.md:570-573`
- **Governing rule:** TASK-005 Outcome, Approach 1 and 3, and AC-009 require
  architecture authority to match the shipped standard OAuth surface; the
  maintainer guide requires documentation to describe the same public operation
  as its implementation and generated contract. Documentation findings block
  here only because this statement is materially inaccurate, not because of its
  wording or placement.
- **Evidence:** The new Human sign-in authority says, without narrowing the
  subject, that “The OAuth endpoints use the RFC 6749 form and JSON wire
  format.” The shipped authorization endpoint is a `GET` query/redirect flow,
  not a form/JSON exchange (`auth/authorize.rs`); the user-code endpoint is an
  HTML `GET`/`POST`; and successful revocation has an empty body under RFC 7009
  §2.2 (`auth/cli_login.rs:300-307`). The remediated security authority now
  states the narrower and correct contract at
  `architecture/wyrd-security-posture.md:208-215`: the token, platform-token,
  device-authorization, and revocation endpoints take form bodies; their
  successes retain distinct standard shapes; their refusals use OAuth JSON.
- **Concrete maintenance cost:** `wyrd-design.md` is the first protocol
  authority and wins on conflict. A maintainer following its broad statement
  can incorrectly make authorization/device-page handling form/JSON-only or
  require a JSON body from successful revocation, undoing the endpoint-specific
  correction recorded elsewhere in the same candidate.
- **Smallest testable correction:** Replace only this broad sentence with the
  already-shipped boundary from the security posture: name the four form
  endpoints, state that their refusals use the conventional OAuth JSON rather
  than Problem Details, and leave each success in its owning standard shape.
  Do not add an envelope, route, abstraction, compatibility path, or runtime
  change. Prove the documentation-only edit with `mise run docs:check` and
  `git diff --check`; add `mise run fmt`/`mise run lints` only if Rust source
  documentation changes again.

## Verification assessment

The original task records successful narrow checks for documentation,
generation, Rust formatting/lints, Python formatting/lints, TypeScript N-API
declarations, and whitespace. The remediation records successful
`mise run docs:check`, `mise run codegen:check`, `mise run fmt`,
`mise run lints`, and `git diff --check`. Those are proportionate to the write
sets, and neither a user-journey suite nor a broad aggregate is required by
this review. Mechanical checks cannot reconcile the conflicting architecture
sentence in `MAINT-R2-001`.

No placement, naming, structure, or wording preference is elevated to a
finding. The RFC 8693 API-key exchange type, ingress-owned rate limiting for
`POST /auth/device`, best-effort RFC 7009 logout revocation, origin-normalized
client base URL, and self-contained access-token validity through expiry were
treated as closed decisions and were not reopened.

## Overall result

**FAIL**

The changed code comments, guides, generated declarations, and R1 remediation
are otherwise maintainable and consistent with the shipped conventional
OAuth/OIDC owners. One task-owned sentence remains materially broader than the
actual wire contract and the corrected security authority, so the cumulative
architecture documentation is not yet safe to maintain from.
