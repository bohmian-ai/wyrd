# Domain Review — Tenancy and Security

## Review Subject

- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Immutable candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Cumulative range: `c1508b375..7fcb45fc1`
- Remediation range: `e8d3cca13..7fcb45fc1`
- Overall result: **PASS**
- Material findings: **none**

The checkout had advanced to `5ab92b003` before this review, so every production
source statement below was read with `git show 7fcb45fc1:<path>` and every diff
with an explicit candidate endpoint. The reviewed candidate identity did not
change.

## Authority and Boundary Coverage

| Authority or boundary | Evidence inspected | Result |
|---|---|---|
| Approved spec revision 11 | REQ-004 private Forge routing, REQ-007 destructive maintenance, REQ-014 tenant-scoped Oracle selection and active reads, and the revision history that removed the older reader-cut machinery | PASS |
| Repository security rules | `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, and the routed architecture constraints/patterns | PASS |
| Prior security review and retained findings | Prior tenancy/security report, validated 17-finding ledger, and TASK-001-R1 through TASK-PACKET-R1; no prior security finding existed, and the security properties adjacent to the remediations remain intact | PASS |
| Tenant SQL boundary | Complete cumulative SQL migration/query diff plus the remediation delta in `oracle_reader_authority`, `olap_catalog`, `forge_operations`, `forge_tasks`, `file_list`, and Scribe query owners | PASS |
| Private Forge peer | `grpc/forge_peer.rs`, private-router composition in `grpc/mod.rs`, server TLS loading/serving, peer admission, election fencing, and worker dial-only configuration | PASS |
| Destructive object/catalog effects | Snapshot expiry, expired-file cleanup, and orphan cleanup from exact table identity through exclusive authority, path validation, external effect, and durable settlement/recovery | PASS |
| Supply chain and gate allowlists | Workspace manifest/lockfile pin change and every newly allowlisted wiremock use plus its manifest dependency class | PASS |

## Exact Source Coverage

| Surface | Source evidence | Security conclusion |
|---|---|---|
| New tenant relations | `20260910000025_oracle_reader_authority.sql:20-116` gives both maintenance-authority and active-read tables tenant-qualified keys and foreign keys, enables and forces RLS, installs exact `wyrd.current_tenant()` policies, revokes `PUBLIC`, and grants only the roles that consume them | Tenant rows cannot cross the request role's RLS boundary; operator privileges are limited to maintenance operations. |
| Catalog pointer definer | `20260910000025_oracle_reader_authority.sql:122-174` uses one `SECURITY DEFINER` function owned by `wyrd_platform_admin`, `search_path = ''`, fully qualified objects, no dynamic SQL, a fixed catalog, a canonical `vala.<segment>` namespace, and a physical namespace derived solely from `wyrd.current_tenant()` | A caller cannot inject SQL, supply a tenant/catalog/physical namespace, or enumerate catalog rows through the definer. |
| Catalog grants | `20260619000000_iceberg_catalog.sql:17-23` grants schema ownership only to `wyrd_platform_admin`, revokes all catalog-table access and schema usage from `wyrd_app`, and revokes schema access from `PUBLIC`; no later migration grants `iceberg_catalog` to `wyrd_app` | Oracle reaches one tenant-relative pointer only through the narrow definer; the request role has no direct catalog capability. |
| One-statement cut acquisition | `20260910000025_oracle_reader_authority.sql:197-326` is `SECURITY INVOKER`, rejects malformed/empty requests and non-positive fences/deadlines, resolves and locks only RLS-visible authority rows, derives `data_tenant_id` from session state, parameterizes all inputs, and returns RLS-visible hot rows | Missing or foreign tables fail the whole acquisition; no partial claim, tenant override, or SQL injection path exists. |
| Rust acquisition owner | `vala-sql/src/queries/oracle_reader_authority.rs:360-457` owns a borrowed `TenantConn`, binds the JSON request as a value, validates positive bounded remaining time, validates decoded tenant/table identity, and releases only the current tenant's query rows under RLS | The Rust boundary cannot substitute a raw/admin connection or interpolate caller input into SQL. |
| OperatorPool layout lookup | `vala-sql/src/queries/olap_catalog.rs:100-127` is the sole relocated layout read, explicitly takes both `OperatorPool` and typed `DataTenantId`, and predicates on `data_tenant_id = $1 AND fqn = $2`; Redux callers no longer own raw `bifrost_tables` SQL | The cross-tenant lane stays explicit and returns only the requested tenant/table row; moving SQL ownership did not widen it. |
| Exclusive destructive capability | `oracle_reader_authority.rs:56-122,211-286`, `forge/table_authority.rs:101-181`, and the `forge_operations`/`forge_tasks` remediation require a live borrowed `ExclusiveTableAuthority` and compare tenant, table UID, catalog, namespace, and table name before preparation/effect | An authority for one tenant/table cannot authorize another, and the API cannot reduce authority to a stale boolean before destruction. |
| Snapshot expiry | `forge/expire.rs:267-323,711-780,1097-1137` acquires a tenant connection for the exact table, holds the exclusive capability through the catalog commit's known outcome, and preserves uncertain outcomes for reconciliation | A reader either claims first and blocks expiry or waits and observes the later pointer; failures do not authorize a different table or an untracked retry. |
| Expired cleanup | `forge/worker.rs:7970-8055,8106-8298` reacquires exact tenant/table authority per candidate, revalidates durable protection and the candidate's table-rooted object path, rechecks the lease fence, and holds authority through the delete outcome before settlement | Stored or replayed candidate text cannot escape the table prefix or delete another tenant's object. |
| Orphan cleanup | `forge/orphan_gc.rs:1398-1468,1538-1609` checks exact authority coverage, normalizes and validates each candidate against the table binding, re-stats it, rechecks protection/fence, and performs bounded deletion while the table capability remains live | Listing results are not deletion authority; every object is rebound to the exact tenant/table before deletion. |
| Forge peer authentication and authorization | `wyrd-server/src/grpc/mod.rs:145-180,328-479` mounts Forge only on the mutual-TLS peer router, requires the dedicated CA and fixed `wyrd-peer` leaf identity before polling a body, applies request-size admission, and exposes no Forge service on the public router; `forge_peer.rs:97-171` validates typed inputs and requires the exact live leader fence | Public tokens cannot reach Forge RPCs; a stale term cannot notify, pull, or report. This matches the stated model in which compromise of a process holding the shared peer key is out of scope. |
| Table identity on peer messages | `forge_peer.rs:58-81,148-168` parses tenant IDs, constructs validated table identities under the fixed Bifrost catalog, parses UUID/outcome values, and passes every request through the live-term state machine | Malformed identities fail before scheduling state changes; reports cannot settle a different current task. |
| Postgres coordination time | Leader acquire/renew/current SQL and active-read abandonment use `statement_timestamp()`; the remediation does not use a host clock to decide SQL eligibility or expiry | A pod clock cannot extend leadership, expire a reader, or select a SQL coordination winner. |
| Wiremock allowlist | New entries cover `wyrd-client/src/storage/upload/tests.rs` included only by `#[cfg(test)]`, the `#[cfg(test)]` module in `wyrd-client/src/bifrost/grpc.rs`, and the external integration test `wyrd-server/tests/pg_verification_routes.rs`; both crates declare wiremock only under `[dev-dependencies]` | The sanctioned check was narrowed to three real test-only seams and does not permit a production mock dependency or security-control bypass. |
| Dependency pin | `Cargo.toml` and `Cargo.lock` move `iceberg-compaction-core` from one literal SHA to literal SHA `380a4d0717e1786b95c4aa9f257579af496b3c8c`; no branch, tag, wildcard, or new runtime dependency is introduced | The remediation does not add a floating supply-chain input. |

## Verification and Limits

- `git diff --check c1508b375..7fcb45fc1`: **PASS**.
- `git diff --check e8d3cca13..7fcb45fc1`: **PASS**.
- The task evidence records `mise run check:tenant-isolation`, `mise run
  verify:bifrost`, `mise run test:principals:integration`, the SQL lane, the
  Oracle/Forge journeys, and the final `mise run gate` as passing.
- I did not rerun worktree-based commands because the checkout is later than the
  immutable candidate. The source-level review used commit-scoped reads, and I
  treated the recorded commands as supplied evidence rather than proof of code
  I had not inspected.
- The shared peer leaf intentionally identifies a trusted cluster process, not
  an individual role or tenant. The normative security posture explicitly
  excludes a compromised cluster member holding that key; this review does not
  reinterpret that approved boundary as a finding.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None material.

### Positive Controls

- Tenant tables have tenant-qualified relational constraints plus enabled and
  forced RLS; tenant request paths use `TenantConn`.
- Cross-tenant engine work uses typed `OperatorPool` owners with explicit tenant
  predicates or exact tenant/table capabilities.
- The one catalog definer has an empty search path, fully qualified names,
  caller-independent tenant derivation, no dynamic SQL, and no `PUBLIC` grant.
- `wyrd_app` has neither schema usage nor table privileges on
  `iceberg_catalog`.
- All reviewed SQL values are bound parameters; no command, SQL, template, path,
  or deserialization injection path was found.
- Object deletion revalidates the exact tenant/table binding, catalog roots,
  object metadata, and storage path immediately before the effect.
- Private Forge RPC is absent from public routing, mutually authenticated before
  body processing, request-bounded, typed, and fenced to the live leader term.
- Security-relevant errors on public query paths remain non-enumerating; Forge
  peer details are confined to the authenticated internal plane.
- The changed git dependency remains pinned to an immutable commit, and the
  three gate allowlist additions are dev-only test seams.

## Findings

No `MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` finding was
confirmed in the tenancy/security domain.

## Conclusion

**PASS.** Candidate `7fcb45fc1` preserves tenant isolation and closes the
remediated authority ownership at the exact tenant/table boundary without
widening SQL, catalog, object-store, peer, or dependency trust.
