---
id: TASK-001
title: Implement object-scoped Bifrost RBAC
kind: implementation
status: ready
spec: SPEC-object-scoped-rbac
spec_revision: 3
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005]
---

## Objective

Implement the approved operation/object RBAC contract and enforce it on every
table resolved by Bifrost Oracle. Prove the result through one focused
real-server journey in which tenant-local `analyst` and `data_scientist` roles
have different table access.

Required execution skill: `$wyrd-implement`.

## Current Repository Facts

- `crates/shared/wyrd-runtime/src/permission.rs` currently owns the two-axis
  `Permission`, its constructors, string conversion, validation, subsumption,
  `PermissionSet`, builtin grant values, and most unit coverage.
- `crates/shared/wyrd-runtime/src/permission_check.rs` already provides the one
  synchronous `PermissionCheck::check(&Principal, &Permission)` chokepoint.
  The approved scope belongs in the required `Permission`; the trait needs no
  target parameter or second checker.
- `wyrd.auth_roles.permissions` already persists role permissions as JSONB.
  `SqlPermissionResolver::resolve` in
  `crates/wyrd/wyrd-auth/src/permission_resolver.rs` already decodes that JSON
  into the principal's `PermissionSet` through a tenant-bound connection.
- `query::service::stream_query` currently requires the global
  `Permission::bifrost_query_read()` before entering Oracle. That exact-global
  check would incorrectly reject every schema- or table-scoped grant.
- `Oracle::prepare_query_attempt` resolves SQL references through
  `OraclePlanner::pin_cut`. Each resulting `PinnedSealedTable` carries the
  canonical `TableRef` and stable `TableUid`; this is the first trustworthy
  complete object set for authorization. Parsed SQL names are not an
  authorization boundary.
- Bifrost's public logical identity is catalog `vala`, schema such as `logs`
  or `traces`, and table such as `records` or `spans`. The internal flattened
  namespace (`vala.logs`) and physical Iceberg catalog name (`wyrd-redux`) are
  not permission-scope values.
- `authorize_payload_projection` already runs before physical-plan creation,
  but `payload_permission` returns an unscoped permission selected by a
  hard-coded table category. This task scopes those existing payload
  permissions; it does not redesign sensitive-column classification.
- `AuthorizedQueryContext.permission` is currently a `resource:action` string,
  and Oracle derives peer `permission_digest` values from it. Scoped authority
  must not be collapsed back to that object-free string.
- The server journey target at
  `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs` already exercises
  the real query server and is the narrowest home for the required role
  matrix.

## Constraints

- Preserve every invariant and non-goal in spec revision 3.
- Keep a single role JSON field, `PermissionSet`, and synchronous
  `PermissionCheck`. Add no grant table, policy lookup, query-path database
  lookup, cache, checker, string/glob scope language, or dependency.
- Scope is required. Update every repository-owned permission value to use
  explicit `All`; reject scope-less JSON rather than retaining an unshipped
  compatibility form.
- Put public/persisted permission and Bifrost scope identity types in the
  dependency-safe contract layer and keep runtime resolution/checking in
  `wyrd-runtime`. `wyrd-spec` remains IO-free, async-free, and PyO3-free.
- Reuse the existing Bifrost table UID and catalog resolution. Do not add a
  second table identity or a catalog/schema registry.
- Tenant identity comes only from the verified principal and tenant-bound
  catalog lookup. It is never accepted from permission scope or query input.
- A scoped query grant may pass only coarse query-route admission; Oracle's
  resolved-object decision is authoritative. Objectless query-lifecycle
  controls retain their existing global required permission and do not
  manufacture a table target.
- Preserve the existing stable payload-denial behavior and fail every
  multi-table query as one decision before returning rows.
- Do not add production `analyst` or `data_scientist` builtin roles. They are
  tenant-local journey fixtures only.
- Update generated artifacts through their generators only.
- Do not run `mise run gate`, `mise run test:bifrost`, or another complete
  platform/family test suite.

## Relevant Surface

- Permission contract and runtime:
  - `crates/wyrd-spec/src/auth/`
  - `crates/shared/wyrd-runtime/src/permission.rs`
  - `crates/shared/wyrd-runtime/src/permission_check.rs`
  - `crates/shared/wyrd-runtime/src/builtin_roles.rs`
- Role persistence and resolution:
  - `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql`
  - `crates/wyrd/wyrd-auth/src/permission_resolver.rs`
  - repository-owned role fixtures and permission literals
- Bifrost identity and authorization:
  - `crates/vala/vala-bifrost-redux/src/catalog/`
  - `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`
  - `crates/vala/vala-bifrost-redux/src/oracle/peer.rs`
  - `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs`
  - `crates/wyrd/wyrd-server/src/query/service.rs`
  - `crates/wyrd/wyrd-server/src/oracle/forwarding.rs`
  - `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs`
- Contract projection, tests, and documentation:
  - `crates/wyrd-spec/src/vala/api.rs`
  - `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs`
  - `architecture/v1/00-foundations/permission-model.md`
  - `architecture/v1/00-foundations/permission-check.md`
  - `architecture/wyrd-design.md`
  - `architecture/wyrd-security-posture.md`
  - `architecture/bifrost-design.md`
  - `docs/src/content/docs/concepts/authorization.svx`
  - `docs/src/content/docs/concepts/identity-and-auth.svx`
  - affected generated schemas and permission documentation

Paths identify the known implementation seams, not an allowlist. Update other
direct constructors, fixtures, or public projections found by compilation and
contract generation; do not widen the task into unrelated authorization work.

## Approach

1. Define the required typed `PermissionScope` and nested Bifrost schema/table
   identities in the contract owner, including exact approved serde shape,
   validation, and stable table UID reuse. Make the runtime permission model
   consume that contract, update all constructors/literals to explicit `All`,
   and extend `PermissionSet` coverage to resource, action, and scope.
2. Keep `wyrd.auth_roles.permissions` as the only grant store. Update the
   existing resolver and repository fixtures to decode scoped permissions,
   reject missing or invalid scope, and preserve tenant isolation plus current
   authorization-epoch/cache behavior. Keep the existing JSONB column; add no
   grant table or SQL schema migration.
3. Replace the query route's exact-global prerequisite with coarse admission
   that recognizes an applicable Bifrost query-read capability. After
   `OraclePlanner::pin_cut` returns the complete `PinnedSealedTable` set,
   require a scoped query-read permission for every resolved table before
   provider registration, physical planning, admission, read-audit acceptance,
   peer dispatch, or data IO.
4. Build sensitive-payload requirements with the same resolved table scope and
   run them through the same `PermissionSet`/`PermissionCheck` containment.
   Preserve current payload categories and error codes. Bind the canonical
   scoped decision and exact authorized table identities into the existing
   audit and peer permission-digest boundary so forwarding and workers can
   verify, but never widen, coordinator authority.
5. Update both foundation permission documents and every conflicting active
   authority/public page. In particular, delete the claims that RBAC has no
   object scope, that `Permission` is only `{resource, action}`, or that the
   checker omits a target because RBAC is object-free. Document instead that
   the target is typed inside `Permission`, static object grants remain RBAC,
   ABAC remains in the Policy plane, and the checker signature stays
   principal-plus-permission. Update the Bifrost permission descriptor and
   regenerate affected contracts without inventing a parallel permission
   string language.
6. Add focused tests only: one permission-contract test, one scoped role
   resolver test, focused Oracle/peer tests, and one real-server query journey
   covering the complete `analyst`/`data_scientist` matrix.

## Acceptance Criteria

- The direct permission JSON exactly supports:

  ```json
  {"resource":"bifrost_query","action":"read","scope":"all"}
  {"resource":"bifrost_query","action":"read","scope":{"bifrost":{"schema":{"catalog":"vala","schema":"logs"}}}}
  {"resource":"bifrost_query","action":"read","scope":{"bifrost":{"table":{"catalog":"vala","schema":"traces","table_uid":"<canonical table uid>"}}}}
  ```

- Missing scope, malformed catalog/schema/table identity, and Bifrost scope on
  an unrelated resource are rejected. No compatibility decoder accepts the
  old two-field JSON.
- Coverage is three-axis: `All` covers every object only for the covered
  resource/action; schema scope covers current and future tables in the exact
  logical catalog/schema; table scope covers only the exact UID; wildcard
  resource/action grants object-wide access only with `All`.
- Tenant-scoped role JSON resolves through `SqlPermissionResolver` into the
  principal's existing `PermissionSet`; the existing epoch/cache invalidation
  path continues to revoke the resulting effective authority.
- Every direct, joined, expanded, or otherwise resolved Oracle table is checked
  from its catalog-resolved identity. An unresolved or unauthorized table
  denies the whole query before source data is read, audited as accepted, or
  sent to a peer.
- Schema and table checks use logical catalog `vala`, explicit schema, and the
  existing stable table UID. They do not compare aliases, raw SQL, flattened
  namespaces, or physical catalog names.
- Existing sensitive payload permissions require scope coverage for the same
  resolved table in addition to query permission. Existing payload categories
  and the stable payload-forbidden error remain unchanged.
- The scoped decision participates in existing audit, forwarding, stage-ticket,
  and worker verification. Tampering with the approved scope or table set is
  rejected before worker IO.
- No scoped authorization or permission digest reduces the decision to the old
  `resource:action` string. Public descriptors expose the typed scope without
  adding a second string permission language.
- The real-server journey proves:
  - `analyst`, granted query-read on `{catalog: vala, schema: logs}`, can read a
    non-sensitive projection from `vala.logs.records` and is denied both
    `vala.traces.spans` and a logs/traces query with zero rows returned;
  - `data_scientist`, granted query-read on the resolved UID of
    `vala.traces.spans`, can read a non-sensitive projection from that table and
    is denied both `vala.logs.records` and a traces/logs query with zero rows
    returned.
- `analyst` and `data_scientist` exist only in the test setup. All public query
  adapters continue to converge on the same server/Oracle authorization path.
- `architecture/v1/00-foundations/permission-model.md` defines operation/object
  RBAC, typed scope, JSON, validation, and three-axis coverage.
- `architecture/v1/00-foundations/permission-check.md` explicitly states that
  the required object is carried by `Permission`, explains why no `TargetRef`
  parameter is needed, and no longer equates an object-free check with RBAC.
- `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
  `architecture/bifrost-design.md`, public authorization docs, and generated
  permission contracts agree with those foundation documents; no active
  authority still rejects permission scope or describes only
  `resource:action` authorization.

## Verification

Run Cargo-backed commands sequentially. For each new focused test, record its
exact fully qualified `mise exec -- cargo nextest run --locked` command in the
implementation report after the implementer chooses the smallest existing
target. Never use a positional filter that can pass after selecting zero
tests. The following existing tests are regression anchors, not substitutes
for the new scoped cases.

The new focused proof must cover the permission contract and invalid input,
tenant role decoding, Oracle's complete resolved table set and payload check,
peer-authority non-widening, and the single real-server role matrix. Keep each
case in its nearest existing test target and run each by exact nextest
expression.

```bash
# Existing permission serialization and subsumption anchors.
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib \
  -E 'test(=permission::tests::permission_set_jsonb_round_trip) | test(=permission::tests::permission_set_subsumption_drops_redundant)'

# Existing tenant-backed role resolution anchor.
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E '\''test(=permission_resolver::pg_tests::resolved_set_matches_constant)'\'''

# Existing verifier cache and authorization-epoch anchors.
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::invalidate_principal_removes_matching_entries_and_forces_re_resolve) | test(=tests::revocation_epoch_rejects_cache_hit_when_iat_predates_epoch)'

# Existing signed peer-binding tamper anchor.
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support \
  -E 'test(=oracle::peer_authority::tests::prove_forward_query_claims_bind_every_field)'

mise run fmt
mise run lints
mise run codegen:check
mise run check:client-tier
mise run check:design-sync
mise run docs:check
git diff --check
```

Do not run the repository gate or a complete Bifrost/platform test suite.

## Implementation Report

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct permission JSON supports `all`, schema scope, table scope | `crates/wyrd-spec/src/auth/permission_scope.rs`, `crates/shared/wyrd-runtime/src/permission.rs` | `wyrd-spec auth::permission_scope::tests::scope_json_matches_the_approved_projection`; `wyrd-runtime permission::tests::scoped_permission_json_matches_the_approved_projection` | PASS |
| Missing scope, malformed identity, and Bifrost scope on an unrelated resource rejected; no compatibility decoder | `Permission` decodes only through `#[serde(try_from = "PermissionWire")]` with `deny_unknown_fields`; `Permission::validate` + `Resource::accepts_bifrost_scope` (`crates/shared/wyrd-runtime/src/permission.rs`) | `wyrd-runtime permission::tests::scope_is_required_and_has_no_compatibility_decoder`, `::bifrost_scope_is_rejected_on_an_unrelated_resource`, `::malformed_scope_identity_is_rejected_at_decode`; `wyrd-spec auth::permission_scope::tests::malformed_identities_are_rejected`, `::malformed_table_identity_is_rejected_at_decode` | PASS |
| Three-axis coverage (`All`, schema, exact UID; wildcard resource/action is object-wide only with `All`) | `Permission::covers` + `PermissionScope::covers`/`BifrostPermissionScope::covers` | `wyrd-runtime permission::tests::coverage_is_three_axis`, `::permission_set_keeps_scopes_that_do_not_subsume_each_other`; `wyrd-spec auth::permission_scope::tests::all_covers_every_object_and_is_covered_by_nothing_narrower`, `::schema_scope_covers_every_table_in_that_exact_schema`, `::table_scope_covers_only_the_exact_uid` | PASS |
| Tenant role JSON resolves through `SqlPermissionResolver` into the existing `PermissionSet`; epoch/cache revocation unchanged | No resolver change was needed — the single `Permission` decode path enforces scope (`crates/wyrd/wyrd-auth/src/permission_resolver.rs`) | `wyrd-auth permission_resolver::pg_tests::decodes_scoped_bifrost_grants_and_rejects_unscoped_rows`, `::resolved_set_matches_constant`; `wyrd-auth-verify tests::invalidate_principal_removes_matching_entries_and_forces_re_resolve`, `::revocation_epoch_rejects_cache_hit_when_iat_predates_epoch` | PASS |
| Every resolved table checked from its catalog-resolved identity; denial precedes source IO, accepted read audit, and peer dispatch | `authorize_resolved_tables` called from `OraclePlanner::pin_cut` immediately after `protect_and_materialize` (`crates/vala/vala-bifrost-redux/src/oracle/planner.rs:395`), defined at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4909` | `wyrd-testing::server query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` (join case denied, zero rows) | PASS |
| Checks use logical catalog/schema and the existing `TableUid`, not aliases, raw SQL, flattened namespaces, or physical catalogs | `resolved_table_scope` splits the pinned `TenantTableBinding` namespace and reuses `cut.table_uid` (`oracle/mod.rs:4878`) | Same journey; `vala-bifrost-redux oracle::tests::payload_permission_requires_the_resolved_table_scope` | PASS |
| Sensitive payload permissions require the same resolved-table scope; categories and payload-forbidden error unchanged | `payload_permission(table, scope)` (`oracle/mod.rs:4773`) | `vala-bifrost-redux oracle::tests::payload_permission_requires_the_resolved_table_scope`, `::payload_permission_is_absent_for_ungated_tables` | PASS |
| Scoped decision participates in audit, forwarding, stage tickets, worker verification; tampering rejected before worker IO | `scoped_permission_digest` at all three digest sites (`oracle/mod.rs:2687,4677,4737`); typed permission comparison in `crates/wyrd/wyrd-server/src/oracle/forwarding.rs` | `vala-bifrost-redux oracle::tests::scoped_permission_digest_binds_the_authorized_table_set`; `wyrd-server oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence` (widened-permission substitution added) | PASS |
| No decision reduces to the old `resource:action` string; descriptors expose typed scope with no second string language | `AuthorizedQueryContext.permission: Permission`; `Display`/`FromStr` remain operation-only for audit text; `BifrostPermissionDescriptor.scope: PermissionScope` (`crates/wyrd-spec/src/vala/api.rs`) | `mise run codegen:check`; regenerated `crates/wyrd-spec/schemas/bifrost_permission_descriptor.json` | PASS |
| Real-server journey proves the `analyst` / `data_scientist` matrix, denials with zero rows | `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs` | `wyrd-testing::server query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` | PASS |
| `analyst` / `data_scientist` are test-only; all query adapters converge on one authorization path | Roles are seeded only via `WyrdTestServer::seed_role` in that journey; no builtin-role change (`crates/wyrd/wyrd-auth/src/roles.rs` untouched except a scope literal) | Whole `server` journey binary (5/5) incl. the gRPC/scheduled-query journey | PASS |
| `permission-model.md` defines operation/object RBAC, typed scope, JSON, validation, coverage | `architecture/v1/00-foundations/permission-model.md` | `mise run docs:check` | PASS |
| `permission-check.md` states the object rides inside `Permission`, explains the absent `TargetRef`, no longer equates object-free with RBAC | `architecture/v1/00-foundations/permission-check.md` | `mise run docs:check` | PASS |
| `wyrd-design.md`, `wyrd-security-posture.md`, `bifrost-design.md`, public authorization docs, generated contracts agree; no authority still rejects scope | Those four files plus `docs/src/content/docs/concepts/authorization.svx` and `identity-and-auth.svx` | `mise run docs:check`; `mise run codegen:check` | PASS |

### Commands run

```bash
# Superseded by the exact expressions recorded in
# changes/active/object-scoped-rbac/review/task-001-codex/TASK-001-R1-close-rbac-contract-gaps.md
# (these two regex module selectors could pass after selecting the wrong set).
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::payload_permission_requires_the_resolved_table_scope) | test(=oracle::tests::payload_permission_is_absent_for_ungated_tables) | test(=oracle::tests::scoped_permission_digest_binds_the_authorized_table_set)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support \
  -E 'test(=oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence)'
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::invalidate_principal_removes_matching_entries_and_forces_re_resolve) | test(=tests::revocation_epoch_rejects_cache_hit_when_iat_predates_epoch)'
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E '\''test(=permission_resolver::pg_tests::resolved_set_matches_constant) | test(=permission_resolver::pg_tests::decodes_scoped_bifrost_grants_and_rejects_unscoped_rows)'\'''
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise run test:bifrost:journey:server:inner'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise run test:bifrost:journey:oracle:inner'
mise run fmt && mise run lints && mise run codegen:check
mise run check:client-tier && mise run check:pyo3-scope && mise run docs:check
git diff --check
```

### Corrections to the task's verification block

- `test(=oracle::peer_authority::tests::prove_forward_query_claims_bind_every_field)`
  selects zero tests: that name is a helper function, not a test. The real
  signed-peer tamper anchor is
  `oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence`,
  which is what was run and extended.
- The scoped role-decode case lives inside `permission_resolver::pg_tests`
  (its module home in the file), so its expression is
  `test(=permission_resolver::pg_tests::decodes_scoped_bifrost_grants_and_rejects_unscoped_rows)`.

### Pre-existing failures, unchanged by this task

- `mise run check:unwrap-audit` fails on `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs`.
  Identical 10 findings at the task's base commit `1c4b518db` (verified in a
  throwaway worktree); no file this task touched is flagged.
- `mise run check:design-sync` fails because `architecture/wyrd-design.md` never
  named `ValaQueryService` or the four `Bifrost*Payload` resources. Identical 6
  failures at `1c4b518db`. Out of this task's write set.

### Material limits

- `clippy::result_large_err` fired once the scope axis grew `Permission` past
  128 bytes. `PermissionDenyReason::Rbac.required` is now `Box<Permission>`,
  which is the lint's own remedy and keeps the check's `Result` small. No
  public wire shape changed.
- `authorize_resolved_tables` has no isolated unit test: constructing a
  `PinnedSealedTable` requires a live `iceberg::table::Table`. Its two pure
  halves (`resolved_table_scope` identity projection, three-axis coverage) are
  unit tested, and the function itself is proven end to end by the journey's
  six-case role matrix, including the join case.
- The journey's accepted queries stream zero rows: the built-in tables are
  provisioned but not ingested into. Acceptance therefore proves that
  authorization, admission, audit, and execution completed; it does not assert
  row content. Denials assert a pre-stream `WYRD_VALA_403_QUERY_FORBIDDEN` and
  zero rows.
- Non-goals stayed excluded: no grant table, policy lookup, query-path database
  lookup, permission cache, second checker, string/glob scope language, second
  table identity, catalog/schema registry, or new dependency was added.
