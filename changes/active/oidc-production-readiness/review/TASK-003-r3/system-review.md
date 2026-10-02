# TASK-003 r3 system-resilience review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `SPEC-oidc-production-readiness`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`
- Cumulative range reviewed: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49..8289fa298ed33d21f2568558bc0a02905fd0b218`
- Latest remediation locator inspected: `622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218`

The candidate remained `8289fa298ed33d21f2568558bc0a02905fd0b218` throughout this review.

## Deployed-path coverage

| Runtime path | Deployment and lifecycle ownership | Failure and recovery evidence | Assessment |
|---|---|---|---|
| Production login and callback | Browser -> either production Node BFF -> public Wyrd login route -> Postgres login state -> screened IdP discovery, authorization, token, and JWKS endpoints -> common callback -> fixed BFF completion route | `server-sessions.ts:99-171`; `wyrd-auth/src/callback.rs:76-215`; `wyrd-server/src/components/auth/routes.rs:349-385`. State is committed consumed before provider IO, so timeout, provider outage, issuer refusal, or invalid token spends only that attempt and produces no completion. A new login starts a new state. | Fail-closed at the request boundary. IdP outage stops new login but does not affect already-issued browser authority, OIDC-off entry, or independent machine authentication. |
| Interactive connection test | Authorized settings action -> normal tenant connection API -> `HumanConnections::begin_test` -> durable test-bound login state -> same provider callback and ID-token verification -> `stamp_test_sign_in` | `settings/+page.server.ts:119-135`; `connections.rs:316-398,400-521`; `callback.rs:241-290`; `auth_login_state` migration. A server/BFF restart after the state commit loses nothing. A crash or provider failure after callback consumption requires a fresh test, creates no User/session/credential, and cannot stamp a changed revision. The final authority recheck, audit, slot lock, and stamp share one tenant transaction. | Recovery and failure boundaries match the human direction. Candidate replacement, tester deauthorization, audit failure, or provider outage refuses only the test attempt and leaves the candidate untested. |
| Session completion and replica use | Either BFF -> TLS-protected private `/internal/bff/v1/*` route -> `BrowserSessions` -> tenant RLS transaction -> Postgres; authority returns only to the BFF for one ordinary `/v1` call | `bff.rs:53-100,181-327`; `browser_sessions.rs:186-390`; `server-sessions.ts:78-97,186-217,312-347`. No process-local production identity exists. A BFF or Wyrd restart loses no committed session. A completion crash before commit leaves it redeemable; an uncertain response after commit can orphan one bounded session but cannot replay or expose it. | Cross-replica persistence is sound. Non-`401` upstream failures retain the cookie, allowing retry after dependency recovery. |
| Concurrent renewal, logout, and connection cutoff | `BrowserSessions::current` locks the browser row; refresh/API-key issuance and connection lifecycle use their existing family/slot owners; logout locks the same row | `browser_sessions.rs:393-587`; `browser_sessions` SQL `FOR UPDATE`; `connections.rs` slot-lock paths. A successful rotation and browser-row update share the caller transaction. Concurrent replicas re-read the committed winner. Refused proactive renewal rolls back and serves the issued token until exact expiry. | Locking and exact-expiry behavior are sound for semantic credential refusals. Infrastructure/audit failures are incorrectly folded into that refusal path and can commit destruction; see `SYSTEM-R3-001`. |
| Sealing-key rolling replacement | Every server boot runs the canonical cross-tenant rewrap over provider and all stored browser-session envelopes, including expired rows; exact-byte CAS handles concurrent renew/logout/rewrap | `sealing.rs:1-175`; `boot/mod.rs:1506-1550`; browser-session sealed inventory queries. The documented post-writer zero pass makes K2-only restart safe, while a keyless boot refuses any stored envelope. | Prior sealing-inventory outage is closed. Process interruption leaves completed swaps durable and a later pass resumes; a lost CAS is counted for the next pass. |
| BFF transport and rolling replicas | One BFF uses loopback HTTP; the other uses native Node fetch through a repository-CA-issued TLS terminator to the same Wyrd server | `identity_ui_e2e.rs:120-209,452-458`; `production-auth.integration.test.ts:272-414`. The second replica receives `NODE_EXTRA_CA_CERTS`, performs completion/read/authority operations over `https://localhost`, and shares only Wyrd/Postgres state with the first. | Prior TLS proof gap is closed. Certificate or endpoint failure refuses the BFF request and leaves independent Wyrd surfaces available. |
| Multi-provider switching and provider replacement | Keycloak and Dex back independent tenant sessions; settings stage/test/activate uses the same server connection authority | `identity_ui_e2e.rs:420-450`; `production-auth.integration.test.ts:481-566,568+`. Genuine callback parameters are retained while only state is substituted in the cross-provider proof. | Different-provider and same-issuer cross-tenant paths are covered without a fallback provider or session rebind. |

## Failure and recovery assessment

- **One BFF crash or rolling replacement:** another replica resolves the same opaque cookie through Wyrd/Postgres. No session authority lives in a Node process.
- **Wyrd crash or request cancellation:** open tenant transactions roll back and release browser, refresh-family, and connection locks. Committed login state, connection-test state, sessions, and credentials survive restart.
- **Postgres outage:** login, completion, read, renewal, logout, and settings operations fail at their request boundary. Existing cookies remain retryable after recovery; no mock identity or alternate tenant/provider is selected.
- **IdP outage:** new production login and an in-progress connection test fail after their one-use callback state is consumed. Existing SSO browser renewal uses Wyrd refresh authority without contacting the IdP; OIDC-off renewal uses its stored API key.
- **Connection replacement/deactivation:** new login and renewal through the retired revision stop. A refused proactive renewal preserves the current token until its stored expiry, then the session ends.
- **TLS/certificate outage:** the affected BFF cannot reach the private channel; native fetch refuses the hop. The other Wyrd client, SDK, CLI, MCP, and machine-auth surfaces remain independent.
- **Sealing-key rollout:** retained keys preserve service while replicas converge. Exact-byte CAS prevents the boot pass from overwriting concurrent session changes; the required post-writer zero pass is the retirement gate.
- **Audit/issuance outage during browser renewal:** requests fail closed, but once the current token reaches expiry the implementation misclassifies these dependency failures as credential revocation and commits destructive state. This prevents normal recovery and can amplify one shared outage across every expiring UI session.

## Affected capabilities and blast radius

The production UI depends on the BFF channel and Postgres-backed browser sessions. A single BFF loss is masked by another replica; loss of shared Wyrd/Postgres removes UI use without authorizing a fallback and leaves independently authenticated SDK, CLI, MCP, and machine traffic available. Provider outage affects new login and connection testing, not already-issued Wyrd authority.

`SYSTEM-R3-001` has fleet-wide potential: a shared audit-path or token-issuance outage near access-token expiry can durably end both SSO and OIDC-off browser sessions across tenants. Users must repeat IdP login or re-enter an operator API key after the dependency recovers; if the IdP is also unavailable, UI service does not resume from the original transient outage.

## Material proposed finding

### SYSTEM-R3-001 — Renewal dependency failures are committed as durable session revocation

- **Classification:** `INCORRECT`
- **Violated obligation:** TASK-003 requires replica-safe browser-session recovery and fail-closed dependency handling without treating an infrastructure failure as a credential or lifecycle decision. R2 explicitly requires refused proactive attempts to commit no tentative refresh/key-use mutation and says infrastructure failures remain fail closed. Repository audit authority requires an authorization decision and its audit append to commit together or both roll back.
- **Locations:**
  - `crates/wyrd/wyrd-auth/src/browser_sessions.rs:489-508,547-585`
  - `crates/wyrd/wyrd-auth/src/refresh.rs:128-164`
  - `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:166-184`
  - `crates/wyrd/wyrd-auth/src/issuance.rs:394-401,460-488`
- **Evidence:** `BrowserSessions::renew` recognizes only direct database variants as `Renewal::Failed`. Every other refresh issuance failure and every non-database API-key issuance failure becomes `Renewal::Refused`. That includes `IssuanceError::Wyrd(WyrdError::AuditUnavailable)`, signing failures, store failures, and corrupt-role failures. When the stored access token is expired, `current` handles `Renewal::Refused` by wiping the browser row and committing. On the SSO path, refresh consumption (or replay-family revocation) occurs before issuance/audit; on the API-key path, `last_used_at` is touched before issuance/audit. The final commit can therefore persist those tentative writes and browser revocation even though the required exchange audit failed. The sole focused renewal test covers a deactivated connection, a genuine semantic refusal; no test injects an audit or issuance failure at expiry.
- **Observable system consequence:** A transient audit-staging outage, signing failure, or relevant store failure at token expiry permanently logs out the browser instead of leaving the session retryable. It can also commit refresh-family/key-use state without the canonical audit event. Recovery of the failed dependency does not restore the session, amplifying a component outage into tenant-wide relogin and, for OIDC-off users, API-key re-entry.
- **Smallest testable correction:** Keep `BrowserSessions` as the owner and classify renewal outcomes by meaning. Only credential/lifecycle outcomes (`NotFound`, `Reused` after its required audit succeeds, inactive tenant/principal/connection, invalid or revoked API key) may enter the durable refusal branch. Map audit-unavailable, signing, store, role-decode, join, and other infrastructure/issuance failures to `Renewal::Failed`, allowing the open transaction to roll back and preserving the browser row for retry. Add focused Postgres tests for both SSO-refresh and API-key modes with an expired access token and a forced audit append failure: the request must fail, the session and recoverable credential must remain unrevoked, no refresh/key-use mutation may commit, and renewal must succeed after audit access is restored. Also cover a refresh-reuse containment whose audit append fails so family revocation cannot commit without its audit.

## Proof assessment

The task evidence records green UI tests/typecheck; filtered and unfiltered real identity journeys; `test:wyrd`; `test:sql`; code generation; tenant isolation; docs; format; lints; and `git diff --check`. This review did not rerun builds or tests, as directed.

Static inspection confirms credible proof for two production Node BFF processes, a trusted TLS private hop, Postgres-backed cross-replica sessions, exact-expiry connection cutoff, logout, OIDC-off entry, Keycloak/Dex tenant switching, standards-shaped mixed callbacks, interactive candidate testing, sealing-key CAS and K2-only restart. The recorded proof does not exercise renewal audit/issuance failure classification, and source establishes the destructive path directly; this is a finding rather than a verification limit.

Residual verification limits that do not establish source defects are direct Postgres outage/recovery and mixed old/new BFF service-key rollout. Source shows bounded fail-closed behavior and the two-hash overlap mechanism for those paths.

## Overall result

**FAIL**

The candidate closes the prior TLS, sealing-inventory, multi-provider, chooser-fanout, upstream-configuration, and exact-expiry findings, and its connection-test flow has bounded crash/restart behavior. Browser renewal still converts audit and other issuance infrastructure failures into committed session revocation, so the deployed UI does not recover correctly from a reachable shared dependency outage.
