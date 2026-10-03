# TASK-005 remediation-round invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-005-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`
- Latest remediation implementation: `e3a47a05d931c010f4c70c75edea2d23c447108b..323ce32118ec72752a7736b8d42dd957abf6a094`

The candidate was `323ce32118ec72752a7736b8d42dd957abf6a094` before source inspection. The repository has no `.codegraph/` directory, so I used the complete Git diffs, `rg`, and direct source inspection.

## Invariant and lifecycle trace

I reassessed the complete base-to-candidate range and used the remediation diff only to locate the owners changed since round 1.

- The four form endpoints share `OAuthForm`, `OAuthClients`, and `OAuthError`. Extractor and client-identification refusals construct `OAuthError` directly and carry only the RFC error identity; only `From<WyrdError>` emits the `wyrd_code` log field. Token and platform-token successes return `TokenResponse`, device authorization returns `DeviceAuthorization`, and revocation returns an empty no-store `200`.
- Tenant issuance remains at `/auth/token`; the platform API-key exchange remains separately mounted at `/auth/platform/token`. No alias or fallback was added.
- Boot seeding and runtime administration both refuse a Human trusted issuer in favor of tenant OIDC connections. Workload issuer `secret_basic` and `secret_post` values go through the existing sealing owner; public and `private_key_jwt` entries carry no stored shared secret.
- `BrowserSessions` derives its encryption key from the BFF client secret, stores the refresh token or recovery API key in a Secure, HttpOnly, SameSite=Lax encrypted cookie, and sends a bearer token to the stateless API server. Logout deletes local cookie/cache state and attempts RFC 7009 revocation; self-contained access tokens retain their bounded validity.
- A completed provider test stamps only the exact candidate revision for the 15-minute validity window. `HumanConnections::activate` checks that persisted stamp and recovery authority under the connection-slot lock and performs no provider IO.
- Saved-login selection still flows through `ClientConfig::resolve_credential` and `SavedLogins::select`: a tenant selector chooses that tenant or fails when the same server has only other-tenant logins; without one, the newest login for the canonical server origin wins. The Rust SDK re-exports this `ClientConfig` unchanged.

The six round-1 contradictions are closed at their source owners. One separate, reachable documentation contract error remains in the cumulative candidate: the Rust configuration example still assigns a removed `ClientConfig::api_key` field even though the surrounding task edits correctly name `ClientConfig::credential`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: OIDC remains optional and existing operator, UI, SDK, and machine paths remain usable | Architecture and self-hosting docs preserve OIDC-off recovery and independent machine credentials; the cumulative task diff changes no runtime behavior | Owning journey evidence remains mapped in TASK-005; broad journeys were intentionally not rerun | PASS |
| REQ-005: only stored provider or workload-issuer shared secrets require sealing | `cloud-identity.svx:85-117` and `configuration.svx:72-94` distinguish secret-bearing from secretless workload issuers; boot/admin persistence uses the existing sealing owner | Recorded `docs:check`, `codegen:check`, `fmt`, and `lints` exited 0; static comparison to boot/admin owners | PASS — prior `FIND-TASK-005-2` closed |
| REQ-018: responsibilities, callback, connection lifecycle, role mapping, rotation, failures, recovery, saved login, and separate human/workload paths are documented accurately | The primary SSO, authentication, cloud-identity, configuration, CLI, and client pages cover the required flows; platform/tenant routing, BFF cookie custody, and activation test-stamp behavior now match their owners | Recorded narrow checks exited 0; source comparison performed here | FAIL — `INV-R2-001` leaves a shipped Rust-client configuration example unusable |
| REQ-021: OAuth request, success, and refusal contracts remain standard and endpoint-specific | `oauth.rs:1-14`, security posture, generated API intros, agent guidance, and SSO docs distinguish token, device-authorization, and revocation successes and the RFC error body | Recorded `docs:check`, `codegen:check`, `fmt`, and `lints` exited 0; handler return sites and extractors inspected | PASS — prior `FIND-TASK-005-1` and `FIND-TASK-005-4` closed |
| INV-001 / INV-004: tenant and connection selection stays server-bound and OIDC trust fails closed | Callback/state, issuer, PKCE, nonce, JWKS, and failure guidance remain aligned; no executable path changed | Existing owner evidence plus cumulative diff inspection | PASS |
| INV-003 / INV-005: platform, tenant-user, and workload planes stay distinct and clients project server authority | `authentication.svx:21-33` names the tenant and platform token routes separately; SDK docs remain projections of the shared client | Static route and client-owner comparison | PASS — prior `FIND-TASK-005-3` closed |
| INV-006 and task non-goals: no hosted signup, social login, commercial stub, SAML, SCIM, certified-provider list, or provider-specific implementation | Complete cumulative diff introduces none | Diff/source inspection | PASS |
| AC-002 / AC-003: self-hosted and hosted provider setup, exact callback, role mapping, and tenant isolation evidence is mapped | SSO guide retains the generic setup and exact callback contract | Owning-task journeys remain listed, not rerun per direction | PASS |
| AC-004 and FIND-TASK-004-8: Rust, Python, and TypeScript describe RFC 8628 saved login, newest-login default, and selected-tenant mismatch | `ClientConfig::tenant`, PyO3 source/stubs, napi/TypeScript source and generated declarations agree on selection and mismatch behavior | Recorded `codegen:check` exited 0; source selection path inspected | PASS for the carried selection obligation; broader AC-009 fails on `INV-R2-001` |
| AC-005: machine paths remain independent from human SSO | Human and workload docs preserve API-key exchange and workload assertion paths; no runtime behavior changed | Existing owner evidence | PASS |
| AC-006 / AC-007: candidate testing/activation, failure, recovery, rotation, logout, and bounded access-token validity are accurate | `sso-and-oidc.svx:111-119` now states that activation checks the exact-revision stamp but does not re-probe; authentication/SSO docs preserve best-effort revocation and bounded access-token validity | Static comparison to `stamp_test_sign_in`, `activate`, and `human_candidate_test_is_current` | PASS — prior `FIND-TASK-005-5` and `FIND-TASK-005-6` closed |
| AC-008: provider-agnostic OIDC and only short common-provider examples | Generic OIDC instructions and five short examples remain; no provider-specific code or certification claim entered the diff | Diff/source inspection | PASS |
| AC-009: architecture, docs, CLI help, UI, SDKs, generated declarations, and shipped public names agree | OAuth, issuer, route, BFF, activation, and saved-login prose mostly agree, but the Rust setup example writes a field the public type does not have | Mechanical lanes are green but do not compile the fenced Rust snippet; direct comparison to `ClientConfig` | FAIL — `INV-R2-001` |
| Required deletion and closed decisions | Scoped search finds no private BFF channel, server-side browser-session rows, sealed-login completion, session-sealing key model, SAML, or SCIM guidance. RFC 8693 API-key exchange, ingress device-page rate limiting, best-effort RFC 7009 revocation, origin-normalized client URLs, and access-token validity through expiry remain unchanged | Cumulative diff and current-source search | PASS |
| Remediation constraint: documentation/source-documentation only, with no new runtime mechanism | The latest fix changes prose, the API-doc generator and generated pages, and OAuth module rustdoc only | Latest fix diff inspection | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-005-1` | Generator-owned OpenAPI/error pages, agent guidance, security posture, SSO guide, and `oauth.rs` now state that OAuth bodies carry `error`, not Wyrd `code`; logging is limited to `From<WyrdError>`. Direct extractor/client refusals remain code-less. | CLOSED |
| `FIND-TASK-005-2` | Cloud-identity docs describe trusted issuers as workload-only, direct human federation to tenant connections, and name the sealing requirement for `secret_basic`/`secret_post`; boot and admin owners enforce those statements. | CLOSED |
| `FIND-TASK-005-3` | Authentication docs scope `/auth/token` to tenant grants and name the existing `/auth/platform/token` platform exchange. | CLOSED |
| `FIND-TASK-005-4` | Security posture, SSO docs, and OAuth rustdoc separately describe RFC 6749 token success, RFC 8628 device authorization, and RFC 7009 empty revocation success. | CLOSED |
| `FIND-TASK-005-5` | Authentication docs distinguish the stateless Wyrd API server from the BFF-owned encrypted cookie and preserve best-effort logout plus access-token validity through expiry. | CLOSED |
| `FIND-TASK-005-6` | SSO activation guidance now states that activation checks the exact-revision 15-minute stamp without provider IO, while an outage blocks a new test and later login. | CLOSED |

## Proposed findings

### INV-R2-001 — INCORRECT — the Rust client configuration example assigns a nonexistent public field

- **Violated obligation:** TASK-005's documentation-accuracy outcome, REQ-018, and AC-009 require SDK configuration documentation to match the shipped public client contract.
- **Location:** `docs/src/content/docs/get-started/client-configuration.svx:88-99`, specifically line 97.
- **Producer-to-consumer evidence:** The example tells a Rust caller to assign `config.api_key`. The sole shared client type is `crates/shared/wyrd-client/src/config.rs::ClientConfig`, re-exported unchanged by `sdks/wyrd-sdk-rust`; its explicit credential field is `credential: Option<SecretString>` and it has no `api_key` field. The same changed page correctly names `ClientConfig::credential` in its credential-precedence list, and the cumulative task diff materially edited both that surrounding page and `ClientConfig` documentation without correcting the example.
- **Reachable consequence:** A user following the documented programmatic-override path gets a Rust compile error (`no field api_key on type ClientConfig`) before a client can be constructed. This is a false public API contract, not a wording, placement, or style preference.
- **Required testable correction:** Change the existing example to assign the shipped `ClientConfig::credential` field. Do not add an alias, compatibility field, helper, or runtime behavior. Confirm the corrected name against the shared client owner and its Rust SDK re-export, then run `mise run docs:check` and `git diff --check`; no journey, aggregate, or new harness is warranted for this documentation-only correction.

## Non-blocking notes

None. I did not retain placement, naming, structure, or wording preferences.

## Verification assessment

The supplied remediation evidence records exit 0 for `mise run docs:check`, `mise run codegen:check`, `mise run fmt`, `mise run lints`, and `git diff --check`. Those are the narrowest lanes covering the remediation write set, and no full journey suite or aggregate is required. They credibly establish generated-page parity, rendering, formatting, and lint cleanliness, but `docs:check` does not compile the Rust fenced example and therefore does not close `INV-R2-001`.

## Overall result

**FAIL**

All six prior findings are source-closed without reopening a shipped OAuth/OIDC decision. The cumulative task is not acceptance-complete while a task-owned client configuration example names a public field that does not exist.
