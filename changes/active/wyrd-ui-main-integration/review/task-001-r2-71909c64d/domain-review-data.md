# Independent data and durability review

Result: **FAIL** — one reachable regression remains at the R2 logout correction boundary. The original relabeled-cookie failure is corrected, but the chosen lookup also prevents durable logout during tenant suspension. This is a proposed finding for independent validation, not a new approved product decision.

## Subject and reviewed boundary

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `8651b96fd0c049ab5e63b0606c98326af1fc8848`; candidate `71909c64d888caac85bcb184bbf2aba7c25e9e0b`; R2 navigation base `2741395a33d5e1e5ee5d7022916627917254a663`. HEAD matched the candidate. Source was not modified; the pre-existing trace working-tree changes were excluded. No CodeGraph index exists.

Reviewed the persistent browser-session boundary: browser input and selected tenant, opaque identifier generation and digest storage, SQL capability/RLS and transaction ownership, lifetime and credential/connection binding, callback single use and atomic completion, current authority, selected logout, restart and response-delivery consequences. Cumulative session changes, not merely the last correction, supplied the subject. Packaging, old domain UI deletion and unrelated auth implementation were considered only where they reach this boundary; other reviewers own their full acceptance.

Inputs read include spec revision 10, original TASK-001 and retained TASK-002, R1 remediation and execution evidence, both earlier verdict/validation ledgers, R2 remediation and its execution evidence, and this review's subject/deferrals. No fresh peer report was read.

## Authority and source coverage

| Boundary | Governing authority | Source inspected and conclusion |
|---|---|---|
| Durable ownership and existing owners | AGENTS §§2–3, 5, 9; agent rules; design Runtime identity/Human sign-in; security posture First-party browser sessions; architecture patterns; spec REQ-033, INV-002/004 | Full `wyrd-auth/src/browser_sessions.rs`; callback `execute`, `complete`, `verify_and_finish`, `finish_id_token_exchange`, `bound_connection`, `fence_bound_connection`; `HumanConnections::begin_bound`, activation/deactivation; issuer verification/resolution and grant shaping; API-key verification/authentication. BrowserSessions composes existing connection and issuance owners rather than replicating provider verification or durable principal resolution. New session rows are expressly authorized; refresh rows are not reused as sessions. |
| SQL capabilities, tenant RLS and transaction ownership | Agent rules TenantConn/OperatorPool and caller-owned commit; REQ-033 persistence | Full new migration and `queries/auth/browser_sessions.rs`; full production login-state queries; `WyrdPostgres::{tenant_conn,resolve_tenant_slug,tenant_directory_entry,login_state_tenant}`; platform resolver and tenant directory query; relevant platform migrations and TenantConn acquire path. Session reads/writes take TenantConn, use forced RLS, and leave transaction completion to their caller. Platform lookup uses existing OperatorPool/definer boundaries. |
| Digest, time and credential liveness | REQ-033 ownership/current authority; AC-012; security posture browser-session record rules | Random 32-byte secret; SHA-256 of entire `{tenant}.{secret}` value; schema digest width/authority exclusivity/12-hour check; Postgres statement timestamps for expiry/revocation/purge; recovery-key join checks identity, revocation and expiry; connection sessions recheck exact active revision; issuer refuses inactive tenant/principal and resolves current grants. BrowserRecovery preserves API-key grant shaping while avoiding runtime-activity writes on resolution. |
| Public transport, enumeration and mutation | REQ-033 browser HTTP/current authority; REQ-004; INV-002 | Full HTTP browser adapter, session/live_view/logout; callback adapter and callback dispatch; HTTP tenant-auth middleware; Node BrowserSessions/BrowserSession, hook, tenant layout, settings actions and upstream forwarder. Native public cookies now reach Rust, Node metadata uses the accepted current enumeration, and callback server errors retain catalog classification. Bearer-only siblings and bearer precedence are retained. |
| Cancellation, atomic completion and delivery | Agent rules caller transaction ownership; AGENTS documentation/testing; R2 finite delivery requirement | Login state consumes/commits before provider IO. Session, identity/role changes and device approval commit in the existing finishing transaction, fenced by family/connection slot locks. Recovery creates a fresh digest-only session before response. Commit can outlive cancellation/delivery; no replay of consumed state is promised. New docs explain fresh-login/recovery retry and idempotent logout. No new scheduler, durable cookie vault, retry controller or Node authority appears. |
| Proof and retained sibling behavior | AGENTS §11; spec AC-003/012, INV-007; R2 scope | `pg_browser_sessions` authority and tenant-API tests; browser/callback module tests; native/nginx identity journey source and production driver; metadata/forwarder unit cases. Tests cover digest/expiry/tamper/tenant mismatch/key revocation/current grants, bearer precedence/origin, connection retirement and replica use. The new suspended-tenant logout path is absent. |

Applicable workflow references read include task-review, reference router, spec-driven development, maintainer style and architecture patterns. Design/doctrine/security authority was applied to the session and identity boundary; no Bifrost data-plane behavior was changed by the reviewed session machinery.

## Existing-owner comparison

The cumulative implementation appropriately extends `AuthorizationCodeExchange`, `HumanConnections`, `TenantTokenIssuer`, API-key verification, TenantConn queries, and the existing browser/BFF request owners. Provider, nonce/PKCE/issuer verification, principal grant resolution, device approval, connection-test completion, refresh-chain containment and non-blocking audit remain with their prior owners. OAuth refresh lookup deletion removes the retired browser-only non-rotating path; CLI rotation retains its family lock, single-use consume and chain containment.

The R2 logout guard belongs at `BrowserSessions::logout`: it is the sole production caller of `revoke_browser_session`; HTTP logout is its sole Rust production caller. Both direct public native requests and Node's selected logout call converge on that handler. Enumeration separately uses `resolve` and `view` before comparing its label; recovery uses an active-tenant resolver because it admits a new credential. These are different lifecycle operations: admitting authority is not required merely to revoke an already possessed session.

An existing directory mechanism already answers the identity question needed for logout without admission: `WyrdPostgres::tenant_directory_entry(tenant)` delegates to `queries/platform/tenants.rs::tenant_by_id`, which explicitly includes suspended/failed/provisioning states and excludes soft-deleted history. It returns the authoritative slug for the embedded tenant. This avoids new SQL or a second resolver and preserves mismatch refusal.

## Proposed finding

### DATA-R2-001 — REGRESSION: successful logout during tenant suspension does not revoke the session

**Violated obligations:** REQ-033 requires selected-tenant logout to immediately revoke only that browser session; AC-012 requires logout/revocation behavior. R2 FIND-TASK-001-8 correction explicitly preserves ordinary session-only logout without requiring live principal/connection permission. Existing suspension admission must remain effective, but it does not authorize reporting successful revocation without writing it.

**Changed location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:329–335`, in `BrowserSessions::logout`. HTTP consumer: `crates/wyrd/wyrd-server/src/auth/browser.rs:516–532`.

**Evidence and reachable producer-to-consumer path:**

1. Recover or sign in while tenant Acme is active, retaining its opaque session value. The session is stored with absolute expiry and `revoked_at = NULL`.
2. Suspend Acme. `TenantTokenIssuer::resolve` correctly refuses that session while suspended. Suspension does not revoke browser rows: the session query checks expiry/revocation/key, while current tenant admission happens in the issuer. Existing `pg_browser_sessions.rs:446–455` explicitly exercises suspension as an authority refusal.
3. Send same-origin public `POST /auth/browser/logout` with `{tenant: acme}` and `wyrd_session_acme=<the matching Acme value>`. The endpoint has no live-authentication prerequisite; it passes the selected tenant and value to the lifecycle owner.
4. R2 now calls `resolve_tenant_slug`. `queries/platform/tenant_resolver.rs::resolve_by_slug` documents that suspended tenants resolve to None; `migrations/20260601000000_platform.sql:69–87` implements `WHERE slug = p_slug AND status = 'active' AND deleted_at IS NULL`.
5. Therefore `routed != Some(tenant)` at line 333 returns `Ok(())` before opening TenantConn or invoking revocation. The adapter answers 204 and clears the browser cookie. The stored digest remains unrevoked.
6. Reactivate Acme before session expiry. Present the retained pre-logout value: the unchanged row, still-valid key/connection and now-active tenant pass resolution again. A session the caller was told had logged out is usable.

**Observable consequence:** logout can be reported successful while leaving a durable session reusable after a routine temporary tenant suspension ends. This newly results from the R2 lookup: the preceding implementation routed the possessed value directly to its tenant revocation transaction. No unknown-cookie theft, cross-tenant access or claim that suspension itself must permanently revoke every session is asserted.

**Sibling/writer audit:** `revoke_browser_session` has one production caller, this lifecycle method. The handler selects by requested tenant cookie label; Node logout and public forwarding both reach it. `recover_in` legitimately needs active admission for new authentication; `view` already uses the status-independent directory. `TenantConn::acquire` binds RLS without requiring active admission, and the revoke SQL itself requires only matching digest and not-already-revoked status. No hidden writer revokes suspended tenants' browser rows. Existing tests suspend Beta and prove read refusal, then test logout only on still-active Acme, leaving this transition untested.

**Smallest testable correction:** keep the single lifecycle mutation boundary and selected-route/value check, but establish label/value identity through the existing status-independent `tenant_directory_entry` for the value's embedded tenant and compare its returned slug with `tenant_route_key`. A mismatch/unknown/deleted value still mutates nothing; a matching suspended tenant may revoke its exact digest. Preserve existing tenant RLS, digest revocation, idempotent missing/already-revoked cleanup, store-unavailable propagation, and all credential/connection/grant checks for authentication. Do not widen the active credential resolver or add principal/connection permission checks, manual SQL tenant filters, new APIs or duplicate guards in Node/HTTP.

**Focused closure proof:** extend the existing `pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable` test with active recovery → save value → suspend tenant → same-origin selected logout → reactivate → saved value refused. Confirm sibling session/key survive, while the existing relabeled-cookie case still revokes neither tenant. Use its existing repository-managed Postgres lifecycle and exact integration selector. No test was run by this reviewer.

This finding shares FIND-TASK-001-8's mutation boundary and should be reconciled with that stable finding during validation rather than automatically assigned a new FIND number.

## Closure and proof limits

| Obligation | Source assessment | Evidence limitation |
|---|---|---|
| Original digest-only, tenant-bound, absolute-expiry session authority | PASS in inspected cumulative source | SQL/HTTP cases and execution claims exist; not rerun here. |
| FIND-3 public session forwarding | Corrected in source; public production driver now enumerates and proves saved logged-out value refusal with sibling surviving | Claimed native run only; no execution by reviewer. |
| FIND-4 current metadata | Corrected in source; no stale selected fallback after 401/503 or sibling-only success | Fetch-stub proof claims inspected, not rerun. |
| FIND-5 backend callback classification | Corrected in source; 5xx handled before directory lookup with no-store and cleared login binding | Focused adapter test inspected; not rerun. |
| FIND-8 relabeled selected logout | Original mismatch corrected; regression DATA-R2-001 prevents complete lifecycle closure | Required new transition not covered by current proof. |
| FIND-9 committed-but-undelivered semantics | Finite source documentation corrected for reviewed lifecycle boundary | No requirement to add runtime reconciliation; no execution claimed. |
| Provider-replacement expectation 200 → 401 | Independently corroborated from source: activation retires the actor's exact bound connection; post-action tenant layout enumerates current sessions; Rust refuses the retired revision; corrected metadata propagates 401 | R2 records trace and an independent diagnostician's conclusion. No separate original diagnostician execution artifact/path was supplied, so this review does not claim to have inspected one. No source reason to restore stale metadata or 200 was found. |
| Replicas/restart/delivery | Durable authority is Postgres; no per-process session cache. Native fixture uses two Rust/Node replicas and fresh BFF; nginx test writes whole-container restart assertions | Runtime claims are bounded to recorded native evidence; nginx/Linux run is deferred, not a pass. |

Only read-only source/diff/status inspection was performed. No tests, builds, services, image attempts or shared identity mutation were run. Full official image matrix, Linux/nginx execution, signed source-free native runtime and human visual inspection remain **DEFERRED/UNVERIFIED** under the caller's explicit scope. They do not supply additional implementation findings or block this source review by themselves.

Skipped execution and unrelated platform/UI acceptance audits; the material risk is the durable logout regression above.
