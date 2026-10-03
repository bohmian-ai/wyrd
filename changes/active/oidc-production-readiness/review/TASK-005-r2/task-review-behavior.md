# TASK-005 behavior review

## Proposed findings

### BHV-R2-001 — INCORRECT: the cross-plane concept map still sends platform issuance through the tenant route

- **Violated obligation:** REQ-018, REQ-021, INV-003, TASK-005 Approach 3,
  remediation acceptance criterion 3, and AC-009 require the platform and
  tenant administration planes and their public token routes to remain
  distinct in the documentation.
- **Location:** `docs/src/content/docs/concepts/identity-and-auth.svx:3,17-20,45,86-109`.
- **Evidence:** The page presents itself as the end-to-end map across both
  planes, but says that all machine-facing token issuance and human login end
  at `/auth/token`, and describes human refresh behavior only as the tenant
  CLI/web-app variants. Production instead mounts platform credential exchange
  at `/auth/platform/token` (`crates/wyrd/wyrd-server/src/components/platform/routes.rs:79-131`).
  Platform OIDC completes at `/auth/platform/callback` and returns a 15-minute
  access-only `TokenResponse` whose `refresh_token` is `None`
  (`crates/wyrd/wyrd-server/src/components/platform/identity.rs:719-764` and
  `crates/wyrd/wyrd-server/src/components/platform/routes.rs:145-153`). The
  remediation corrected the same overgeneralization on the operator
  authentication page but did not close this sibling public consumer.
- **Observable consequence:** A platform operator following Wyrd's advertised
  cross-plane map can send a platform credential to the tenant endpoint or
  expect tenant-style refresh behavior from a platform federated login; both
  conflict with the deliberately separate platform contract.
- **Required testable correction:** Keep the existing route owners and scope
  this page's `/auth/token` dispatch and refresh-token statements to tenant
  issuance. Name `/auth/platform/token` as the platform credential exchange and
  describe platform OIDC completion as the existing access-only platform
  session. Do not add an alias, fallback, refresh path, or new abstraction.
  Compare the corrected text to both platform handlers and run only
  `mise run docs:check`.

### BHV-R2-002 — INCORRECT: the generic provider setup requires a confidential client while the shipped contract supports a public client

- **Violated obligation:** REQ-005, REQ-018, TASK-005 Approach 2, and AC-009
  require setup inputs and sealing-key needs to match the selected supported
  client-authentication method.
- **Location:** `docs/src/content/docs/self-hosting/sso-and-oidc.svx:59-65`.
- **Evidence:** The generic instructions say every provider requires an OIDC
  confidential Web client, then four lines later offer `Public` for a client
  without a secret. `HumanClientAuth` explicitly supports `Public`, for which a
  secret must be absent (`crates/wyrd-spec/src/auth/human_connection.rs:70-80,232-264`),
  and code redemption maps it to a client with no secret while retaining PKCE
  (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:503-535`). The tenant
  connection handler's coverage also stages `Public` as a valid candidate
  (`crates/wyrd/wyrd-server/src/components/admin/identity.rs:737-743`).
- **Observable consequence:** An operator using a provider's public-client
  registration is told that its supported configuration is not a valid setup,
  or is led to create and seal an unnecessary shared secret.
- **Required testable correction:** Describe the registration generically as a
  Web/OIDC application using authorization code with PKCE, then distinguish
  the already-supported confidential (`SecretBasic`/`SecretPost`) and public
  (`Public`, no secret) choices. Preserve the current contract and sealing-key
  behavior; add no client method or compatibility path. Compare the corrected
  text to `HumanClientAuth` and `RelyingParty::redeem`, then run only
  `mise run docs:check`.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior review and remediation: `changes/active/oidc-production-readiness/review/TASK-005-r1/`

The candidate remained checked out at the stated commit while this report was
prepared. The repository has no `.codegraph/` directory, so navigation used
the complete Git diff, `rg`, and direct source reads. The review covered the
complete base-to-candidate range and used the remediation delta
`e3a47a05d931c010f4c70c75edea2d23c447108b..323ce32118ec72752a7736b8d42dd957abf6a094`
to locate the corrected owners.

## Navigation map

| Public path or obligation | Documentation consumer | Runtime/source owner followed |
| --- | --- | --- |
| OAuth request, success, error, and logging contract | API generators and generated pages, agent error guide, SSO guide, OAuth rustdoc | `OAuthForm`, `OAuthClients`, `OAuthError`; tenant/platform token handlers; device and revoke handlers |
| Tenant and platform issuance split | Authentication and identity concept pages, SSO guide | tenant and platform routers; platform OIDC callback and `platform_session_response` |
| Tenant IdP setup and supported client authentication | SSO setup guide and configuration guide | `HumanClientAuth`, `ConnectionInput`, `RelyingParty::redeem`, connection staging |
| Workload-only trusted issuers and sealing | Cloud-identity and configuration guides | boot/admin Human refusal and `issuer_write_from_trusted` |
| Web-app credential custody and logout | authentication concepts and operator guide | `BrowserSessions`, refresh/revoke owners |
| Candidate testing and activation | SSO lifecycle guide | `stamp_test_sign_in`, `HumanConnections::activate`, persisted 15-minute test stamp |
| Saved-login selection and SDK declaration parity | client configuration, CLI, Rust/Python/TypeScript source docs and generated declarations | `ClientConfig::resolve_credential`, `SavedLogins`, shared client OAuth mapping |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: tenant OIDC remains optional; existing credentials, OIDC-off UI recovery, and machine identity remain usable | SSO and authentication guides retain API-key recovery and independent machine paths; no executable behavior changed | Owning TASK-003/011 journey evidence recorded in TASK-005; not rerun by contract | PASS |
| REQ-005: provider and secret-bearing workload issuer secrets alone require the deployment sealing key, with no secret exposure | Authentication/configuration/cloud-identity docs now name tenant, platform, and workload issuer secrets and distinguish secretless methods | `docs:check`; static comparison with staging/sealing owners | PASS |
| REQ-018: operator/IdP responsibilities, exact callback, one active connection, role mapping, rotation, failures, recovery, CLI selection, and separate human/workload paths are accurate | SSO guide covers each required item, but its universal provider-registration step contradicts the supported public tenant connection | `docs:check`; source comparison exposes `BHV-R2-002` | FAIL |
| REQ-021: OAuth form endpoints use their RFC-owned request, success, and error contracts | Generator, generated pages, SSO guide, security posture, and OAuth rustdoc now distinguish token, device, revoke, and protocol-native errors | `docs:check`, `codegen:check`, `fmt`, `lints` | PASS |
| REQ-021 / INV-003: platform and tenant token routes remain separate | Operator authentication guide now names both routes, but the cross-plane identity map still claims all machine and human issuance ends at `/auth/token` | Static route/callback comparison exposes `BHV-R2-001` | FAIL |
| INV-001 / INV-004: tenant and connection selection remain server-bound and provider trust remains fail closed | SSO documentation preserves state, PKCE, nonce, issuer binding, discovery screening, and exact callback behavior | Existing owner journeys listed from TASK-009/010; no executable change | PASS |
| INV-005: UI and first-class SDKs project server-owned identity and permissions | BFF docs describe bearer calls and encrypted cookie custody; SDK docs share the Rust client resolution behavior | Recorded codegen and declaration checks; no language-local durable behavior added | PASS |
| INV-006 and task non-goals: no signup, social login, commercial stub, certified-provider list, SAML, SCIM, or provider-specific implementation | Complete diff adds none; provider notes remain standard configuration examples only | Complete diff/static search | PASS |
| AC-002 / AC-003: self-hosted and hosted tenant connection setup and tenant-isolation journey ownership are recorded | Exact callback, hosted responsibility split, one active connection, wrong-tenant/same-issuer journey owners are listed in task evidence | Owning TASK-002/009/011 evidence recorded; journeys intentionally not rerun | PASS |
| AC-004 and FIND-TASK-004-8: Rust, Python, and TypeScript describe RFC 8628 login, explicit precedence, tenant selection, newest-login default, and mismatch refusal | Shared-client, CLI, Python source/stubs, TypeScript source/native/generated declarations, and client guide agree | Recorded codegen/N-API/language checks from TASK-005; remediation did not touch these surfaces | PASS |
| AC-005: machine identity remains independent of tenant human SSO | Workload guide uses form-encoded RFC 7523 and exact workload bindings; SSO guide says machines remain independent | Owning TASK-002/012 evidence recorded | PASS |
| AC-006: replacement requires exact-revision real sign-in and recovery authority | SSO lifecycle guide describes test, stamp, activation, replacement, and recovery key | Owning TASK-002/011 evidence recorded | PASS |
| AC-007: failure, recovery, rotation, logout, and expiry behavior are accurate | Remediation now states activation does not re-probe, logout revocation is best-effort, and access tokens live to bounded expiry | Static owner comparison; focused checks passed | PASS |
| AC-008: standard provider-agnostic OIDC with short Okta, Entra ID, Google, Auth0, and Keycloak notes | Generic connection contract plus five examples; no provider branch or certification claim | Docs check and complete diff | PASS |
| AC-009: architecture, docs, CLI, UI, SDKs, generated declarations, and shipped behavior agree | The six prior defects are corrected at their named locations, but the cross-plane concept map and generic confidential-client instruction remain contradictory | Focused checks are green but cannot prove prose semantics | FAIL — `BHV-R2-001`, `BHV-R2-002` |
| Required deletion: no private BFF channel, browser-session row, sealed completion, or session-sealing-key design remains | Docs now distinguish stateless API server from BFF-owned encrypted cookie and use the web-app secret for cookie encryption | Complete diff/static search | PASS |
| Locked decisions remain closed | RFC 8693 API-key type, ingress-owned `/auth/device` rate limiting, best-effort RFC 7009 revocation, origin-normalized clients, and access-token validity through expiry are all preserved | Complete diff/static comparison | PASS |
| Documentation-only task and remediation add no runtime behavior, dependency, endpoint, alias, retry, or storage mechanism | Production changes are documentation, generated declarations, source docs/rustdoc, and help text only | Complete diff | PASS |

## Prior-finding closure

| Prior finding | Closure evidence | Result |
| --- | --- | --- |
| `FIND-TASK-005-1` OAuth error identity/logging | Generator and generated API pages exempt the four OAuth form endpoints; agent guide and OAuth rustdoc distinguish extractor-native refusals from `WyrdError` conversions | CLOSED |
| `FIND-TASK-005-2` trusted issuer scope/sealing | Cloud-identity describes workload-only issuers and secret-bearing issuer sealing; configuration routes human setup to tenant connections | CLOSED |
| `FIND-TASK-005-3` platform vs tenant token route | The named operator guide is corrected, but the public cross-plane identity map retains the same false route generalization | NOT CLOSED — `BHV-R2-001` |
| `FIND-TASK-005-4` endpoint-specific success responses | Security posture, SSO guide, and OAuth rustdoc separately name token, device-authorization, and empty revocation success | CLOSED |
| `FIND-TASK-005-5` BFF cookie custody | Operator authentication now distinguishes the stateless API server from the encrypted HttpOnly BFF cookie | CLOSED |
| `FIND-TASK-005-6` activation liveness | SSO guide says activation checks the persisted exact-revision stamp and performs no provider IO | CLOSED |

## Non-blocking notes

None. The findings above concern false public setup and route behavior, not
placement, naming, structure, or wording preference.

## Verification notes

Available focused checks all exited zero: `mise run docs:check`, `mise run
codegen:check`, `mise run fmt`, `mise run lints`, and `git diff --check`.
Those lanes credibly cover rendering, generated-page parity, Rust source-doc
format/lint, and whitespace. They do not establish semantic agreement between
prose and the runtime owners cited above. In accordance with the caller's
standing direction, no full journey suite or broad aggregate was run or
required.

## Overall result

**FAIL**

Two bounded documentation defects remain. Both corrections reuse the existing
standard OAuth/OIDC contracts and route owners and require only the narrow docs
lane; neither reopens a shipped product or architecture decision.
