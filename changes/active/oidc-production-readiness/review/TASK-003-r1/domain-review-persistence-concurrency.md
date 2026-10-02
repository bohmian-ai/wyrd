# Persistence, concurrency, and tenancy domain review

## Subject

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Domain: browser-session persistent state, tenant derivation and RLS, login-completion/session atomicity, renewal/logout concurrency, expiry/revocation, crash/cancellation, migration safety, and sealing-key rotation of durable session credentials

## Authority and source coverage

| Boundary | Authority | Source and caller coverage | Result |
|---|---|---|---|
| Tenant ownership and connection capabilities | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md` SQL rules; security posture tenant boundary | `WyrdPostgres::{login_completion_tenant,browser_session_tenant,tenant_directory_entry,definer_tenant}`; `BrowserSessions::{complete,exchange_api_key,current,logout,begin}`; `wyrd.auth_browser_sessions` RLS and both SECURITY DEFINER resolvers | PASS |
| Login completion to browser session | REQ-005/006/009, INV-001; task packet-local session contract | `BrowserSessions::complete`; `redeem_login_completion`; `insert_browser_session`; BFF `sessions/complete`; callback/login-state producer | PASS |
| Durable session shape and expiry | REQ-005/009/010/016 | migration `20261001000001_auth_browser_sessions.sql`; `BrowserSessionWrite`; insert, lock, rotate, revoke, and purge statements | PASS except key rotation below |
| Concurrent renewal, logout, and provider lifecycle | REQ-009/016; task requirement that replicas serialize on one session record | `BrowserSessions::{current,renew,logout}`; `LOCK_BROWSER_SESSION_SQL ... FOR UPDATE`; `RefreshTokens::execute`; human refresh-family and connection-slot lock ordering; activate/deactivate/remove callers | PASS |
| Crash/cancellation and partial progress | Task atomic completion and renewal contract; maintainer cancellation guidance | Transaction ownership in `BrowserSessions::{complete,exchange_api_key,current,logout,insert}` and `TenantConn` callees; response-loss behavior after commit | PASS |
| Sealing-key rotation of live sessions | REQ-005; AC-007 | `BrowserSessions` seal/open sites; `rotate_browser_session`; `SealedSecretRewrap`; `SealedSecretTable::ALL`; boot rewrap; current operator runbook | FAIL (`PC-001`) |
| Migration application and schema gate | Repository migration rules and task durable-session contract | migration SQL plus `pg_migration::migrations_apply_and_are_idempotent`; forced-RLS metadata assertion | PASS with verification limit below |
| User-journey proof | `AGENTS.md` §11; task verification contract | `identity_ui_e2e::production_ui_bff_journey`; `production-auth.integration.test.ts` including cross-replica renewal, replay, cross-tenant cookies, logout, and deactivation | PASS except rotated-key scenario omitted by `PC-001` |

## Persistence and concurrency assessment

- Session and completion tenant identity is derived from a high-entropy hash through narrow SECURITY DEFINER functions, after which all row work uses a tenant-scoped `TenantConn` under forced RLS. The route tenant is not accepted as session authority.
- Completion redemption and session insertion share one tenant transaction. Cancellation before commit rolls both back; a committed session contains the exact redeemed connection provenance and encrypted credentials.
- `SELECT ... FOR UPDATE` serializes all session renewal and logout mutations. Refresh-family rotation, audit, session credential rotation, and browser-session revocation remain in the caller-owned transaction. A losing replica re-reads the committed row, and connection lifecycle changes serialize through the existing human-connection slot lock.
- Renewal failure does not fall back to another credential or tenant. Refused renewal wipes the browser-session ciphertexts in the same transaction; logout also locks the row and revokes the current human refresh leaf before wiping the session. The current leaf is the only unrevoked member of its family, so this ends the chain.
- Expiry comparisons and fixed lifetimes are derived from PostgreSQL time. Expired rows are unusable before eventual tenant-local purge. Cancellation before a renewal or logout commit restores the whole transaction; cancellation after commit may lose a response but cannot expose a second credential or partially rotate durable state.

## Material finding

### PC-001 — INCORRECT: the canonical sealing-key rotation omits durable browser-session ciphertexts

- **Violated obligation:** REQ-005 requires operators to rotate the deployment keyring without making existing browser sessions permanently unusable and requires that rotation procedure to be documented and tested; AC-007 specifically requires rotated-sealing-key evidence. TASK-003 maps both obligations and introduces the durable session ciphertexts.
- **Location:** `crates/wyrd/wyrd-auth/src/sealing.rs:73-118`; `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:419-468`; `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-242,292-304,321-323,453-455,479-494,541-545`; `docs/src/content/docs/self-hosting/authentication.svx:78-88`.
- **Evidence:** `wyrd.auth_browser_sessions` stores `access_token_sealed`, `refresh_token_sealed`, `api_key_sealed`, and `csrf_token_sealed`, but `SealedSecretRewrap::run` walks only `SealedSecretTable::ALL`, whose two variants are human connections and trusted issuers. Session renewal reseals only the access token and optional refresh token; it never reseals the bootstrap API key or CSRF token. The canonical runbook says that after the provider-secret pass reports `remaining = 0` and the two-minute login-completion overlap elapses, no stored secret needs the old key and the operator may remove it. Live browser sessions last eight or twelve hours and are neither counted nor rewrapped. The candidate's AC-007 evidence exercises two replicas and connection deactivation but no sealing-key rotation.
- **Observable consequence:** following the repository's rotation procedure can retire K1 while live K1 sessions remain. After the final K2-only rollout, `SessionRead` cannot open the K1 CSRF ciphertext and ends both SSO and API-key sessions; an API-key session also cannot open its K1 bootstrap key for renewal. Even an SSO session used during the overlap retains a K1 CSRF ciphertext because rotation updates only its access/refresh pair. Thus the verification pass can report zero while existing sessions still depend on K1.
- **Required correction:** extend the existing `SealedSecretRewrap` authority, rather than adding a second rotation workflow, so its compare-and-swap scan and `remaining` result cover every non-null sealed column of live browser-session rows. Preserve the short-lived login-completion exemption. Add focused Postgres proof for both session modes: create under K1, run the K2-with-retained-K1 rewrap, then use a K2-only `BrowserSessions` owner to read/obtain authority and exercise renewal/logout. Extend the real BFF evidence or an equivalent production-shaped session journey so AC-007 records a session surviving the supported key rotation. The post-roll `remaining = 0` statement must then be true for provider and browser-session credentials together.

## Verification limits

- Per the review assignment, no Cargo-backed command was rerun. I inspected the candidate's committed verification record and the named test sources.
- The real BFF journey does issue concurrent requests from two replicas with a 30-second access-token TTL, below the one-minute renewal margin. That proves row-lock serialization and absence of refresh replay, but both serialized requests may renew in succession; it does not by itself prove the documented optimization that the loser observes a freshly usable winner token. This is not a correctness finding because the transaction and family-lock path remains safe.
- The migration test asserts application/idempotence and forced-RLS presence. Cross-tenant refusal is exercised through the real BFF journey rather than a dedicated SQL test.
- No candidate test covers a live SSO or API-key browser session across sealing-key retirement; that missing proof corresponds to `PC-001`.

## Overall result

**FAIL** — tenant derivation, transactional session lifecycle, row-lock concurrency, and crash/cancellation behavior are sound, but the canonical key-rotation owner and its zero-remaining proof omit the new durable browser-session secrets. One bounded persistence correction is required.
