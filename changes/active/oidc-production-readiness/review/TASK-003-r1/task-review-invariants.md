# TASK-003 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md` (revision 5)
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md` at the candidate
- Candidate identity was rechecked after source inspection and remained unchanged.

## Invariant trace

The browser flow is produced by `ServerSessions.begin`, stored as a hash in tenant login state, selected across RLS only by `auth_login_completion_tenant`, and deleted in the same tenant transaction that inserts the browser-session row. The raw browser session id is returned once and only its digest is stored. `BrowserSessions::current` resolves the tenant from that digest, locks the RLS row, and performs refresh/API-key renewal inside the same transaction; sibling replicas therefore serialize on the row. The BFF checks the server-returned tenant against the path tenant before projecting session context, and ordinary `/v1` authorization remains the final permission boundary. Logout, renewal refusal, and session revocation wipe the row's sealed values.

The shared producers and sinks also expose two unmet invariants: the existing sealing-key rewrap owner does not inventory the new long-lived browser-session ciphertext, and the private channel sends its service credential and session authority to an upstream URL that is allowed—and tested—to be cleartext HTTP.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: tenant settings project the server-owned connection contract without cross-tenant administration | `settings/+page.server.ts:27-141` calls the existing authenticated `/v1/identity/oidc/*` routes through session authority; hooks bind the path tenant from the server-returned session | The real journey proves redacted read, authorized stage/deactivate, denied reader, and wrong-tenant refusal, but not the exposed test/activate/remove actions | **FAIL** — INVREV-003 |
| REQ-005: all recoverable browser credentials are sealed and sealing-key rotation cannot strand sessions | `auth_browser_sessions` stores access, refresh/API-key, and CSRF envelopes; `BrowserSessions` seals/opens them through `SealingKeyring` | No browser-session missing-key or K1-to-K2 retirement proof exists; the existing rewrap journey covers provider stores, not this new table | **FAIL** — INVREV-002 |
| REQ-006: canonical tenant entry and callback derive authority only from server state | `/t/[tenantKey]/login`, fixed `/login/complete`, hashed flow cookie, and RLS redemption implement the route and binding | Real Keycloak login covers missing, forged, mismatched, and replayed flow cookies | PASS |
| REQ-009: production BFF session is replica-safe, token-free in browser data, CSRF/tenant/expiry bound, and carried over the approved private channel | Postgres row lock, opaque cookies, safe view, `checkAction`, server-side authority fetch, logout and tenant switch exist | Two production BFF processes prove shared state, concurrency, CSRF, tenant mismatch, logout, and no browser token; the harness uses HTTP for the channel | **FAIL** — INVREV-001 |
| REQ-010: OIDC-off entry uses an existing Wyrd credential and no password store | `exchange_api_key` reuses `ExchangeApiKey`, seals the presented key, and disables routine API-key entry when SSO is active | `OIDC-off credential UI` exercises reader/admin, invalid and wrong-tenant keys, no mock flag, permissions and logout | PASS |
| REQ-015: tenant switching resolves an independent target-tenant session or starts target login | `ServerSessions.switch` reads the target cookie independently and never rebinds the current id | Journey proves the no-target-session branch and wrong-tenant cookie refusal, but never establishes two tenant SSO sessions in one browser or proves the existing-target-session branch | **FAIL** — INVREV-003 |
| REQ-016: inactive/replaced connection cannot renew; bounded current access is not rebound | OIDC renewal reuses the connection-bound `RefreshTokens` owner under the session row lock; refusal revokes the browser row | 30-second access TTL forces renewal, and deactivation ends both tested sessions | PASS |
| REQ-018 / AC-009: browser-session configuration and sealing behavior agree with operator guidance | UI truthfully separates production and local mock authentication; generated callback schema agrees with source | Existing authentication guide still says an API-key-only deployment needs no sealing key and that a zero provider-secret rewrap report plus two-minute wait makes old-key retirement safe, despite the new 8/12-hour sealed session rows | **FAIL** — INVREV-002 |
| INV-001: path, headers, browser state, and unverified provider data cannot select effective tenant after login starts | Flow/session digest selects tenant through narrow SECURITY DEFINER lookup, then RLS; BFF compares server tenant with path tenant | Real mismatched-flow and cross-tenant-cookie refusals | PASS |
| INV-003: platform, tenant-user, and workload planes remain distinct | Browser SSO consumes User refresh authority; OIDC-off reuses a tenant API-key exchange; normal `/v1` authorization still decides permissions | Reader/admin and cross-tenant refusals pass; no platform identity path was added | PASS |
| INV-005: UI projects server-owned identity and permissions | Server returns tenant/principal/grants from its verified access token; UI performs no durable mapping | Real mapped admin/reader path and Wyrd route denial | PASS |
| AC-001: real OIDC-off UI works without IdP or mock authentication | API-key session path and production hook | Real production BFF journey | PASS |
| AC-002: real provider UI login maps roles and proves allow/deny | Keycloak callback, server-owned issuance, settings calls | Alice admin allow and Bob reader deny | PASS |
| AC-003: two tenants with different active providers are concurrently configured, logged in, used, mutated/removed, and cross-tenant refused | Per-tenant cookies and target re-read can represent the behavior | Candidate substitutes one SSO tenant plus one OIDC-off tenant; it does not run two active provider sessions, provider-specific mutations/removal, same-issuer cross-tenant refusal, or the successful switch-to-existing-session branch | **FAIL** — INVREV-003 |
| AC-006: provider management and replacement are usable from tenant settings | All five settings actions are wired | Only stage and deactivate cross the real UI boundary | **FAIL** — INVREV-003 |
| AC-007: browser binding, two replicas, inactive renewal, absent/rotated key, and private transport fail closed | Flow replay, row locking, renewal cutoff, BFF key admission and sealing exist | Binding/replica/cutoff checks pass, but rotation omits browser rows, missing-key browser creation is unproved, and the channel is exercised over HTTP | **FAIL** — INVREV-001, INVREV-002 |
| Prohibited changes: no browser-held Wyrd token, second role mapper, local password store, production mock dependency, or commercial stub | Access token exists only in the BFF-to-server request; provider groups remain server-owned; local mock remains behind `localAuthEnabled` | Leak checks cover HTML/data/redirects/cookies on both replicas | PASS |

## Proposed findings

### INVREV-001 — VIOLATION: the BFF credential and session authority cross a channel that permits cleartext HTTP

- **Violated obligation:** TASK-003's packet-local contract requires internal BFF session operations to use the deployment-wide service key **over TLS** on a private, non-public route. REQ-005 and REQ-009 require the API key, opaque session id, and Wyrd access authority to remain protected server-side.
- **Exact location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:3-6`; `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:75-84, 171-180, 303-329`; `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:72-93, 201-210`.
- **Evidence:** `serverUrl()` accepts an arbitrary `WYRD_SERVER_URL` and defaults to `http://127.0.0.1:8080`. `ServerSessions.channel` sends the raw BFF key in `x-wyrd-bff-key`; the same channel carries an API key, raw session id, CSRF token, and Wyrd access token responses. No production-path scheme check requires HTTPS. The claimed production journey starts both BFFs against the test server's `http://` base URL, so it positively proves the unapproved transport rather than TLS.
- **Observable consequence:** A production configuration that points `WYRD_SERVER_URL` at a cleartext pod/service address sends the deployment BFF credential and recoverable session authority without the transport control fixed by the task. Network privacy alone is not identity or confidentiality, and possession of the BFF key admits every internal session operation.
- **Required testable correction:** Reuse the existing server-origin boundary but make the production BFF session channel require an authenticated TLS origin before sending any secret. Preserve a narrowly explicit local-development/test mechanism only if the approved authority permits it; do not add a second HTTP client or credential path. Exercise the existing production BFF journey against TLS and assert that a non-approved cleartext origin is refused before a request is sent.

### INVREV-002 — INCORRECT: sealing-key retirement ignores every browser-session ciphertext

- **Violated obligation:** REQ-005, REQ-018, AC-007, and the task's OIDC-off/session contract require the deployment keyring to protect recoverable browser credentials, allow rotation without stranding existing sessions, refuse affected use when key material is absent, and keep operator guidance truthful.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql:8-18, 28-35`; `crates/wyrd/wyrd-auth/src/sealing.rs:1-29, 73-118`; `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:419-466`; `docs/src/content/docs/self-hosting/authentication.svx:63-88`; browser consumption at `crates/wyrd/wyrd-auth/src/browser_sessions.rs:453-505`.
- **Evidence:** Each live browser row holds up to four sealing-key envelopes and lasts eight or twelve hours. The existing `SealedSecretRewrap` walks only `HumanConnections`, `TrustedIssuers`, and the platform connection; `SealedSecretTable::ALL` has no browser-session store. It can therefore report `remaining = 0` while live K1 browser rows still exist. The runbook then says no stored secret needs K1 and authorizes retirement after only the two-minute login-completion window. Once K1 is removed, `open_text` refuses those sessions and renewal/logout cannot open their credentials. Keyless boot inventory likewise does not see the new ciphertext. No candidate test creates browser sessions under K1 and proves read/renew/logout after K2-only restart.
- **Observable consequence:** Following the documented successful rotation can terminate all still-live OIDC and OIDC-off browser sessions and can leave their refresh families unrevoked. It also makes the runbook's retirement proof false for the newly introduced durable store.
- **Required testable correction:** Extend the existing `SealedSecretRewrap` owner and its operator SQL—not a second rotation engine—to inventory and safely CAS-rewrap every live browser-session envelope while preserving concurrent renewal/logout. Its `remaining`/keyless result must include those rows. Update the same runbook to describe browser-session key ownership and the true retirement condition. Add one focused K1→K2+K1→K2-only proof covering both OIDC refresh and API-key modes, including read/renew/logout, plus missing-key refusal with no credential exposure.

### INVREV-003 — MISSING: the required browser journey does not close multi-provider switching or the complete settings projection

- **Violated obligation:** TASK-003 scenarios 2 and 3, REQ-003/015, AC-003/006, and the repository user-journey rule require the shipped browser surface to prove an independent target-tenant session and the connection-management actions it exposes.
- **Exact location:** exposed settings actions at `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/settings/+page.server.ts:96-141`; switch implementation at `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:261-277`; journey coverage at `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:272-279, 302-309, 312-375`; harness topology at `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:201-249`.
- **Evidence:** The harness activates one Keycloak tenant and creates one OIDC-off tenant. The switch assertion covers only the absence of a target session. The journey never signs the same browser into two provider-backed tenants, never takes the successful target-session branch, and never exercises the UI's `test`, `activate`, or `remove` actions. The claimed AC-003 evidence narrows the approved two-active-provider journey to a cross-named cookie and an API-key tenant, while claimed AC-006 evidence stops at staging.
- **Observable consequence:** The required lane can remain green if the successful tenant switch, a second provider-backed session, or the UI request mappings for test/activate/remove regress. Server API tests do not exercise those BFF form and cookie seams.
- **Required testable correction:** Extend the existing two-BFF HTTP journey and existing provider setup; do not add another harness. Establish independent sessions for two active provider tenants in one browser, prove switching to the already-authenticated target and wrong/same-issuer cross-tenant refusal, and drive the remaining settings test/activate/remove actions with authorized and denied principals. Keep the existing stage/deactivate and OIDC-off checks.

## Verification assessment

- Reviewed the complete base-to-candidate diff and all changed runtime owners, sibling callers/writers, migration, BFF routes/hooks/actions, and committed journey sources.
- `git diff --check 63c5bffc93cd2f7b5ed558e610a213efcc34fd49..4e0ca8d2ecc2940724861cf6884b68f9adf65464` passed.
- I did not rerun Cargo-backed lanes as directed. The task records passing focused UI tests, the filtered and unfiltered identity journeys, UI unit/check lanes, format, lints, codegen, and `test:wyrd`; those results do not exercise the three gaps above.
- CodeGraph was unavailable because this repository has no `.codegraph/` directory.

## Overall result

**FAIL**

The candidate preserves tenant derivation, CSRF, row-lock renewal, permission, and browser-token invariants, but it does not satisfy the approved private-transport contract, makes the existing sealing-key retirement proof unsound for the new session store, and leaves required multi-provider/switch/settings journey branches unverified.
