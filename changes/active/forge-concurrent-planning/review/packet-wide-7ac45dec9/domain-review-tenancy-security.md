# Domain Review — Tenancy and Security

## Review Subject

- Candidate: `7ac45dec99535c881b7a936c66623044f15d8823`
- Range: `c1508b375..7ac45dec9`
- Domain result: **PASS**
- Material findings: **none**

The candidate remained at the immutable commit above throughout this review. I screened the complete changed-file set and diff, then traced the security-sensitive SQL, Oracle read acquisition, Forge destructive operations, private peer transport, public compaction registration, identity validation, and their tests to current source.

## Boundary Reviewed

- Every migration and SQL query module added or materially changed by this packet, with emphasis on `TenantConn`, forced RLS, explicit tenant binding for `OperatorPool`, grants/revokes, and function execution context.
- The active-table read cut from the public Oracle query context through acquisition, transaction commit, Iceberg metadata/file planning, object-path validation, and claim release.
- Every Forge cleanup, closeout, promotion, worker-pull, and worker-result path that can expose or destroy tenant-scoped data.
- The Forge peer gRPC listener, mutual-TLS identity check, request bounds, and absence from the public application router.
- Catalog-pointer access and the prohibition on direct `iceberg_catalog` access by `wyrd_app`.
- Tenant-qualified logical and physical table identities, cross-tenant failure behavior, and error redaction.
- Dependency revisions in manifests and the lockfile for floating or unpinned supply-chain inputs.

## Authority Coverage

| Authority | Coverage and conclusion |
|---|---|
| Approved spec revision 11 and revision history | Reviewed the active-read authority, deadline, release, destructive-maintenance, tenant-isolation, and deletion constraints against revision 11. Implementation matches the current authority rather than superseded reader-cut designs. |
| Task packet and task README | Reviewed TASK-001 through TASK-006 and TASK-005-R1 claims relevant to leader/worker privileges, active reads, maintenance, deletion, and RisingWave comparison requirements. No tenancy/security conflict found. |
| `AGENTS.md`, `architecture/agent-rules.md` | Checked tenant isolation, server ownership, typed identities, structured errors, SQL/runtime boundaries, and repository security rules. No material violation found. |
| `architecture/wyrd-security-posture.md` | Applied the documented threat boundary: tenant callers and public clients are hostile; trusted cluster peer compromise is out of scope; private peer authentication must still be enforced. The implementation stays inside that boundary. |
| `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md` | Checked Oracle/Forge ownership, PostgreSQL coordination, private worker protocol, Bifrost tenant paths, and fail-closed destructive maintenance. No drift found. |
| `architecture/references/domain/{olap-serving,iceberg}.md` and architecture constraints | Checked RLS, catalog isolation, Iceberg pointer mediation, path validation, and dependency placement. No conflict found. |

## Source Coverage

| Surface | Evidence checked | Result |
|---|---|---|
| Reader-authority migration | `20260910000025_oracle_reader_authority.sql`: tenant-qualified keys/FKs; `ENABLE` and `FORCE ROW LEVEL SECURITY`; exact current-tenant policies; `SECURITY INVOKER` acquisition; narrow catalog-pointer `SECURITY DEFINER`; empty `search_path`; owner, revoke, and grant statements | PASS |
| Catalog privilege boundary | Base Iceberg migration revokes all table access and schema usage from `wyrd_app` and `PUBLIC`; candidate tests temporarily grant then revoke access only to prove the schema guard detects widening | PASS |
| Oracle acquisition | `oracle_reader_authority.rs`, Oracle planner/module, and `BifrostCatalog`: acquisition uses a tenant connection, commits before object IO, validates returned identity, validates tenant-rooted metadata/data paths, and releases only after the plan no longer needs IO | PASS |
| Public query context | `AuthorizedQueryContext::try_new` refuses principal/data-tenant mismatch; distributed journey proves a foreign tenant receives the same class of failure as a missing object and does not leak the foreign tenant identifier | PASS |
| Forge destructive paths | `forge_operations.rs`, Forge task SQL, and row decoding: bind current tenant before tenant-local work; internal BYPASSRLS reads carry tenant-qualified identity; maintenance-authority and active-read locks precede destructive action; expiration uses PostgreSQL statement time | PASS |
| Forge worker/admin transport | `forge_peer.rs`, peer router, boot/config: service exists only on the private mutual-TLS router; client certificate and fixed peer DNS identity are verified before request handling; bodies are bounded; no public Forge route was added | PASS |
| Identity and path validation | `tenant_table.rs`, acquired-cut materialization, and cleanup candidates reject foreign catalog/namespace/table identities, absolute paths, URI paths, traversal, backslashes, and malformed separators | PASS |
| Public compaction type path | Authenticated caller tenant is used for describe/register; RBAC precedes catalog access; audit behavior is preserved; the compaction type is a closed typed value | PASS |
| Dependency changes | Changed git dependencies remain pinned to immutable commit revisions; no wildcard or floating branch was introduced | PASS |
| Tests | `pg_oracle_membership.rs` checks exact function privileges, invoker/definer mode, owner/search path, direct catalog denial, two-tenant same-name isolation, RLS-hidden claims, atomic failure, serialization, expiry, and tenant-scoped release | PASS |

## Verification

- Re-ran `mise run check:tenant-isolation`: **PASS**.
- Re-ran the active-read SQL integration test with its actual nextest-qualified name, `pg_tests::active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired`: **PASS (1/1)**.
- Confirmed the candidate remained exactly `7ac45dec99535c881b7a936c66623044f15d8823` after verification.
- The packet records successful full `verify:bifrost` and principals-integration lanes. I treated those as recorded evidence, not as substitutes for source review.

### Verification Limits

- The exact S1 command recorded in TASK-005-R1 uses `test(=active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired)`. On this candidate it selects zero tests because the binary-qualified nextest name is `pg_tests::active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired`. The corrected exact command passes. This is an evidence-command defect, not an implementation or security defect, but the recorded command should not be cited as proof without the module prefix.
- I did not re-run the full packet-wide gate because the supplied immutable-candidate evidence reports it green and the focused tenancy gate plus exact SQL integration test directly exercise this domain.
- A compromised trusted cluster certificate remains outside the repository's stated threat model. Within that model, the private Forge peer service correctly authenticates transport identity and is not mounted publicly.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None material. `wyrd_app` has several RLS-confined DML privileges on the new coordination tables, including verbs not exercised by every current path. Direct tenant database access is not a supported trust boundary, forced RLS prevents cross-tenant access, and corrupting maintenance authority fails destructive work closed, so this is not an exploitable issue under the approved model.

### Positive Controls

- New tenant tables use tenant-qualified primary/foreign keys and both enabled and forced RLS.
- The ordinary acquisition function is `SECURITY INVOKER`; the sole definer is narrow, has an empty search path, derives tenant identity from session state, permits only the fixed logical namespace shape, performs no dynamic SQL, and exposes only the catalog pointer.
- `PUBLIC` execution is revoked and direct `iceberg_catalog` access is denied to `wyrd_app`; tests pin both controls.
- SQL inputs are parameterized. No user-derived SQL, command, template, or filesystem-path interpolation was found on reviewed paths.
- OperatorPool paths explicitly bind tenant context and carry tenant identity in every cross-tenant internal row; tenant-local public paths use `TenantConn`.
- Destructive maintenance is fail-closed behind table authority and active-read expiry, with PostgreSQL—not host time—owning coordination time.
- Object-store paths are tenant-rooted and structurally validated before IO or deletion.
- Cross-tenant object lookup is deliberately non-enumerating and error output is redacted at public boundaries.
- Private worker RPC is separated from public routing and protected by mutual TLS and a fixed peer identity.
- Supply-chain changes use immutable git revisions.

## Conclusion

**PASS.** No exploitable cross-tenant access, privilege escalation, catalog bypass, SQL/path injection, public worker exposure, or sensitive query leakage was found in the immutable candidate. The only review caveat is the stale zero-test S1 command recorded in task evidence; the correctly qualified test passes and does not change the implementation verdict.
