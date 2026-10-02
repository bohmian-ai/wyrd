# TASK-003 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md` at the candidate

The complete cumulative diff was reviewed. The candidate remained the checked-out `HEAD` throughout this pass. The task's Implementation Evidence and command results were treated as claims; no Cargo-backed verification lane was rerun.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: project redacted tenant connection administration in settings without cross-tenant administration | `routes/t/[tenantKey]/settings/+page.server.ts:27-142` projects the existing `/v1/identity/oidc/*` contract through the session authority; `+page.svelte:35-126` renders redacted views and actions | Claimed real-HTTP journey proves reader denial, admin staging, and admin deactivation; the server journeys cover the underlying lifecycle contract | PASS |
| REQ-005: seal every recoverable browser credential with the deployment keyring; missing keys fail closed; rotation must not strand stored sessions | `browser_sessions.rs:230-242,292-304` seals access, refresh/bootstrap, and CSRF material and `require_keyring` refuses creation/use without a key | No browser-session absent/retained/rotated-key journey exists; the existing rewrap owner walks only provider-secret tables | **FAIL (BEH-003-03)** |
| REQ-006: canonical `/t/{tenantKey}/login`, generic SSO only when active, deployment-controlled callback and fixed completion route | `login/+page.server.ts:14-45`, `server-sessions.ts:96-168`, and `login/complete/+server.ts:1-19`; callback redirect is derived from configured public origin | Claimed Keycloak HTTP journey follows the shown callback and completes on the other BFF replica | PASS |
| REQ-009: replica-safe opaque session, safe browser metadata, one-use flow redemption, CSRF/expiry/tenant binding, private authenticated channel, logout | Postgres-backed `BrowserSessions`, `/internal/bff/v1/*`, Secure/HttpOnly/SameSite cookies, `checkAction`, and session revocation implement the main path | Claimed two-replica journey covers cookie attributes, flow mismatch/replay, service-key refusal, CSRF, cross-tenant cookie refusal, and logout | **FAIL (BEH-003-02, BEH-003-04)** |
| REQ-010: OIDC-off UI entry reuses existing API-key exchange with one indistinguishable invalid-credential behavior | `BrowserSessions::exchange_api_key` exchanges and seals an existing tenant key and the login action is disabled while SSO is active | Claimed HTTP journey covers malformed, wrong-tenant, reader, and admin keys by status only | **FAIL (BEH-003-01)** |
| REQ-015: each tenant requires its own verified session/login; switching cannot rebind a current session | `ServerSessions.switch` reads the target tenant's own cookie and server record before selecting it | Claimed journey proves direct switch to a tenant without a session reaches login | **FAIL (BEH-003-02, BEH-003-05)** |
| REQ-016: inactive/replaced connection refuses renewal while already-issued authority remains bounded by five minutes | `BrowserSessions::renew` routes SSO renewal through `RefreshTokens`; a refused renewal revokes only the browser session | Claimed 30-second-token journey makes deactivation hit renewal immediately and proves both sessions end | PASS |
| REQ-018 / AC-009 task-local UI projection: production UI no longer advertises only mock authentication; generated callback contract accepts real provider response parameters | production login/settings surfaces and regenerated `CallbackQuery` schema; mock identity remains `dev`-gated | Claimed production `node build` journey, UI check, and `codegen:check` | PASS (the full operator documentation is explicitly owned by dependent TASK-005) |
| INV-001: browser/path data cannot select effective tenant identity after login starts | server session lookup returns the tenant and hooks require equality with the path before API use | Cross-tenant session ids are refused, but the visible tenant selector itself is populated from unverified cookie names | **FAIL (BEH-003-02)** |
| INV-003: platform, tenant-user, and workload planes remain distinct | browser sessions mint only through tenant refresh/API-key issuance; no platform issuer or group-to-permission path was added | Claimed admin/reader allow/deny journey | PASS |
| INV-005: UI projects server-owned identity and permissions, with no durable role mapper | `BrowserSessionView` derives roles/permissions from a verified Wyrd token; settings calls the normal server API with server-issued authority | Claimed reader/admin permission behavior | PASS |
| AC-001: OIDC-off self-hosted UI with an existing Wyrd credential and no IdP/mock flag | API-key browser-session mode and tenant login action | Claimed production-build OIDC-off journey | PASS, subject to BEH-003-01 and BEH-003-03 |
| AC-002: real provider, exact callback, UI login, mapped role, allowed and denied calls | Keycloak-backed SSO session plus settings API projection | Claimed alice/admin allowed and bob/reader denied journey | PASS |
| AC-003: browser journey with two concurrent SSO tenants/providers, separate sessions and mutations, wrong-tenant and same-issuer refusal | Session storage and cookie naming are tenant-scoped, but the UI host seeds only one SSO tenant; the second tenant is OIDC-off | Existing server-only journeys do not exercise the new BFF boundary for the required two-provider/two-tenant scenario | **FAIL (BEH-003-05)** |
| AC-006: UI/provider-switch journey tests and activates a replacement, preserves recovery, and proves no authority/email inheritance | Settings exposes stage/test/activate/remove; underlying server owner implements replacement | UI journey only stages a candidate and deactivates an active connection; no UI test/activate/remove or replacement login occurs | **FAIL (BEH-003-05)** |
| AC-007: BFF fault/security evidence includes replica behavior, flow binding, unauthorized caller, replay, inactive connection, and absent/rotated sealing keys | Main negative paths exist; stored browser-session ciphertext is not part of the rotation pass | Claimed journey covers replicas/flow/service key/deactivation, but not browser-session key absence/rotation | **FAIL (BEH-003-03)** |
| Prohibited changes/non-goals: no UI role mapper, browser bearer storage, password authority, production mock dependency, or untrusted-browser tenant selector | No role mapper, password database, or browser token storage was added; mock auth is production-disabled | Secret-leak assertions cover pages, data responses, and redirects | **FAIL only for the explicit tenant-selector prohibition (BEH-003-02)** |

## Proposed findings

### BEH-003-01 — VIOLATION: browser API-key refusals bypass the fixed-cost verification owner

- **Violated obligation:** REQ-010's use of the existing API-key exchange and the task's requirement that unusable credentials receive one indistinguishable refusal; `architecture/wyrd-security-posture.md` requires every invalid tenant API-key condition to perform exactly one verification.
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:268-280`.
- **Evidence:** `exchange_api_key` parses the presented key and returns `ApiKeyInvalid` immediately for malformed input or a route/key tenant mismatch. Only a syntactically valid key whose embedded tenant equals the resolved route tenant reaches `ExchangeApiKey::execute` at lines 281-290. The shared owner explicitly prevents this distinction: `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:188-229` always runs `verify_presented`, including malformed, unknown-prefix, and cross-tenant refusals; the public token route supplies the same dummy verification for an input that cannot open a tenant connection (`wyrd-server/src/components/auth/routes.rs:135-155`).
- **Observable consequence:** although the BFF maps responses to the same `401`, malformed/wrong-tenant inputs return without Argon2 while a correct-tenant unknown/live prefix pays the verification cost. A caller can distinguish credential classes by timing, contrary to the existing API-key trust-boundary invariant.
- **Required correction:** route every browser-login credential refusal through the existing fixed-cost verification mechanism exactly once. Preserve route/key tenant equality before session creation, but do not perform a free parse or tenant-mismatch return outside the shared verification owner.
- **Focused closure proof:** exercise malformed, wrong-route-tenant, unknown-prefix, wrong-secret, expired/revoked, and valid browser-login keys and assert the credential-verification counter advances exactly once for every case while all invalid public responses remain identical.

### BEH-003-02 — VIOLATION: the tenant chooser is populated from untrusted cookie names

- **Violated obligation:** the task explicitly prohibits "a tenant selector based on untrusted browser data"; REQ-015 requires tenant switching to revalidate membership/session state.
- **Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:279-295` (consumed by `Shell.svelte:68-72` and `TenantChooser.svelte`).
- **Evidence:** `metadata` enumerates every cookie whose name begins `wyrd_session_`, strips the prefix, and publishes each syntactically valid suffix as a tenant option without reading the corresponding server session. The comment acknowledges these are cookie hints. `switch` later validates a selected target, so the forged option does not become authority, but the selector itself is still browser-derived—the precise prohibited shape.
- **Observable consequence:** a forged `wyrd_session_victim=<anything>` cookie makes `victim` appear in the signed-in tenant chooser even when no server session or membership exists. The UI presents attacker-controlled tenant state and only discovers the forgery after selection.
- **Required correction:** derive switch choices only from server-verified session/membership data. Cookie names may be lookup hints, but each option must be resolved and tenant-bound by the server before it enters page metadata; invalid hints must be cleared or omitted. Do not add a second membership store in the UI.
- **Focused closure proof:** with one valid current session plus forged, expired, and cross-tenant-named cookies, load a real page and prove only server-verified tenant choices render, invalid cookies do not render, and selecting a verified second session works across replicas.

### BEH-003-03 — MISSING: browser-session ciphertext is outside deployment sealing-key rotation

- **Violated obligation:** REQ-005 requires one deployment keyring to protect recoverable browser-session credentials and requires operators to rotate it without making existing sessions permanently unusable; AC-007 requires absent/rotated sealing-key evidence.
- **Location:** browser ciphertext is written in `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-242,292-304`; rotation is owned by `crates/wyrd/wyrd-auth/src/sealing.rs:74-120` and its table list in `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:421-468`.
- **Evidence:** `SealedSecretTable::ALL` contains only `HumanConnections` and `TrustedIssuers`, plus the separate platform connection. It never reads or rewrites `wyrd.auth_browser_sessions`. Lazy token renewal does not close the gap: it reseals access/refresh tokens but leaves `csrf_token_sealed` and an API-key session's `api_key_sealed` under the old key. After the old retained key is removed, `BrowserSessions::current/read` maps the open failure to an ended session (`browser_sessions.rs:453-455,689-699`), and OIDC logout can no longer open and revoke the refresh family (`browser_sessions.rs:406-415`). No candidate test rotates a live browser session.
- **Observable consequence:** the documented provider-secret rewrap can report `remaining == 0`, telling an operator the old key may be retired while live browser-session secrets still require it. Retiring it logs out those sessions, prevents clean refresh-family revocation on logout, and makes the rotation proof false for the newly introduced credential store.
- **Required correction:** extend the existing `SealedSecretRewrap` authority and its remaining-count proof to every live browser-session sealed column with compare-and-swap/concurrency safety, or retain the old key until every affected session expires and make that bounded retention part of the authoritative rotation proof. The former reuses the established rotation owner and preserves active sessions without creating a second mechanism.
- **Focused closure proof:** create live OIDC and API-key browser sessions under K1; boot/run the rotation owner with K2+K1; prove all access, refresh/API key, and CSRF envelopes move to K2, both sessions work on another replica after K1 is removed, logout retains its mode-specific behavior, and a keyless deployment refuses stored session ciphertext.

### BEH-003-04 — VIOLATION: the private BFF credential channel does not enforce TLS off loopback

- **Violated obligation:** the packet-local browser-session contract requires the BFF service-key channel to run over TLS on a private route; REQ-005/REQ-009 prohibit exposing recoverable session credentials.
- **Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:3-6` and `src/lib/server/auth/server-sessions.ts:75-93,303-329`.
- **Evidence:** every internal-channel call uses `serverUrl()`, which accepts any `WYRD_SERVER_URL` string and defaults to loopback HTTP. `ServerSessions.channel` performs no scheme/host posture check before sending the raw BFF service key, and the authority path subsequently returns/sends raw session IDs and Wyrd access tokens over that connection. Nothing in the candidate rejects `http://wyrd.internal:8080` or another non-loopback cleartext origin.
- **Observable consequence:** a production misconfiguration can transmit the deployment BFF key, browser session IDs, CSRF tokens, bootstrap credentials (during creation), and access tokens over plaintext between processes/pods. The service key authenticates the caller but does not provide confidentiality.
- **Required correction:** enforce HTTPS for non-loopback BFF-to-server origins at the configuration/boundary owner, preserving HTTP only for an explicit loopback development/test topology. Keep the route private; TLS is additive to, not a replacement for, the service key.
- **Focused closure proof:** configuration/production-boundary tests refuse a non-loopback `http://` origin before any request is sent, accept HTTPS, and retain the repository's explicit loopback journey fixture.

### BEH-003-05 — MISSING: the required browser journeys do not cover AC-003 or AC-006

- **Violated obligation:** the task says AC-001/002/003/006/007 **browser journeys** pass and instructs the real HTTP BFF journey to assert tenant settings authority; `AGENTS.md` requires each user-facing capability and its relevant negative/edge flows at journey tier.
- **Location:** `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:1-275` and `wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:185-375`.
- **Evidence:** the host creates one Keycloak-backed SSO tenant and one OIDC-off tenant. The SSO journey logs into only that one provider/tenant, and the settings coverage proves only deactivation. The OIDC-off journey proves staging only. No browser journey has two concurrent SSO tenants with different providers, same-issuer cross-tenant refusal, candidate test/activation/removal, replacement login, recovery preservation, or non-inheritance across the provider switch. Server-only TASK-002 journeys validate the underlying API but do not traverse the new BFF cookies, completion, settings actions, and switch boundary.
- **Observable consequence:** the exact multi-tenant and provider-replacement paths this UI task is required to project can regress while every claimed UI test remains green; the Implementation Evidence overstates AC-003 and AC-006 closure.
- **Required correction:** extend the existing real HTTP BFF journey (not a new mock harness) with the smallest two-SSO-tenant/provider setup that drives settings test/activate/remove, separate logins/sessions and switching, same-issuer/wrong-tenant refusal, replacement identity non-inheritance, and recovery through the public UI actions.
- **Focused closure proof:** exact named Vitest selectors for the two-tenant provider and replacement/settings scenarios through `WYRD_IDENTITY_TARGET=ui`, plus the unfiltered identity journey lane proving earlier server journeys remain present.

## Verification assessment

The claimed focused unit tests, production-build HTTP journeys, UI suite/check, format, lints, codegen, `test:wyrd`, and exact Rust selector are relevant and broad. They credibly support the implemented happy paths and several negative paths. They do not close the findings above: status-only API-key assertions cannot prove fixed-cost refusal; direct switch POSTs do not prove selector provenance; the UI journey uses one sealing key; the BFF origin is loopback HTTP; and the required AC-003/AC-006 browser topologies are absent.

No conditional follow-up is requested by this reviewer. The five findings have distinct producers and reachable consumers; BEH-003-03 consolidates all browser-session key-rotation effects under the existing rewrap owner.

## Overall result

**FAIL**
