# Wave 1 Domain Review — Tenancy, Persistent Data, and Concurrency

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation inputs: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md`

The candidate commit resolves to the supplied tree. Candidate-owned auth,
server, SQL, migration, and journey sources had no working-tree differences
from that commit during this review.

## Reviewed boundary and authority coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant SQL capability and RLS | `AGENTS.md` sections 2, 3, 6, and 9; `architecture/agent-rules.md` raw-pool, `TenantConn`, RLS, caller-owned transaction, and cross-tier rules; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md` | `HumanConnections`, `PgLoginStateStore`, `PgIssuerResolver`, `WyrdPostgres`, human-connection/login-state/refresh SQL slots, constructor sites, and the migration's forced-RLS policy | PASS |
| One Active / one Candidate and lifecycle serialization | SPEC REQ-002/003/014/016; original task lifecycle contract | Partial unique indexes in `20260925000000_auth_human_connections.sql:127-134`; tenant advisory transaction lock and exact-state writes in `queries/auth/human_connections.rs:23-106,155-342`; `HumanConnections::{put_candidate,stamp_candidate,activate,deactivate,remove}` | PASS |
| Session provenance and immediate lifecycle cutoff | SPEC REQ-016; R1 `FIND-TASK-001-5`; R2 `FIND-TASK-001-5` | Login state stores exact connection id/revision (`login_state.rs:10-86`); refresh rows persist/copy that binding (`refresh_tokens.rs:31-49,129-165`); issuance takes the shared slot lock and rechecks exact Active state before mint/insert (`issuance.rs:453-528`); callback and refresh route transaction owners; cutoff journey and focused refresh/issuance tests | PASS |
| Activation, mutation, and audit transactionality | SPEC REQ-003/017; agent-rules canonical audit and same-transaction decision rules; R3 `FIND-TASK-001-22` | Bearer decision append in `HumanConnections::begin_locked`; recovery credential verification, permission resolution, attributed Allowed/Denied append, promotion, and refusal in `connections.rs:437-512,632-648,925-1002`; server handler decision path; rotation journey's denied, allowed, unresolved, and injected append-failure cases | PASS |
| Migration and durable upgrade behavior | Original task migration contract; R2 migration provenance correction; SPEC INV-004 | Full `20260925000000_auth_human_connections.sql`; preflight for ambiguous/unsafe Human trust; Human-to-connection move; workload-binding preservation; login-state discard; legacy user refresh families left unbound; `pg_migration.rs::human_connection_upgrade_preflight` plus refresh owner tests | PASS |
| Replica and concurrent behavior | SPEC REQ-002/003/016; deployment topology authority | Database-resident state, transaction-scoped tenant slot lock, exact revision checks under that lock, concurrent activation journey, two-replica session-cutoff journey, and refresh token atomic `UPDATE ... RETURNING` | PASS |
| Tombstones and durable references | Original task tombstone and history contract; SPEC REQ-016/017 | Connection removal forces Inactive, wipes ciphertext, and retains the id; refresh rows reference the connection id; exact revision remains in durable session provenance; live reads exclude removed rows | PASS |

## Source conclusions

The SQL and owner boundaries are coherent. Tenant-scoped production work is
acquired through `WyrdPostgres::tenant_conn` and handed to query slots as
`&mut TenantConn<'_>`; those slots do not commit or roll back. RLS is enabled
and forced on `wyrd.auth_human_connections`, and lifecycle queries rely on that
boundary rather than adding a competing tenant selector. The only cross-tenant
connection-secret operations use the explicit `OperatorPool` and exact-row
compare-and-swap for deployment sealing-key rotation.

Every lifecycle mutation and every human-session issuance takes the same
transaction-scoped advisory lock. Consequently, activation, deactivation, or
removal either commits before the issuance check and causes refusal, or waits
until the issuing transaction commits. Login state and refresh successors carry
the exact connection id and revision; the refresh owner refuses unbound legacy
families and copies a valid predecessor's binding rather than selecting the
current provider.

Activation now appends two independently attributable decisions in its one
locked transaction when the recovery key resolves: the bearer caller's
decision and the recovery principal's Allowed or Denied decision, including the
verified non-secret credential id. Failure of either append aborts before
promotion; a committed underprivileged refusal preserves both lifecycle rows.
Malformed, unknown, cross-tenant, hash-mismatched, and inactive recovery keys
resolve no principal permission decision and retain the indistinguishable
refusal path.

The migration no longer invents provider provenance for legacy refresh rows.
It leaves pre-existing human families unbound, and the runtime owner refuses
such a family without a successor. The current migration test proves the row
shape after upgrade, while `refresh::pg_tests::rotation_refuses_an_inactive_or_unbound_connection`
proves the executable refusal and `rotation_copies_the_connection_binding`
proves bound successor behavior. This is a smaller composed proof than the R2
single-test A-to-B fixture and preserves the same material invariant; the R3
remediation record explicitly accepts that reduction.

## Prior-finding closure

| Stable finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-001-5` — bind sessions to the exact Active revision and do not infer legacy provenance | Exact binding on login state and refresh rows; shared slot lock and Active check before first issuance and renewal; migration leaves legacy rows unbound; focused refresh tests and cross-replica cutoff journey | CLOSED |
| `FIND-TASK-001-6` — remove raw `PgPool` propagation from the connection owner | `HumanConnections` and `PgLoginStateStore` own `WyrdPostgres` and acquire `TenantConn`; no raw pool remains in those owner fields or signatures | CLOSED |
| `FIND-TASK-001-7` — transactional stamp audit must represent a real permission evaluation | The server evaluates again after provider IO, and `stamp_candidate` appends that second decision under the slot lock with the exact revision stamp | CLOSED |
| `FIND-TASK-001-14` — resolver must use the sanctioned tenant capability | `PgIssuerResolver` owns `WyrdPostgres` and resolves through `tenant_conn` | CLOSED |
| `FIND-TASK-001-22` — audit the recovery principal's permission decision | `recovery_key_authorizes` appends the attributed Allowed/Denied decision on the activation transaction; the journey covers distinct principals, insufficient permission, unresolved input, and append-failure rollback | CLOSED |

The prior callback-host proposal `TD-R3-001` is not reopened: the r3 validated
ledger rejected it as unchanged base behavior outside TASK-001's declared
requirements and requiring a separate callback-routing/persistent-state
decision.

## Material proposed findings

None.

## Verification limits

- Static review only, as assigned. No Cargo, `mise`, Postgres, Keycloak, Dex,
  migration, or real-server lane was rerun.
- I inspected the complete cumulative base-to-candidate diff, current candidate
  source, committed tests, prior validated ledgers, and the verification
  evidence recorded in all three remediation files. Runtime pass claims were
  not independently reproduced in this Wave 1 review.
- The migration invariant is proven across the migration test and the owning
  refresh tests rather than one combined post-upgrade `RefreshTokens::execute`
  scenario. The code path is direct and the R3 remediation record expressly
  accepts this smaller proof, but an execution failure in their shared fixture
  or lane would not have been observed here.
- No `.codegraph/` index exists at the repository root, so caller and source
  tracing used `rg`, the complete diff, and direct source inspection.

## Overall result

**PASS**

Within this domain, the candidate satisfies the original task and all three
remediations: tenant state stays behind forced RLS and `TenantConn`, lifecycle
and issuance serialize on one durable slot lock, refresh provenance fails
closed across upgrades, connection mutations and recovery authorization are
transactionally audited, and the committed tests cover the material replica,
concurrency, migration, and rollback paths. No material tenancy, persistent
data, transaction, concurrency, lifecycle-durability, or migration finding
remains.
