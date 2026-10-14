# Tenancy, RBAC, and Information-Disclosure Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`
- Domain: tenant isolation, table-level RBAC, peer assignment authority, and public planning diagnostics

## Boundary and Source Coverage

| Boundary | Evidence inspected | Result |
|---|---|---|
| SQL table discovery, including nested subqueries | `oracle/mod.rs:1685-1709`, `oracle/mod.rs:3233-3274`; `resolve_table_references(..., true)` supplies the complete canonical relation set to `pin_cut` before DataFusion planning | PASS |
| Tenant-scoped catalog resolution | `oracle/planner.rs:99-138`, `catalog/bifrost_catalog.rs:366-437`; the authenticated `DataTenantId` binds the catalog cut and cache key | PASS |
| Table RBAC before source access | `oracle/planner.rs:117-140`, `oracle/mod.rs:3394-3446`; each resolved `TableUid` is checked before metadata/manifest materialization, provider registration, live discovery, admission, audit, or peer dispatch | PASS |
| Provider/session visibility | `oracle/mod.rs:2624-2660`, `oracle/mod.rs:2724-2767`, `oracle/mod.rs:3134-3172`; the planning session registers only providers from the already authorized, tenant-bound cut | PASS |
| Payload-column authorization through subqueries | `oracle/mod.rs:3018-3033`, `oracle/mod.rs:3448-3514`; the optimized plan is traversed with `apply_with_subqueries` before it can execute | PASS |
| Public planning diagnostics | `oracle/mod.rs:3735-3813`, `wyrd-spec/src/vala/error.rs:543-554`, `wyrd-spec/src/vala/error.rs:734-742`; logical SQL faults expose only the selected caller-fault leaf, physical-planning faults remain fixed-detail, and unauthorized tables stop at detail-free `QueryForbidden` before session registration | PASS |
| Live Scribe route and fragment tenancy | `oracle/tail_discovery.rs:52-67`, `oracle/live.rs:65-100`, `oracle/live.rs:282-337`; the authenticated cut supplies the tenant-bearing wire binding carried into each signed follower assignment | PASS |
| Follower/peer fail-closed checks | `oracle/follower.rs:918-980`, `oracle/peer.rs:430-453`, `oracle/peer.rs:795-824`; the receiver checks assignment tenant and full table binding against authenticated authority before provider resolution or I/O, and peer claims bind tenant plus permission digest | PASS |
| Scalar-subquery live-filter change | `oracle/live.rs:382-423`, `oracle/live.rs:509-523`; only a filter containing `ScalarSubqueryExpr` is retained on the leader, while the live leaf keeps the same tenant-bound routes and follower assignment path | PASS |
| User-facing negative and subquery coverage | Rust, Python, TypeScript, and MCP journey changes under `sdks/*/tests/integration` and `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`; unauthorized-table checks require `WYRD_VALA_403_QUERY_FORBIDDEN` without misspelled or suggested column names, and live scalar/`IN`/correlated `EXISTS` cases compare with joins | PASS (source inspection) |

The changed code does not construct a second table-resolution or tenant-selection path. Scalar-subquery handling occurs after authorization and changes only whether a physical filter is serialized to Scribe; it does not change route discovery, table binding, peer claims, assignment validation, or result authorization.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required for acceptance.

### Positive Controls

- Authorization uses catalog-resolved UIDs and canonical tenant-bound table identities rather than SQL spelling or aliases.
- Unauthorized table access fails before provider registration, so DataFusion cannot use the unauthorized schema to generate column suggestions.
- Public physical-planning failures keep a fixed repair message; full dependency chains remain operator diagnostics.
- Payload authorization traverses subqueries explicitly.
- Live and distributed reads carry tenant-bearing bindings into signed assignments, and followers validate them before I/O.
- Assignment authority digests cover tenant, namespace, table, schema fingerprint, provider cut, and projected columns.
- Typed query parameters become `ScalarValue` literals and are not interpolated into SQL.

## Verification Limits

No verification results were supplied, and this review did not execute the Postgres-backed ignored journey lanes. The assessment is based on the immutable cumulative diff, current source, the pinned DataFusion fork source at `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, and inspection of the added Rust, Python, TypeScript, and MCP journeys. The candidate remained at the supplied commit during review.

## Material Findings

None.

## Overall Result

**PASS**
