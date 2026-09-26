---
id: TASK-002-R10-HUMAN
kind: remediation
status: implemented
parent_task: TASK-002
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-005, REQ-007, REQ-009, REQ-011, REQ-013, INV-001, INV-004, AC-004, AC-007]
directed_by: human, 2026-09-26
---

# Human-directed TASK-002 drift remediation

This direction supplements the R10 documentation corrections. It is not a
validated R10 finding ledger. Preserve the original TASK-002 base and task
contract; review the cumulative candidate after implementation.

## 1. One slug resolver and only scoped SQL capabilities

Login calls `WyrdPostgres::resolve_tenant_slug`, while workload exchange and
boot call `resolve_by_slug_for_app` through `&PgPool`. Boot also passes raw
pools to issuer and binding seeding. `wyrd-auth` passes raw pools to failure
audit and card-scope audit and stores one in `PgWorkloadBindingResolver`.
`check:from-pools-allowlist` guards pool construction sites, not these SQL
capabilities; `check:tenant-isolation` still describes raw pools as valid for
some query signatures. This permits another auth caller to bypass the intended
boundary while the checks remain green.

Use one slug-resolution operation owned by `WyrdPostgres`, backed by the
existing `platform.resolve_tenant_by_slug` SQL function. Pre-tenant lookup
uses `OperatorPool`; tenant work uses `TenantConn`. If the operator connection
is absent, fail closed rather than using an app-pool fallback. Remove the
second Rust resolver and all raw-pool signatures or fields in the touched
auth, boot, and SQL paths. Reuse existing `WyrdPostgres::tenant_conn` for
tenant transactions and the canonical audit append; do not introduce another
pool wrapper, slug query, audit path, or lock service. Keep the login endpoint's
generic unknown/inactive-tenant refusal and workload tenant binding.

Update the existing `check:tenant-isolation` boundary so production query and
auth signatures/fields accept only `OperatorPool` or `TenantConn`. Reconcile
its stale raw-pool allowlists with `architecture/agent-rules.md`. Keep
`check:from-pools-allowlist` for its distinct construction property. Prove
login, workload, and boot resolve the same active slug and refuse an inactive
or missing slug; run both existing boundary checks. Pool construction inside
the SQL connection owner remains its private implementation, never a
capability handed to domain code.

## 2. Accept the shared JWT correction

`ExternalVerifier::verify_external_against` now requires `exp`, `iss`, and
`aud` and validates a present `nbf`. This is an accepted security correction,
not a defect to remediate. A trusted signature without a required audience
does not show that the token was meant for Wyrd. RFC 7523 requires `iss`,
`sub`, `aud`, and `exp` for JWT bearer assertions and forbids accepting a
present future `nbf`; OIDC Core requires the corresponding ID-token checks.
Keep the one shared signature/JWKS, issuer-key, audience, and algorithm
verification path. Keep ID-token-only `iat`, subject-format, `azp`, and nonce
rules on the existing ID-token path. Do not relax the shared verifier or build
a second workload verifier. The only follow-up is to retain focused regression
proof that a valid workload assertion succeeds and missing `iss`/`aud` or
future `nbf` is refused; reuse existing tests where they already prove it.
Use OIDC Core §3.1.3.7, RFC 7523 §3, and RFC 8725 §3 as the checklists.

## 3. One refresh-family serialization rule

The refresh, revocation, initial issuance, and callback-role fixes all share
one cause: a writer of User session or role authority can race another
writer unless both take the existing family lock. List every production writer
of User status, roles, sessions, and refresh rows, then make each use the
existing family lock before the connection lock and before reading authority.
Keep transaction ownership with the caller and RLS on `TenantConn`; do not
add a new lock mechanism. Prove the two issuance/revocation orders and
concurrent callbacks cannot union roles. Reuse one existing lock-wait test
helper for equivalent Postgres observations and delete duplicate helpers.

## 4. Enforce approved sealing-key and handoff contract

Approved spec revision 5 resolves the former REQ-005 conflict: the deployment
keyring protects provider secrets and recoverable login/session credentials,
including secretless-provider login. Keep the existing encrypted, two-minute,
one-use completed-credential handoff for browser and CLI, using the same
keyring and tenant-scoped SQL path. Do not store the token pair in plaintext,
put it in a redirect or browser page, introduce a second key or auth path, or
make machine authentication depend on human-login key material.

For TASK-002, verify callback issuance and sealed completion remain atomic,
the completed row expires, redemption consumes it only once, tenant and
initiator bindings are retained, activation and login with a missing or
unusable key fail closed even for a secretless provider, and retained keys
can open in-flight completions during rotation. Preserve the existing
provider-secret rotation proof and independent machine authentication. The
browser BFF's authenticated flow-cookie redemption and the CLI's
verifier-held claim are owned by
TASK-003 and TASK-004 respectively; TASK-002 cannot claim their end-to-end
security before those tasks pass. Do not build those later surfaces in this
remediation.

## 5. Keep aligned behavior and isolate unrelated fixes

Keep email non-unique within a tenant: `(issuer, sub)` is the identity and a
same-email replacement-provider login creates a separate User. Preserve the
provider-switch journey. Keep unrelated gateway, Forge, and router test fixes
only where needed for required green gates; identify them in separate commits
and evidence, and remove incidental changes. Complete the two R10 rustdoc
findings in the parent remediation packet.

## Completion evidence

- A source inventory of every slug-resolution caller, raw-pool signature or
  field, and User authority writer, with the surviving owner for each.
- Focused OIDC ID-token, RFC 7523 workload, slug refusal, and concurrency
  checks, using the existing test lanes and exact focused commands for named
  Rust tests.
- `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`,
  `mise run check:from-pools-allowlist`, and the affected identity journeys.
- Evidence for REQ-005 under approved spec revision 5: missing-key refusal,
  sealed one-time completion, expiry, and key rotation without losing
  in-flight completions. Record TASK-003/004 initiator checks as pending
  until their real browser and CLI journeys pass; do not count earlier green
  gates or the R10 verdict as proof of this new work.

## Implementation evidence

Commits: `454bdb90e` (item 1), `b0899f651` (item 2), `fa24a79ac` (item 3),
`0de851078` (item 4), `afe0605b2` (identity-journey gate fix). Postgres-backed tests were run through
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked ...`.

### Source inventory

| Site | Before | Surviving owner |
|---|---|---|
| Tenant login `HumanConnections::begin_login` (`wyrd-auth/src/login.rs`) | `WyrdPostgres::resolve_tenant_slug` | unchanged: `WyrdPostgres::resolve_tenant_slug` |
| Workload exchange `resolve_tenant_slug` (`wyrd-server/src/auth/jwt_bearer.rs`) | `resolve_by_slug_for_app(&PgPool)` | `WyrdPostgres::resolve_tenant_slug` |
| Boot `resolve_implicit_tenant` / `seed_trusted_issuers` / `seed_workload_bindings` (`wyrd-server/src/boot/issuer.rs`) | `&PgPool` | `&WyrdPostgres` (`resolve_tenant_slug`, `tenant_conn`) |
| `WyrdPostgres::login_state_tenant` (`wyrd-sql/src/postgres.rs`) | `resolve_tenant_slug` | unchanged |
| Resolver query `tenant_resolver::resolve_by_slug` | `pub resolve_by_slug_for_app(&PgPool)` on the app pool | `pub(crate) resolve_by_slug(&OperatorPool)`; `WyrdPostgres::resolve_tenant_slug` refuses with `InsufficientPrivilege` when no operator pool is configured |
| `PgWorkloadBindingResolver` field (`wyrd-auth/src/pg_resolvers.rs`) | `PgPool` | `WyrdPostgres` |
| `record_auth_audit_best_effort`, `audit_scope_mint_failure_best_effort` (`wyrd-auth`) | `&PgPool` | `&WyrdPostgres` |
| Storage admin `reap_idempotency_keys`, `expired_uploads_batch` (`wyrd-sql/src/queries/storage/admin`) | `&PgPool` | `&OperatorPool` |
| Raw `PgPool` still in `wyrd-server/src`: `postgres.rs` (pool owner), `test_support.rs` (cfg(test)), `components/eval/resolver.rs`, `audit/mod.rs` | — | Outside the auth/boot/SQL-query scope of this packet; the new `check_sql_capability_signatures` covers the wyrd-sql and vala-sql query dirs, `wyrd-auth/src`, and `wyrd-server` `auth/`, `components/auth/`, `boot/` |
| User authority writer: callback role sync (`wyrd-auth/src/callback.rs`) | family lock before role replacement | unchanged. Identity resolution precedes the lock because the principal id is not known until then. |
| User authority writer: `issue_human_session` (`wyrd-auth/src/issuance.rs`) | family then connection slot | unchanged |
| User authority writer: refresh rotation (`wyrd-auth/src/refresh.rs`) | hash lookup finds the family, then lock, then consume | unchanged |
| User authority writer: revocation (`wyrd-auth/src/revoke.rs`) | read User, then lock | family lock **before** `user_by_id` |
| Lock-wait test helpers | inline pid loop in `refresh.rs` | reuses `revoke::pg_tests::wait_for_advisory_lock_wait`. `wait_for_connection_slot_waiter` (keyed, unknown pid) and the server's count-based `wait_for_lock_waiters` observe different conditions and remain. |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One slug resolver on the operator pool, fail closed without it | `wyrd-sql/src/postgres.rs`, `queries/platform/tenant_resolver.rs` | `-p wyrd-sql --lib -E 'test(=postgres::telemetry_tests::resolve_tenant_slug_without_operator_pool_fails_closed)'`; `-p wyrd-sql --test pg_tenant_slug -E 'test(=pg_tests::resolver_answers_only_active_slugs)'` | PASS |
| No raw pool in auth/boot/SQL-query signatures; the check enforces it | `scripts/check_tenant_isolation.py` (`check_sql_capability_signatures`, stale allowlists deleted) | `mise run check:tenant-isolation` passes and fails on an injected `&PgPool`; `mise run check:from-pools-allowlist` passes (`boot/issuer.rs` cfg(test) entry added) | PASS |
| Callers compile and behave on `WyrdPostgres` | `wyrd-auth`, `wyrd-server` boot/auth, `wyrd-storage` sweeper, `wyrd-testing` | wyrd-sql lib (76), wyrd-auth pg_resolvers/card_scope/audit (29), wyrd-server `boot::issuer` + `auth::jwt_bearer` (17), `-p wyrd-storage --test pg_sweeper` (2) | PASS |
| Shared JWT correction; a valid workload assertion succeeds; missing iss/aud and future nbf are refused | `wyrd-auth-verify/src/lib.rs` | `-p wyrd-auth-verify --lib -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims) \| test(=tests::verify_external_happy_path_returns_verified_identity_not_principal)'` | PASS |
| Family lock before connection lock and before reading authority; both orders; no role union | `wyrd-auth/src/revoke.rs` | `-p wyrd-auth --lib` revoke `pg_tests` (10), the issuance both-orders test, `refresh` `ancestor_replay_overlapping_rotation_revokes_successor`; `-p wyrd-server --lib -E 'test(=auth::callback::pg_tests::concurrent_callbacks_replace_roles_without_union)'` | PASS |
| REQ-005: callback issuance and sealed completion are atomic | `wyrd-auth/src/callback.rs` `finish_id_token_exchange` (single transaction) | `-p wyrd-server --lib -E 'test(=auth::callback::pg_tests::a_failed_role_sync_audit_rolls_back_the_whole_login)'` | PASS |
| REQ-005: one-time redemption, bound to tenant and initiator | `wyrd-sql` `REDEEM_LOGIN_COMPLETION_SQL` | `-p wyrd-auth --lib -E 'test(=login::pg_tests::a_completed_login_is_redeemed_once_by_its_binding)'`; `-p wyrd-sql --test pg_login_state` tenant confinement | PASS |
| REQ-005: expiry, retained-key rotation, unusable key fails closed | `wyrd-auth/src/login.rs` | `-p wyrd-auth --lib -E 'test(=login::pg_tests::completions_survive_rotation_and_fail_closed_on_unusable_or_expired)'` | PASS |
| REQ-005: a missing key fails closed for a secretless provider at activation and login | `HumanConnections::activate` now calls `require_keyring` first | `-p wyrd-auth --lib -E 'test(=connections::probe_tests::activation_without_a_sealing_key_is_refused_for_a_secretless_provider) \| test(=login::pg_tests::begin_without_a_sealing_key_is_refused_before_any_state)'` | PASS |
| Provider-secret rotation proof and machine independence are preserved | unchanged: `wyrd-crypt` `keyring_rotation_rewraps_and_retires_the_old_key`, boot `keyless_boot_refuses_only_when_ciphertext_is_stored`, `tenant_machine_independence_journey` | `mise run test:identity:journey` | PASS (27/27) |
| Email non-unique; the provider switch creates a separate User | migration `20260925000001_auth_login_state_binding.sql` drops `auth_users_data_tenant_id_email_key` | `tenant_provider_switch_journey` in `mise run test:identity:journey` | PASS (27/27) |
| R10 rustdoc findings 23/24 | commit `1c6b86c65` | `mise run fmt`, `mise run lints` | PASS |
| Format and diff hygiene | — | `mise run fmt`, `git diff --check` | PASS |

### Pending and non-goals

- TASK-003 (BFF authenticated flow-cookie redemption) and TASK-004 (CLI
  verifier-held claim) initiator checks are **pending**. The BFF and CLI
  surfaces were not built here, and no earlier gate counts as their proof.
- The gateway and Forge test fixes (`3b3c0a6b7`, `0fce08515`, `194888ef9`,
  `1c6e0c054`) and the bifrost format commit (`dacd7d04b`) are isolated
  commits from earlier rounds that were needed for green lanes. No further
  incidental changes were made in this remediation.
- `afe0605b2` is a required gate fix. `tenant_callback_refusal_journey` makes
  more `/auth/*` calls from the single test peer than the shared auth
  governor's burst of 20, so step 6 was refused with `429` by admission. The
  journey helpers `begin_login` and `callback_reply` now honor
  `retry-after`, as a real client does. The governor refuses before any
  handler runs, so no login state is consumed. Production behavior is
  unchanged. Proof: `WYRD_IDENTITY_FILTER=tenant_callback_refusal_journey mise run test:identity:journey`, then the full
  `mise run test:identity:journey` (27/27).
- Gates: `mise run fmt`, `mise run lints` (exit 0),
  `mise run check:tenant-isolation`, `mise run check:from-pools-allowlist`,
  `git diff --check`.
- Operational risk: tenant login and workload exchange now require the
  `wyrd_platform_admin` operator pool (its DSN is optional for external
  Postgres), and that pool defaults to 2 connections.
