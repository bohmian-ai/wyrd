# TASK-009 round-2 system-resilience review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Overall result: **FAIL**

The remediation closes the prior process-lifetime, RFC 9207, workload-discovery,
and cache-coalescing failures without introducing a new runtime coordinator,
retry system, health check, or distributed cache. Provider failures remain
request- or boot-boundary failures and do not crash the shared server. One
new destructive migration, however, makes the remediated application
incompatible with every overlapping base-version replica, so the candidate
cannot follow Wyrd's supported rolling-release contract.

`FIND-TASK-009-5` is withdrawn by
`changes/active/oidc-production-readiness/review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`.
This review does not reopen it.

## Deployed paths and process ownership

| Path | Deployed owner and dependencies | Failure, restart, and recovery assessment | Result |
|---|---|---|---|
| Tenant login, connection test, and callback | `boot::install_auth` builds one `HumanConnections` per server process. Its clones share one `RelyingParty` and process-local Moka cache. Durable login state and connection lifecycle remain in Postgres. | Discovery/JWKS/token failures fail the affected login after bounded screened IO. Consumed state is not replayed; the person starts again. A restart or replacement replica starts with a cold cache and rediscovers. Other tenants, workload auth, and non-auth server capabilities remain available. | PASS |
| Platform begin and callback | `boot::install_auth` now builds one `PlatformLogin`, stores it in `ServerAuth::platform_login`, and both served handlers borrow that same process-owned owner (`crates/wyrd/wyrd-server/src/boot/mod.rs:1591-1603`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:38-41`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:644-656,682-739`). | Begin and callback on one replica reuse provider metadata and keys. A callback routed to another or restarted replica may need ordinary cold discovery; that is the task's declared process-local cache boundary, not a cross-replica availability promise. The global platform credential remains usable during an IdP outage. | PASS; prior `FIND-TASK-009-1` is closed. |
| Provider discovery, JWKS, and token exchange | Every relying-party network call uses `ScreenedHttp`: screened and pinned resolution, no proxy, no redirects, a ten-second request timeout, and a 1 MiB decoded-body cap (`crates/shared/wyrd-auth-oidc/src/screening.rs:139-199`; `crates/shared/wyrd-auth-oidc/src/relying_party.rs:155-194`). | DNS, connect, response, and body failures return typed errors. Cancellation drops request-local state. Redirect targets are never followed. An IdP outage does not terminate the process or alter another tenant's trust. | PASS |
| Provider cache and key rotation | `RelyingParty::cached` uses Moka `try_get_with`; clones share the cache. An unknown key invalidates and re-enters that same cached path once (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:377-395,479-550`). | Overlapping cold/expired misses share one fetch. Concurrent rotated-key redemptions share the overlapping refresh proven by the focused test; each redemption retries verification at most once. Failure leaves no poisoned provider entry and fails the affected login closed. No unsupported all-interleaving or cross-replica guarantee was added. | PASS; prior `FIND-TASK-009-4` is closed. |
| Workload issuer administration and boot seeding | Both paths now call `ScreenedHttp::provider_metadata`, which reads only typed discovery metadata and persists its `jwks_uri`; `ExternalVerifier` remains the sole workload key fetcher (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:197-247`; `crates/wyrd/wyrd-server/src/components/admin/routes.rs:735-776`; `crates/wyrd/wyrd-server/src/boot/issuer.rs:220-264`). | A JWKS outage no longer blocks administration or first boot after valid discovery. Assertion verification still fails closed until keys recover. Boot discovery retries are bounded; an already-seeded issuer permits restart with its durable prior trust when discovery is temporarily down. | PASS; prior `FIND-TASK-009-3` is closed. |
| Platform connection schema rollout | The new migration immediately drops `platform.oidc_connection.expected_audience` (`crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`). The base application still selects and writes that column (`35a53faa2:crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:28-45,64-75,90-129`). | Once the one-off migrator applies the candidate schema, every still-serving base replica fails platform connection reads and writes with an undefined-column error. Rollback to the old image also fails against the contracted schema. This affects platform OIDC configuration and federated login throughout the overlap window. | FAIL — `SYS-R2-001`. |

## Failure and recovery paths

### Dependency outage and timeout

Provider dependency failures remain contained to their owning request, except
for the already-established boot rule for a never-seeded configured workload
issuer. `ScreenedHttp` performs one bounded resolution and one bounded request
per endpoint and has no internal retry loop. Human login does not fall through
to another tenant or to platform trust. The platform global credential remains
an independent recovery path. The candidate adds no readiness dependency on an
IdP and no retrying gateway behavior.

### Cancellation and one-time state

Tenant and platform callbacks consume their durable one-time state before
provider redemption. Cancellation or loss after the provider accepts a code can
therefore require a new login, but it cannot replay the state or create a local
session without the final transaction completing. This is the conventional
authorization-code failure boundary. Provider cache initialization is
process-local and non-durable; a canceled or failed fetch does not replace a
previous good cache entry with a failed value.

### Process restart and rolling replacement

Process restart safely loses only provider cache entries; Postgres retains
login state, connections, principals, and sessions. A new process performs a
cold discovery while old processes can continue serving their cached issuers.

The database migration is not similarly safe. Wyrd's deployment authority
requires expand-and-contract whenever old and new replicas overlap and says a
destructive column contraction occurs only in a later release after old
replicas and workers drain
(`architecture/operations/deployment-and-release.md`, "Migration contract").
The candidate performs the contract step in the same release as the new reader
and writer. Applying migrations before surge therefore breaks the old fleet;
deploying the new fleet before migration makes the new SQL fail because it no
longer supplies the base schema's non-null `expected_audience` column
(`crates/wyrd/wyrd-sql/migrations/20260601000023_platform_identity.sql:41-53`).
There is no safe overlap ordering.

### Recovery after interruption

Provider and Postgres request failures return errors without killing the
server. A failed migration statement is transactionally refused by SQLx, but a
successful column drop is irreversible for ordinary image rollback: restoring
the old application does not restore the column or its values. Recovery then
requires a roll-forward schema correction or database restore, contrary to the
release contract's ordinary adjacent-version rollback path.

## Verification and proof assessment

The task evidence records successful exact selectors for the relying-party
cache/rotation tests, workload boot/admin tests, the platform SQL test, and the
served platform begin/callback journey. It also records the full required
aggregate lanes on code candidate `0b516e235`, with candidate
`1ddc10e21054ddc158f461e8c6d8aa862c32a067` adding only that durable evidence.
Those results credibly prove the single-version healthy and outage paths they
exercise.

The proof does not exercise adjacent-version database compatibility. All SQL
tests run only the candidate code against the fully migrated candidate schema.
They therefore cannot detect that the base reader and writer stop working as
soon as this migration is applied.

## Proposed finding

### SYS-R2-001 — VIOLATION: destructive audience-column contraction prevents a rolling release

- **Violated obligation:** `architecture/operations/deployment-and-release.md`
  requires expand-and-contract when old and new replicas overlap: both versions
  must read the overlap representation, writers must emit data readable by
  both, and dropping a column is a separate release after old replicas drain.
- **Exact location:**
  `crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`
  and the candidate writer at
  `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:89-124`; the base
  reader/writer that remain live during overlap are at
  `35a53faa2:crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:28-45,64-75,90-129`.
- **Evidence:** the migration drops `expected_audience` immediately. A base
  replica's `SELECT` and `INSERT ... ON CONFLICT` still name it. Conversely,
  before the migration, the candidate writer omits the base table's non-null
  column. Thus neither migration-first nor application-first ordering supports
  adjacent-version overlap.
- **Observable system consequence:** during a rolling or blue/green
  replacement, platform OIDC connection reads, configuration writes, and
  federated platform login fail on every remaining old replica after migration.
  Rolling back to the old image also leaves those capabilities broken. The
  rest of the server need not crash, and the global platform credential remains
  usable, but the release violates its database compatibility gate.
- **Smallest conventional correction:** use the repository's ordinary
  expand-and-contract rule. In this release, retain the existing column and
  have the new writer derive its stored compatibility value from `client_id`,
  so it is not an independent public setting and both old and new readers can
  consume rows. Do not add a toggle, alias, second audience input, compatibility
  service, or migration mode. Drop the now-unused column only in a later
  release after the supported old version has drained.
- **Focused closure proof:** migrate the base schema to the corrected overlap
  schema, write a platform connection through the candidate owner, then execute
  the base version's existing column projection against that row and prove it
  reads `expected_audience == client_id`; also prove the candidate reader and
  platform login use `client_id` and expose no independent audience input.

## Finding summary

| ID | Classification | Affected capability | Result |
|---|---|---|---|
| `SYS-R2-001` | VIOLATION | Rolling/blue-green deployment and rollback of platform OIDC configuration and federated login | Open |

**Overall: FAIL.**
