# Persistence and concurrency domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation: `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- R3 remediation direction: `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`

The candidate remained at the stated commit throughout this review. `.codegraph/`
is absent, so I used the immutable Git range and repository source directly.
`FIND-TASK-010-1` remains routed to TASK-011 and was not reopened. The current
lead direction for `FIND-TASK-010-10` supersedes the R2 task: this review did not
require an image-local limit, application limiter, edge manifest, setting, or
new persistent state.

## Reviewed boundary

I traced the cumulative persistent authorization state from its producers to
all material consumers and terminal transitions, including crash, rollback,
retry, restart, and concurrent-replica behavior.

| Durable boundary | Authority and source coverage | Result |
| --- | --- | --- |
| Authorization login state and codes | REQ-009, REQ-021, TASK-010 Scenario 1; `wyrd-sql` login-state migration/query owner; callback completion and token redemption; SQL and identity tests | PASS |
| Device authorizations | REQ-011, AC-007, RFC 8628 sections 3.4-3.5; device migration/query owner; callback approval, denial, expiry/poll deletion, and token redemption; deterministic terminal-race tests | PASS |
| Refresh rows and rotation graph | REQ-012, REQ-016, RFC 6749 section 6, RFC 9700 section 4.14.2; refresh row/query owner; public rotation, confidential non-rotation, chain revocation, logout, and route commit handling | PASS |
| Human-connection lifecycle | REQ-016; connection slot lock, exact id/revision predicate, callback final fence, and issuance/refresh lock order; multi-replica cutoff and overlap coverage | PASS |
| Transactional audit | REQ-017 and the canonical audit rules in `AGENTS.md` / `architecture/agent-rules.md`; callback, device, authorization-code, refresh-reuse, issuance, and revoke transaction owners | PASS |
| Migrations, constraints, and RLS | Changed human-connection, login-state, connection-test, and device-authorization migrations; clean migration proof; forced RLS and `TenantConn` query paths | PASS |
| R3 correction delta | `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29..1f4466a9ae482eb1311f6e5206484758bc272ad8` changes review records, bundled NGINX/startup assertions, and one operator note only; no auth table, query, lock, transaction, or persistence owner changed | PASS |

Applicable repository authority included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/references/architecture/patterns.md`, and the approved spec/task.
I applied the standing conventional-mechanism direction: no mechanism beyond
the named RFC behavior or ordinary PostgreSQL transaction/locking behavior is
required here.

## Source assessment

### Authorization codes

`issue_authorization_code` attaches a digest, principal, and database-clock
expiry only to a consumed authorization-request row
(`crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:78-87,369-383`). Token
redemption deletes by digest and returns the complete binding in one statement
(`login_state.rs:95-101,396-424`). The owning transaction commits a refused
expired/client/redirect/PKCE mismatch so the code stays spent, while a later
issuance or audit/store failure rolls the delete back and returns no token
(`crates/wyrd/wyrd-auth/src/callback.rs:532-580`). Concurrent redemptions
therefore serialize on the delete and only one can receive the row. Process
failure before commit leaves the code retryable; failure after commit cannot
recreate it.

### Device grant

Approval is a guarded update that succeeds only for the still-live, undecided
row (`device_authorizations.rs:58-69,214-227`). Polling locks that row with
`FOR UPDATE` and records cadence in the same statement
(`device_authorizations.rs:71-89,238-247`). A terminal denial or expiry deletes
the device row and bound login state before commit; a live approval is deleted,
issued, audited, and committed in one tenant transaction
(`crates/wyrd/wyrd-auth/src/cli_logins.rs:316-384`). A concurrent approval,
denial, expiry deletion, or second redemption cannot pass around the row lock
or guarded predicate. Any issuance or audit failure rolls back both deletion
and token/refresh writes, so retry never observes a credential stored awaiting
pickup.

### Refresh and lifecycle concurrency

Refresh execution resolves the immutable owner, takes the tenant-qualified
principal-family advisory lock, and only then classifies the row
(`crates/wyrd/wyrd-auth/src/refresh.rs:137-186`). Public-client rotation uses
an atomic active-row update and stores the predecessor relationship; only a
row revoked as `rotated` enters reuse containment, which walks that existing
rotation chain and leaves independent CLI and UI logins intact
(`refresh.rs:174-219`; `refresh_tokens.rs:204-240`). Expired, logout-revoked,
administratively revoked, and already-contained rows return the ordinary
inactive-token refusal without containment state or theft audit. The server
route commits the chain revocation and canonical audit on the `Reused` refusal
(`crates/wyrd/wyrd-server/src/components/auth/routes.rs:318-371`).

Callback completion and human issuance use the same family-then-connection-slot
lock order. The callback checks the exact active connection revision while the
slot lock is held, before roles, authorization code/device approval, and
`auth.login` can commit (`crates/wyrd/wyrd-auth/src/callback.rs:390-469,612-640`).
Issuance reuses the same slot fence (`crates/wyrd/wyrd-auth/src/issuance.rs:727-755`).
Connection replacement, deactivation, and removal therefore either win first
and refuse issuance, or wait until the already-fenced transaction ends; there
is no reverse slot-then-family acquisition in the reviewed lifecycle writers.

### Schema and recovery

The migrations encode one Active connection per tenant, exact connection and
OAuth-client provenance on human refresh rows, one initiation binding per
login-state row, unique digests/device binding, device approval column
coherence, bounded database-clock expiry, and forced RLS. The unreleased
browser-session migrations are deleted as the task directs; clean migration
and idempotency passed during this review. Auth state is PostgreSQL-owned, so
process replacement loses no committed grant state. Transaction-scoped row and
advisory locks release on rollback or connection loss, allowing ordinary retry
without a lease, owner token, recovery worker, or extra state machine.

## Prior-finding closure

- `FIND-TASK-010-2`: **closed**. The deterministic denial-during-approval and
  expiry/delete-during-approval tests exercise the routed device terminal races.
- `FIND-TASK-010-8`: **closed**. The newly added confidential-client
  `active_refresh` query relies on `TenantConn` RLS and binds no parallel tenant
  selector.
- `FIND-TASK-010-11`: **closed**. Reuse is limited to a rotated predecessor and
  containment follows only its persisted `rotated_from` descendants.
- `FIND-TASK-010-12`: **closed**. The final callback transaction owns the exact
  connection-revision fence under the established lock order.
- `FIND-TASK-010-13`: **closed**. Successful callback outcomes stage canonical
  `auth.login` in the same transaction; injected audit failure rolls the whole
  durable outcome back.
- `FIND-TASK-010-10`: the final lead-directed correction changes no persistence
  or concurrency owner and adds none. Device entropy, expiry, single-use
  redemption, and poll cadence remain the in-server protections.

## Verification and limits

Focused verification run against the immutable candidate:

```text
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=cli_logins::pg_tests::a_denial_during_approval_wins) | test(=cli_logins::pg_tests::an_expiry_deleted_during_approval_wins) | test(=refresh::pg_tests::rotated_replay_revokes_only_its_chain) | test(=refresh::pg_tests::inactive_rows_are_refused_without_containment) | test(=refresh::pg_tests::active_refresh_resolves_only_this_tenants_active_row)'"
```

Result: clean migrations passed; **5 tests passed**.

```text
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::callback::pg_tests::an_unchanged_role_device_login_is_audited_once) | test(=auth::callback::pg_tests::a_failed_login_audit_rolls_back_the_whole_login)'"
```

Result: clean migrations passed; **2 tests passed**.

The task and prior immutable review record additional green focused SQL,
callback cutoff, refresh overlap, audit rollback, tenant-isolation, and startup
evidence. I did not rerun full identity or every-language journeys; those are
intentionally change-review evidence under the standing narrowest-lane
direction. No missing journey is converted into a persistence finding here.

## Material findings

None. No behavioral, security, tenancy, durability, or public-contract defect
was found in the reviewed persistence/concurrency boundary. Placement, naming,
structure, and wording observations were not promoted to findings.

## Overall result

**PASS**
