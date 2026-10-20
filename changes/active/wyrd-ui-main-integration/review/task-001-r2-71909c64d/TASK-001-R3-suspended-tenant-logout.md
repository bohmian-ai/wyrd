---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 10
requirements: [REQ-033, AC-012]
depends_on: [TASK-001-R2]
parent_task: TASK-001
remediates: [FIND-TASK-001-8]
---

# Revoke the selected session when its tenant is suspended

## Authority and subject

Approved [spec revision 10](../../spec.md), [original task](../../tasks/TASK-001-distribution-login.md), [R2 task and evidence](../task-001-r1-2741395a3/TASK-001-R2-session-boundary-fixes.md), current [verdict](verdict.md) and [independently validated finding](findings-validation.md) govern this task. Execute with `$wyrd-implement`.

Original cumulative base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
Prior R1 reviewed candidate/R2 base: `2741395a33d5e1e5ee5d7022916627917254a663`.
Current reviewed candidate: `71909c64d888caac85bcb184bbf2aba7c25e9e0b`.
Re-review the next candidate cumulatively, not only this fix diff.

## Diagnosis — FIND-TASK-001-8, revised

REQ-033 requires logout to revoke the selected browser session immediately while leaving sibling sessions and credentials intact. R2 fixed relabeling a Beta value under Acme's cookie label by adding route/value binding in `wyrd-auth::BrowserSessions::logout`. However, at `crates/wyrd/wyrd-auth/src/browser_sessions.rs:329–335` it uses `resolve_tenant_slug`, an authentication admission lookup that returns no tenant while suspended.

A valid saved Acme session can therefore follow this reachable path:

1. An active tenant creates a browser session, then is temporarily suspended.
2. Same-origin `POST /auth/browser/logout` presents the matching selected tenant/cookie. The public handler does not require live authority and reaches the lifecycle method.
3. The active-only lookup returns None, so logout returns `Ok(())` before the digest revoke transaction. HTTP returns 204 and clears the browser cookie, but leaves the durable session unrevoked.
4. Reactivation before expiry restores admission. The saved value now works again because suspension changed only status, not the session/key/principal.

Evidence: `WyrdPostgres::resolve_tenant_slug` and its SQL function in `20260601000000_platform.sql` require active status; the platform status route/provisioning owner changes only status/time; `TenantConn` and session-table RLS permit exact-digest revocation without active admission. Public native forwarding and Node's root logout action both reach the same Rust handler and lifecycle owner. No other writer revokes the row on suspension. Full source locations and caller/writer trace are in the validated ledger.

The existing `pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable` test suspends Beta and reads refusal, then logs out active Acme. It never logs out the suspended tenant and resumes it. Its GREEN result proves the original mismatch fix but misses this regression. This continues FIND-8; it is not a new finding or a request to alter suspension semantics.

## Required correction and ownership

Keep the single selected-tenant mutation boundary in Rust `BrowserSessions::logout`. Use the existing `WyrdPostgres::tenant_directory_entry` for the embedded tenant's authoritative slug, compare that identity to the selected route, and retain the existing TenantConn/digest revoke/commit for a match, including a suspended tenant.

This status-independent directory read already exists in `BrowserSessions::view`; its `tenant_by_id` query includes suspended tenants and excludes deleted/unknown entries. It answers the identity question without granting admission. No new resolver, SQL query, API or persistence decision is needed. This replaces the R2 private recommendation to reuse recovery's active-only resolver; the approved logout behavior remains unchanged.

Do not delete the tenant binding, widen the shared active-only resolver, reauthorize live principal/credential/connection access before logout, or add guards in Node/HTTP. Recovery and authentication still require an active tenant. Preserve malformed/unknown/deleted/mismatching no-op cleanup, already-revoked idempotency, directory/store unavailable responses, exact Origin, cookie behavior, RLS, absolute expiry and all sibling session/key/connection behavior. No new settings, secrets, dependencies, migrations or wire/schema changes.

Expected owning write set: the Rust auth lifecycle owner and its existing HTTP/Postgres authority test in `crates/wyrd/wyrd-server/tests/integration/pg_browser_sessions.rs`. Follow repository rustdoc requirements on modified items; reversible private mechanics remain implementation-owned. Preserve unrelated uncommitted trace edits and prior review artifacts.

## Ordered proof scenarios

**Scenario 1 — suspended logout stays logged out after resume (FIND-8).** Extend the existing real HTTP/Postgres authority test: create and save a valid session, suspend its tenant, perform same-origin selected logout, observe 204 and selected-cookie clearing, reactivate before expiry, and prove replay of the saved value remains 401. Keep an untouched session/key as a positive control so unrelated invalidation cannot supply the refusal.

**RED:** run the exact focused test below with that transition and confirm failure comes from the unrevoked saved session becoming usable after resume, rather than setup or compilation. If the first failure is unexpected, rerun with tracing and record a diagnosis before changing code or assertions.

**GREEN:** correct the identity lookup at the existing logout owner and rerun the exact test. Preserve its existing cross-tenant relabel case (neither session revoked), immediate suspension refusal, active/idempotent logout, Origin refusal and unavailable behavior. No live provider or container is necessary for this durable revocation seam.

**REFACTOR:** only if passing code exposes a simpler existing-owner expression; keep the same focused proof green. Record actual RED/GREEN outcomes, commands and commit evidence in this task.

## Focused commands and acceptance

Use the existing managed Postgres setup/migrations and registered integration target:

```bash
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)"'
```

Final checks for the expected Rust owner/test change:

```bash
mise run fmt
mise run lints
mise run check:tenant-isolation
git diff --check
```

Run Cargo-backed lanes sequentially. No full identity/principal/journey suite, UI build, codegen/OpenAPI lane, image attempt or broad `gate` is required for this correction; final integrated change verification retains its own obligations.

| Finding | Acceptance |
|---|---|
| FIND-TASK-001-8 | Matching selected logout durably revokes while suspended; saved value stays refused after resume. Cross-tenant relabel still mutates neither tenant, independent session/key remains usable, and existing Origin/idempotency/unavailability behavior passes. |

Nginx execution (FIND-2), full official image matrix, signed source-free native runtime and human visual acceptance remain deferred/unverified under current caller direction. They are not added to this remediation. No new UX redirect or historical diagnostic artifact reconstruction is requested. Stop for review if the fix would require changing approved observable behavior; the validated correction requires no such decision.

## Implementation evidence

Commit `73508c3b3` on `wyrd-ui-main-integration`. Preserved trace edits remain uncommitted and untouched.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Matching selected logout durably revokes while suspended; saved value stays refused after resume | `wyrd-auth/src/browser_sessions.rs::BrowserSessions::logout` compares the route key to `WyrdPostgres::tenant_directory_entry(<value tenant>)` (the status-independent read `view` already uses) instead of the active-only `resolve_tenant_slug`; existing TenantConn digest revoke/commit unchanged | RED: extended `pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable` — trace `logout cookie does not belong to the selected tenant` on the suspended logout, then `left: 200 right: 401` after resume. GREEN: 1 passed | PASS |
| Relabel mutates neither tenant; independent session/key usable; Origin/idempotency/unavailability preserved | Same owner; no change to HTTP, Node, SQL or the shared resolver | Same test GREEN: relabel case, untouched Beta control session 200 after resume, Origin 403, repeated logout 204, key exchange, outage 503 | PASS |

Commands: `WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)"'` (RED then GREEN); `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation` PASS; `git diff --check 71909c64d..HEAD` clean.

Non-goals excluded: recovery/authentication still use the active-only resolver; no new query, setting, migration, schema or UX change. FIND-2 nginx execution, the image matrix, signed native runtime and human visual acceptance remain unverified and deferred.
