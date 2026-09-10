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
tests.

The new focused proof must cover the permission contract and invalid input,
tenant role decoding, Oracle's complete resolved table set and payload check,
peer-authority non-widening, and the single real-server role matrix. Keep each
case in its nearest existing test target and run each by exact nextest
expression. The Postgres-backed resolver and server journey commands must use
the repository-managed Postgres wrapper and migrations.

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:client-tier
mise run check:design-sync
mise run docs:check
git diff --check
```

Do not run the repository gate or a complete Bifrost/platform test suite.
