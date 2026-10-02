# Persistence and concurrency domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Routed direction: `FIND-TASK-010-1` belongs to TASK-011 and was not reopened.

The candidate commit was unchanged during this review. `.codegraph/` is absent,
so repository source, `rg`, and the immutable Git diff were used directly.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
| --- | --- | --- |
| Tenant SQL and transactions | `AGENTS.md` sections 2, 6, 9, 11, and 12; `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md`; `architecture/references/doctrine/architecture-constraints.md`; `wyrd-sql` refresh, device-authorization, login-state, and human-connection queries | PASS |
| Device terminal races | `wyrd-auth/src/cli_logins.rs`; `wyrd-sql/src/queries/auth/device_authorizations.rs`; device/login-state constraints and tests | PASS |
| Callback lifecycle fencing and lock order | `wyrd-auth/src/callback.rs`; `wyrd-auth/src/connections.rs`; `wyrd-auth/src/issuance.rs`; `wyrd-sql/src/queries/auth/human_connections.rs`; server callback tests and `tenant_connection_session_cutoff_journey` | PASS |
| Refresh rotation and replay containment | `wyrd-auth/src/refresh.rs`; `wyrd-auth/src/cli_logins.rs`; `wyrd-auth/src/revoke.rs`; `wyrd-sql/src/queries/auth/refresh_tokens.rs`; refresh tests and token-route commit path | PASS |
| RLS-only active refresh lookup | `TenantConn` rules; `ACTIVE_REFRESH_SQL`; all production callers of `active_refresh` | PASS |
| Transactional login and revocation audit | canonical audit rules; callback, refresh, device, and revocation transaction owners; injected audit-failure tests | PASS |
| Startup persistence/recovery proof | `mise.toml` task description; base and candidate `scripts/server/test-startup.sh`; operator-key production validation; startup SDK write/restart/read journey | PASS |

## Boundary assessment

### Device approval terminal races

`CliLogins::approve` reads a still-pending device authorization, commits that
read transaction, and begins the bound login. This remains safe because the
durable approval is not written there: callback completion uses
`approve_device_authorization`, whose update succeeds only while the device row
still exists, is unexpired, and is undecided. The final callback transaction
also contains User/role work and `auth.login`, so a failed guarded update drops
the transaction and leaves none of those effects.

The two new tests park the real approval after its lookup and at its existing
unique device-binding insert, observe the database waiter through
`pg_blocking_pids`, then deny or expire-and-poll-delete the device row from a
second actor. Both resume the production completion path and prove no User,
refresh row, authorization code, device approval, or login audit survives. The
polling loop is only lock-state observation with a timeout; it is not a sleep-
timed race or a production test hook.

### Callback lifecycle fencing and lock order

`finish_id_token_exchange` now takes the User refresh-family lock, then the
tenant human-connection slot lock, and checks the exact connection id and
revision inside the same final `TenantConn` transaction before roles, code or
device approval can commit. Human issuance uses the same family-then-slot
order. Connection lifecycle mutations take only the slot lock, so there is no
reverse slot-then-family acquisition and no new deadlock cycle.

The multi-replica cutoff journey parks the callback on the family lock, lets
deactivation commit, and then proves the callback is refused with no code,
role, refresh, or login-audit delta. Ordinary unchanged-connection callbacks
remain covered by the callback completion tests.

### Refresh rotation-chain containment

Refresh execution resolves the immutable row owner, takes the principal-family
lock before classification, and re-reads the row under that lock. Only a row
whose reason is `rotated` is classified as replay. Its existing
`rotated_from` graph is passed to `revoke_refresh_chain`; unrelated root chains,
including other CLI logins and the non-rotating UI session, are untouched.
Expired, logout-revoked, principal-revoked, and already-contained rows return
the indistinguishable inactive-token result with no containment write or theft
audit.

The route explicitly commits `RefreshError::Reused`, which is necessary for
the chain revocation and canonical audit to survive the refused HTTP grant.
The overlap test proves an ancestor replay waits for a concurrent successor
rotation and then contains that committed successor.

### RLS and transactional audit

`ACTIVE_REFRESH_SQL` now binds only the token hash and relies on the existing
`TenantConn` RLS boundary. Its focused test proves an identical hash in another
tenant is invisible while revoked and expired rows in the current tenant stay
inactive. No replacement tenant selector, option, or check was introduced.

Every successful non-test callback appends one redacted `auth.login` event in
the same final transaction as User creation/reuse, conditional role sync, and
authorization-code or device approval. The injected `auth.login` staging
failure proves both authorize and device completion roll back the User, roles,
code/approval, and audit effects. Logout likewise revokes a single rotation
chain and rolls the revocation back when its required audit append fails.

## Startup-lane judgment

The `test:server:startup` change is a legitimate lane repair, not a weakened
gate under `AGENTS.md` section 12.

The lane's declared contract is the official image against external Postgres,
including migration refusal/retry, nginx routes, SDK write, restart under
`APP_ENV=production`, and persistence. Neither the `mise.toml` task description
nor the script's case list declares a multi-tenant Vault journey. At the base
commit the script selected multi-tenant mode only implicitly by omitting a
tenant slug, but it did not configure Vault at all; it therefore exercised the
development default operator-key source, which the already-authoritative
production validation refuses. It was not a working multi-tenant-with-Vault
assertion that the candidate deleted.

The candidate makes the supported production topology explicit with
`WYRD_SERVER_TENANT_SLUG=acme`, uses the architecture-approved owner-only file
KEK for single-tenant production, and keeps that key plus the signing key in a
read-only mounted Docker volume across the production restart. It removes no
migration, RLS, restart, or persisted client-data assertion. Multi-tenant
production remains Vault-only in configuration validation and architecture;
changing this general startup lane does not relax that product rule. Requiring
this TASK-010 remediation to add a separate Vault-backed image topology would
broaden the task rather than restore a removed assertion.

## Verification

Focused commands run during this review:

```text
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=cli_logins::pg_tests::a_denial_during_approval_wins) | test(=cli_logins::pg_tests::an_expiry_deleted_during_approval_wins) | test(=refresh::pg_tests::rotated_replay_revokes_only_its_chain) | test(=refresh::pg_tests::inactive_rows_are_refused_without_containment) | test(=refresh::pg_tests::active_refresh_resolves_only_this_tenants_active_row)'"
```

Result: 5 passed.

```text
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::callback::pg_tests::an_unchanged_role_device_login_is_audited_once) | test(=auth::callback::pg_tests::a_failed_login_audit_rolls_back_the_whole_login)'"
```

Result: 2 passed.

The implementation record also reports green narrow owner lanes and the exact
multi-replica callback cutoff, refresh overlap, callback audit, and official
startup proofs. I did not rerun the container image build or full identity
journeys; those are substantially broader than this domain's focused source
checks and are retained for change review under the standing direction.

## Findings

No material persistence, concurrency, RLS, transactional-audit, or recovery
finding remains. Placement, naming, structure, and wording observations were
not promoted to findings.

## Overall result

**PASS**
