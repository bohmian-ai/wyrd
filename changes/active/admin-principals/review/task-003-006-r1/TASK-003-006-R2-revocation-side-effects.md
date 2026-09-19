# TASK-003-006-R2 — Remediation: revocation side effects introduced by R1

Route to `$wyrd-implement`.

This task covers **only** the two defects the R1 remediation commit introduced.
The nineteen findings from the first review that R1 did not close remain governed
by
`changes/active/admin-principals/review/task-003-006/TASK-003-006-R1-lifecycle-audit-and-atomicity.md`,
which is not superseded. Do not re-plan that work here.

## Subject

| | |
|---|---|
| Approved spec | `changes/active/admin-principals/spec.md`, revision 6 |
| Original tasks | `changes/active/admin-principals/tasks/TASK-006-credential-lifecycle-and-recovery.md` (primary), `TASK-005-tenant-administration.md` |
| Prior verdicts | `review/task-003-006/verdict.md`, `review/task-003-006-r1/verdict.md` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `4225069` |
| Commit that introduced both defects | `f156e64` |

## Issue diagnoses

### 1. A re-revoke returns 404 after committing a principal-wide token kill (FIND-006-5)

**Violated obligation**: the route's own documented contract at
`crates/wyrd/wyrd-server/src/components/principals/routes.rs:337-339` — `NotFound`
is documented to mean "the credential is unknown in this tenant", which a caller
reads as "nothing happened". INV-011 fail-closed on ambiguity. The R1 task's
constraint that the correction preserve adjacent behavior.

**Current behavior**: `revoke_credential`
(`components/principals/routes.rs:341-401`) advances the principal's revocation
epoch unconditionally, commits, and only then decides what to return.

- `credential_belongs_to` (`crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:93-108`)
  has no `revoked_at` predicate, so an already-revoked credential passes the
  ownership gate at `routes.rs:357-365`.
- `REVOKE_API_KEY_SQL` (`api_keys.rs:11-15`) carries `AND revoked_at IS NULL`, so
  `revoke_api_key` returns `false` for that credential — correct, and documented
  as idempotent at `api_keys.rs:19-20`.
- `routes.rs:370` calls `revoke_service_account_principal` regardless of that
  value, `routes.rs:373` commits, `routes.rs:379` fans out the NOTIFY, and only
  `routes.rs:393` consults `revoked` — by which point the epoch advance is durable.

**Observable consequence**: an automation principal holding credentials C1 and C2
has C1 revoked (204). Any replay of that identical `DELETE` — an SDK retry after a
timed-out 204, a second operator, a re-run pipeline step — returns `404 Not Found`
while advancing `tokens_not_before` again, so every live token C2 minted since the
first revocation stops authorizing. The call is repeatable, reports that it did
nothing, and is available to any holder of `service_accounts:write`. The ordinary
retry-after-timeout on a revoke is therefore destructive in a way the 204 path
already accounts for and this one does not.

A second consequence on the same path: the `!owned` return at `routes.rs:360-365`
drops `conn` without committing, so the `Allowed` audit row written at
`routes.rs:95-102` is rolled back. A caller enumerating
`{principal_id, credential_id}` pairs leaves no audit trail at all — the opposite
of what FIND-005-1 was closed to achieve.

**Why the existing proof falls short**: `a_credential_cannot_be_revoked_through_another_principal`
(`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:582`) covers the
wrong-principal path, which returns before any write and is correct. No test
revokes the *same* credential twice, so the defective path is never entered.

### 2. The revocation NOTIFY names a kind the verifier may never look up (FIND-006-6)

**Violated obligation**: the Ponytail reuse ladder — the repository already owns
this lookup. The R1 commit's own claim that the fanout makes other replicas "drop
their cached epoch instead of serving the revoked token until the TTL lapses".

**Current behavior**: `routes.rs:379-385` calls `notify_principal_revoked` with a
hardcoded `PrincipalKindTag::Service`. The epoch cache is keyed by
`(tenant, kind, principal_id)` (`crates/wyrd/wyrd-auth/src/revocation_resolver.rs:26-31`),
`invalidate` matches on that whole key (:79-82), and `epoch` resolves
`TenantAdmin`, `Service` and `Agent` from the same table under three distinct keys
(:116-122).

**Observable consequence**: revoking a credential of a `tenant_admin` principal —
one of which provisioning mints for every tenant — or of an `agent` principal
invalidates `(tenant, Service, id)`. Every other replica reads
`(tenant, TenantAdmin, id)` or `(tenant, Agent, id)`, misses the invalidation, and
keeps authorizing the revoked credential's live tokens for the full five-second
TTL. The durable epoch write still enforces revocation eventually, so this bounds
the window rather than defeating it — but it is precisely the window the fanout
was added to close, and it closes only for `service` principals.

**Why the existing proof falls short**: the in-process test server has no NOTIFY
listener at all (`crates/wyrd/wyrd-testing/src/server.rs:3732-3739`), so no
journey can observe the fanout. The defect is only reachable in a multi-replica
deployment.

## Intended correction outcome

Revoking a credential advances the owning principal's epoch **exactly when it
actually revoked something**, and the fanout that shortens the cross-replica
window names the principal's real kind. A call that revokes nothing changes
nothing and says so. Everything the R1 commit got right — one transaction for the
revoke and the epoch, principal-scoped addressing, same-transaction audit,
fail-closed on an unrecordable decision — is preserved unchanged.

## Decision-complete recommendation

**For FIND-006-5**, reuse `revoke_api_key`'s existing idempotence rather than
adding a second notion of "already revoked". It already reports, through its
boolean, whether this call was the one that revoked the credential. Make the epoch
advance and the fanout conditional on that answer, so the not-found outcome
performs no write at all. Resolve the not-found outcome before the transaction
commits so the surface has one refusal path rather than two, and let the
authorization row recorded by `authorize` survive that refusal — the repository's
own precedent for this is stated at
`crates/wyrd/wyrd-server/src/components/admin/routes.rs:15-18` ("Every mutation
commits the Allowed row standalone before its work, so the decision survives a
conflict, not-found, or failed write"), and that is the shape to match here.

Do not reach for the alternative of giving `credential_belongs_to` a `revoked_at
IS NULL` predicate. It would make the second call return not-found for the right
reason by accident, but it conflates "not yours" with "already gone" and leaves
the unconditional epoch write in place for every other future caller of this
handler.

**For FIND-006-6**, reuse the kind lookup the repository already has.
`revoke_principal_in_conn` (`crates/wyrd/wyrd-auth/src/revoke.rs:35-45`) reads
`service_account_by_id` and derives the wire kind through `principal_kind_wire`;
that is the same table and the same principal this route already holds an id for.
Take the kind from the row instead of asserting it. Do not widen the cache key or
add a kind-agnostic invalidation path — the key's per-kind shape is deliberate
(`revocation_resolver.rs:22-25`) and changing it is a concurrency decision outside
this remediation.

## Constraints and preserved behavior

- The credential revoke and the epoch advance stay in one transaction
  (`routes.rs:367-373`). Do not split them.
- Keep the same-transaction `audit::append_on` seam and its fail-closed behavior.
  Do not move this surface to a standalone audit transaction for the *decision*;
  AGENTS.md §2 requires the decision to be audited in the transaction that made
  it. Only the not-found refusal's own durability is in scope.
- Keep principal-scoped addressing via `credential_belongs_to`.
- Keep the NOTIFY best-effort: a failed fanout logs and does not fail the request
  (`routes.rs:387-390`).
- Preserve the per-principal epoch granularity and the trade-off the R1 commit
  documented at `routes.rs:328-332`. That behavior is accepted; only its
  unconditional application is the defect.

## Explicit non-goals

- Tenant lifecycle enforcement, provisioning resume, tenant list/inspect/suspend,
  CLI, documentation, OpenAPI registration, and every other prior finding. Those
  belong to `TASK-003-006-R1-lifecycle-audit-and-atomicity.md`.
- Any change to the epoch cache key, TTL, or the NOTIFY protocol.
- Any new revocation mechanism.

## Acceptance criteria

1. Revoking a credential that is already revoked returns not-found, advances no
   epoch, and emits no fanout. (FIND-006-5)
2. A token minted by a *sibling* credential after the first revocation still
   authorizes after a second `DELETE` of the first credential. (FIND-006-5)
3. The first revocation still stops the tokens the revoked credential already
   minted — the R1 property is unchanged. (FIND-006-1 regression guard)
4. A not-found refusal on this route leaves a durable authorization record.
   (FIND-006-5, secondary)
5. The revocation fanout names the principal's actual kind as stored in
   `wyrd.auth_service_accounts`, for `service`, `agent` and `tenant_admin` alike.
   (FIND-006-6)

## Proof

Focused, directly on the gap:

- Extend `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs` with a journey that
  issues two credentials on one principal, revokes the first, mints a token from
  the second, replays the identical `DELETE` for the first, asserts `404`, and
  asserts the second credential's already-held token still authorizes. That single
  journey carries criteria 1, 2 and 3.
- Criterion 4 is asserted in the same journey by reading the tenant's audit rows
  after the replayed call.
- Criterion 5 does not need a multi-replica harness. A Postgres-backed test in
  `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs` that revokes a
  `tenant_admin` principal's credential and asserts the kind the route resolves
  matches the stored `principal_kind` is the smallest credible check; do not build
  a NOTIFY listener fixture for it.

Broader verification, scope `VER-001`…`VER-006` only — no aggregates:

```
mise run lints
mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e
mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_admin_principals
```

Both lanes need the repository-managed Postgres wrapper; the journeys need
`WYRD_AUTH_E2E=1`.
