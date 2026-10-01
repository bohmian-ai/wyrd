# Domain review: tenancy, persistent data, migration, and concurrency

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Candidate identity was rechecked before writing this report and remained the repository `HEAD`.

## Reviewed boundary and authority coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant SQL capability and RLS | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` raw-pool, transaction-ownership, and `TenantConn` rules; `architecture/wyrd-security-posture.md` tenant/data isolation | `wyrd-sql` migration and human-connection queries; `WyrdPostgres`; `HumanConnections`; migration and identity journeys | FAIL: TD-002 |
| One Active / one Candidate and tenant isolation | Task packet-local contract; SPEC REQ-002; security posture tenant isolation | partial unique indexes, forced RLS policy, tenant advisory transaction lock, revision derivation, two-tenant and concurrent-activation journey source | PASS |
| Mutation transaction ownership and atomic activation | Task packet-local contract; agent rules for `TenantConn`; SPEC REQ-003/REQ-017 | `HumanConnections::{put_candidate,test_candidate,activate,deactivate,remove}`, SQL query slots, canonical audit append ordering | PASS |
| Deactivate/remove/replacement lifecycle | Task packet-local contract lines 104–107; SPEC REQ-016 | connection mutations, callback exchange, refresh-token rotation, refresh schema, identity journey source | FAIL: TD-001 |
| Tombstones and historical identity | Task packet-local connection contract | removal update, table constraints, live-row projections | PASS for connection storage; session provenance gap is part of TD-001 |
| Legacy Human migration and workload binding preservation | Task packet migration contract | migration preflight, Human-to-connection copy, binding-preserving Workload conversion, migration integration test | PASS |
| Replica visibility and concurrency | Task packet-local contract; SPEC REQ-002/REQ-003 | durable per-request Active reads, advisory lock, database constraints, two-replica and concurrent-activation journey source | PASS, subject to TD-001 for session renewal and in-flight callback |

The migration correctly refuses multiple Human issuers, nonempty default roles,
audience/client mismatch, unsupported client authentication, and a missing
required secret before schema mutation. It copies the claim mapping, group map,
JWKS URI/TTL, and ciphertext, retains a bound legacy issuer as Workload without
changing the binding key, deletes only unbound Human trust, then prevents future
Human rows in the workload store. The migration test uses isolated pre-migration
databases and checks both successful preservation and transactional refusal.

The connection table forces RLS and has database-enforced partial uniqueness for
Active and Candidate. All lifecycle mutations serialize on a tenant-derived
transaction advisory lock. The SQL query functions accept `&mut TenantConn` and
do not commit or roll back; the owning `HumanConnections` workflow commits the
audit decision and state transition together. Activation retires the old Active
and promotes the exact tested, unexpired candidate in one transaction, so a
failed promotion rolls the retirement back. Removal retains the row identifier,
sets it Inactive, wipes ciphertext, and excludes it from live reads.

## Material proposed findings

### TD-001 — MISSING: connection lifecycle is not enforced at session issuance or renewal

- **Violated obligation:** The task requires deactivate/remove to block new
  login and renewal immediately, requires activation/replacement to retire the
  old provider atomically, requires renewal to read the durable Active record,
  and states that tombstone identifiers remain while refresh/session references
  may exist (`TASK-001-tenant-connections.md:104-107`). SPEC REQ-016 likewise
  requires an old-connection session to stop renewing after replacement,
  deactivation, or removal.
- **Exact locations:**
  - `crates/wyrd/wyrd-auth/src/callback.rs:87-117` reads the Active connection
    before provider discovery/code exchange, but
    `finish_authorization_code_exchange` at `:192-215` opens a later tenant
    transaction and issues the session without re-reading or locking the Active
    connection.
  - `crates/wyrd/wyrd-auth/src/refresh.rs:101-142` consumes and rotates a Human
    refresh token based on the user and current grants only. It never resolves
    the durable Active connection.
  - The candidate adds no connection provenance to
    `wyrd.auth_refresh_tokens`; neither callback issuance nor refresh rotation
    persists or compares a human connection id/revision.
  - `crates/wyrd/wyrd-server/tests/identity_e2e.rs:1681-1719` proves only that a
    *new* login attempt fails after deactivation/removal, and `:2108-2143`
    proves replica-visible new-login behavior. Neither journey presents an
    already-issued refresh token after replacement/deactivation/removal or
    races a callback already past its first Active read.
- **Evidence and reachability:** A successful OIDC callback issues a refresh
  token. After that, `HumanConnections::deactivate`, `remove`, or replacement
  changes only `auth_human_connections`. The existing refresh row and user stay
  active, so `RefreshTokens::execute` can mint a successor after the mutation.
  Separately, a callback can read the old Active connection, pause during
  provider IO, allow a concurrent replica to commit deactivation/replacement,
  and then resume into the issuance transaction with the stale `TrustedIssuer`.
  The candidate's implementation evidence explicitly labels renewal work a
  TASK-002 non-goal, but that does not remove the contrary obligation from this
  immutable task packet.
- **Observable consequence:** An old provider session can renew after the
  connection is replaced, deactivated, or removed. An authorization-code
  exchange already in flight can also establish a new session after the
  lifecycle mutation committed. Both outcomes extend old-connection authority
  beyond the task's immediate cutoff and are visible across replicas.
- **Required testable correction:** Bind new OIDC login state and Human refresh
  families to the exact durable connection id and revision. Before callback
  issuance and every Human refresh successor, lock/read the tenant's durable
  Active connection in the same tenant transaction and require that exact
  id/revision; refuse without issuing a session or successor when it is absent,
  inactive, removed, or replaced. Keep the tombstone identifier for historical
  references. Add a real-server two-replica journey that logs in, commits each
  replacement/deactivation/removal mutation on one replica, and proves the old
  refresh token is refused with no successor on the other; also prove an
  in-flight old-connection callback cannot issue after the mutation commits.

### TD-002 — VIOLATION: the new domain owner propagates a raw `PgPool`

- **Violated obligation:** `architecture/agent-rules.md:6` bans raw
  `sqlx::PgPool` from library struct fields and function signatures. Tenant work
  must enter through the owning `WyrdPostgres` handle and `TenantConn`; the
  security posture requires privileged pools to be excluded from handlers by
  construction.
- **Exact locations:** `crates/wyrd/wyrd-auth/src/connections.rs:22,75-79,114-125`
  stores and accepts `PgPool`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:8,62-68`
  accepts `&PgPool`, clones it, and propagates it into the domain owner.
- **Evidence:** The repository already provides the approved owner at
  `crates/wyrd/wyrd-sql/src/postgres.rs:64-145`, including
  `WyrdPostgres::tenant_conn`. The new `HumanConnections::begin` instead calls
  `TenantConn::acquire` on its raw field directly. `mise run
  check:from-pools-allowlist` exited zero during this review (while printing an
  unrelated missing `python/` search path), demonstrating that this named check
  does not supply proof for the raw-field rule at these locations.
- **Observable consequence:** The type boundary no longer guarantees that this
  identity owner acquires tenant transactions through the repository's sole
  runtime-ready SQL owner. It permits arbitrary app pools to be propagated into
  a security-sensitive domain service and bypasses the handle's acquisition
  lifecycle/telemetry.
- **Required testable correction:** Make `HumanConnections` compose the existing
  `WyrdPostgres` owner (or receive an already-open `&mut TenantConn` where
  transaction composition requires it) and acquire tenant transactions through
  `WyrdPostgres::tenant_conn`. Remove raw `PgPool` fields and parameters from the
  changed auth/server path. Verify the changed crates compile and lint, and
  statically confirm the owner boundary contains no raw pool propagation.

## Verification limits

- I reviewed the complete base-to-candidate diff and the current candidate
  source. I did not modify candidate implementation files.
- I inspected the exact migration and identity journey sources and the task's
  recorded all-green verification evidence. To stay inside the domain-review
  time limit, I did not rerun the Postgres/Keycloak/Dex journeys or migration
  suite.
- I ran `mise run check:from-pools-allowlist`; it exited zero but emitted
  `rg: python/: No such file or directory` and does not validate the raw
  `PgPool` field/signature violation described in TD-002.
- The candidate remains at the supplied commit. Other Wave 1 reports are not
  inputs to this independent review.

## Overall result

**FAIL**

The RLS schema, slot serialization, revision conflict handling, atomic
activation, tombstone storage, legacy migration, binding preservation, and
durable replica reads are otherwise coherent. TD-001 leaves a reachable
old-connection authority path, and TD-002 violates the mandatory SQL capability
boundary.
