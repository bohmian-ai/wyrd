---
id: TASK-001-R1
title: Close object-scoped Bifrost RBAC acceptance gaps
kind: remediation
status: ready
spec: SPEC-object-scoped-rbac
spec_revision: 3
original_task: changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md
base: 1c4b518dbb69dd10b9107c06e8ccb410c67c5449
reviewed_candidate: 3de6aed2f86b4bb039e85c7f62c0a48e12602282
findings: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7]
---

# Remediate TASK-001 object-scoped Bifrost RBAC

Required execution skill: `$wyrd-implement`.

## Authorities and immutable inputs

- Approved specification: `changes/active/object-scoped-rbac/spec.md`, revision 3
- Original task: `changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md`
- Review verdict: `changes/active/object-scoped-rbac/review/task-001-codex/verdict.md`
- Original base: `1c4b518dbb69dd10b9107c06e8ccb410c67c5449`
- Reviewed cumulative candidate: `3de6aed2f86b4bb039e85c7f62c0a48e12602282`

Remediation is cumulative: preserve the accepted behavior in the reviewed range and correct only the findings below.

## Diagnosis and correction

### FIND-TASK-001-1 — authorize before source materialization

The complete table-reference list is prepared in `OraclePlanner::attempt_protected_cut`, and each `PreparedReaderIdentity` already carries the tenant-bound canonical binding and stable table UID needed to form the required object permission. The current code delays `authorize_resolved_tables` until `protect_and_materialize` has acquired the reader guard, revalidated catalog state, and opened Iceberg manifests/hot-cut state. That contradicts the approved before-source-IO boundary and lets an out-of-scope caller consume or observe source-materialization work.

Move the one whole-query object decision to the earliest point after every requested table has a prepared catalog identity and before reader-guard acquisition, revalidation, or cut materialization. Reuse the existing `PermissionSet` containment and the prepared binding/UID; do not parse SQL names again, add a lookup, authorize tables incrementally, or create a second identity. One uncovered identity must reject the full set before any materialization observation changes.

### FIND-TASK-001-2 — durably audit scoped object denials

The public query boundary currently audits principals that lack any query capability, but a scoped principal passes coarse admission and an out-of-scope table is refused inside Oracle with only a warning. The verified tenant and principal are available, so the security posture requires a durable denial record. Preserve the stable `WYRD_VALA_403_QUERY_FORBIDDEN` response and zero-row behavior while routing this authoritative denial through the existing tenant-bound query denial-audit mechanism. Audit failure must fail closed as audit-unavailable, and a denied object decision must never create an accepted-read event.

Keep this inside the existing query authorization/audit boundary. Do not invent a new audit store, background best-effort emitter, public error, or raw-SQL/object-name audit payload.

### FIND-TASK-001-3 — reject non-read Bifrost scopes at the shared decoder

`Permission::validate` enforces scope identity and resource compatibility but omits the action constraint. As a result, persisted or signed JSON can carry a Bifrost scope with `Write`, `Wildcard`, or `AnyOf`; wildcard then covers a required read. Correct the single shared `Permission` validation boundary so Bifrost scope is valid only when the resource is one of the approved Bifrost query/payload resources and the action is exactly `Read`. The role resolver must continue surfacing invalid stored JSON as `BadPermissionsJson` with the role name.

Do not add a compatibility decoder, a second validator, an alternate permission type, or special-case route logic.

### FIND-TASK-001-4 — make the foundation permission document literal

`architecture/v1/00-foundations/permission-model.md` contradicts its own decoder rules by describing a narrow wildcard and by showing delegation permissions without required `All` scope. Correct those statements in place: Bifrost scope attached to wildcard resource/action is invalid, and every objectless example explicitly carries `PermissionScope::All`. Keep the typed scope and operation-only display vocabulary already approved.

### FIND-TASK-001-5 — restore one struct-centered query authorization owner

The new coarse admission and durable denial workflows are free async functions threading the same `AppState` and `Caller` dependencies that the module's `ControlAudit` struct already owns for lifecycle authorization. Consolidate query-route admission, denial auditing, and lifecycle authorization/audit into one cohesive concrete owner based on that existing mechanism. Its inherent methods must make the coarse-versus-authoritative boundary and fail-closed audit behavior discoverable without adding a trait, factory, configuration surface, or second owner.

### FIND-TASK-001-6 — document only the new tests

Add concise rustdoc explaining intent and the invariant proved by each new test in:

- `crates/wyrd-spec/src/auth/permission_scope.rs`
- `crates/shared/wyrd-runtime/src/permission.rs`
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`
- `crates/wyrd/wyrd-auth/src/permission_resolver.rs`

Include the new remediation tests. Do not expand this into documentation churn on unchanged code.

### FIND-TASK-001-7 — replace aggregate selectors with exact proof

The implementation report records regex module selectors for `wyrd-spec` and `wyrd-runtime`, and only a whole server journey-binary command for the scoped role matrix. Run exact nextest expressions for every new or changed focused test and record those exact commands and results in the cumulative implementation report. Retain the corrected peer test `oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence`; do not use the task's helper name that selects zero tests.

## Preserved constraints and non-goals

- Preserve the exact three approved permission JSON projections and required-scope behavior.
- Preserve one role JSONB field, one `PermissionSet`, and one synchronous `PermissionCheck` implementation.
- Preserve verified-principal tenancy, stable table UID authority, schema containment, payload categories/errors, epoch/cache behavior, forwarding/stage binding, and all public query adapters.
- Preserve the six-case `analyst`/`data_scientist` real-server matrix and keep both roles test-only.
- Add no grant table, migration, policy lookup, query-path database lookup, cache, checker, string/glob language, table identity, catalog/schema registry, dependency, builtin role, compatibility path, new audit store, or public permission/error surface.
- Do not run `mise run gate`, `mise run test:bifrost`, or another complete platform/family suite.

## Acceptance criteria

- **FIND-TASK-001-1:** With at least one uncovered prepared table in a direct or mixed query, Oracle returns `WYRD_VALA_403_QUERY_FORBIDDEN` before reader-guard/materialization/source-IO evidence changes; every covered table still reaches normal planning.
- **FIND-TASK-001-2:** One public scoped-object denial writes exactly one tenant-bound durable denial event, writes no accepted-read event, returns the existing query-forbidden code, and substitutes audit-unavailable when denial auditing fails.
- **FIND-TASK-001-3:** Direct `Permission` decoding and tenant role decoding reject Bifrost-scoped `write`, `wildcard`, and `any_of` actions; exact `read` remains accepted for every approved Bifrost scoped resource.
- **FIND-TASK-001-4:** The foundation permission document contains no scope-less `Permission` example and does not describe a valid narrow wildcard resource/action grant.
- **FIND-TASK-001-5:** New/modified query authorization and denial-audit IO are inherent methods on one cohesive dependency-owning struct; no parallel trait or owner is introduced.
- **FIND-TASK-001-6:** Every new test function in the cumulative diff, including remediation tests, has intent/invariant rustdoc, with no unrelated documentation churn.
- **FIND-TASK-001-7:** The report contains a successful exact focused command for every named test; `cargo nextest list` or equivalent source inspection confirms every expression selects its intended test.
- All original TASK-001 acceptance criteria still pass on the cumulative candidate and every original non-goal remains excluded.

## Focused and broader verification

Run Cargo-backed commands sequentially through `mise`.

1. Run exact `wyrd-runtime` tests for approved JSON, required scope, invalid resource, each new invalid-action case, three-axis coverage, operation admission, and set subsumption.
2. Run exact `wyrd-spec` scope serialization, containment, and malformed-identity tests.
3. Under the repository Postgres wrapper, run exact `wyrd-auth` scoped-role decode/rejection and existing resolution tests.
4. Run an exact Oracle test proving denial precedes reader materialization/source IO, plus the existing exact payload and scoped-digest tests.
5. Run an exact server test for durable scoped denial/audit failure behavior and the exact peer-authority tamper test.
6. Run the exact real-server role-matrix journey expression through its repository-managed Postgres setup.
7. Run the existing exact authorization-epoch/cache tests.
8. Run `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run check:client-tier`, `mise run check:pyo3-scope`, `mise run docs:check`, and `git diff --check`.

Do not claim the known pre-existing `check:design-sync` or `check:unwrap-audit` failures as remediated unless the cumulative write set actually changes their causes.

## Implementation Report

Cumulative candidate: this remediation branch, base `1c4b518dbb69dd10b9107c06e8ccb410c67c5449`.

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| **FIND-TASK-001-1** uncovered prepared table refused before reader guard, revalidation, materialization, or source IO; covered tables still plan | `authorize_resolved_tables(context, &prepared)` in `OraclePlanner::attempt_protected_cut` (`crates/vala/vala-bifrost-redux/src/oracle/planner.rs`), immediately after the prepare loop and before `acquire_guard`; `resolved_table_scope` now projects any `(TenantTableBinding, TableUid)` pair (`oracle/mod.rs`); `pin_cut` no longer authorizes | `vala-bifrost-redux::integration oracle::reader_authority::object_denial_precedes_reader_guard_and_materialization` (denied set leaves `sealed_pin_count_for_test() == 0`; granted set materializes through the same sequence) | PASS |
| **FIND-TASK-001-2** one scoped-object denial writes exactly one durable tenant-bound denial event, no accepted-read event, keeps `WYRD_VALA_403_QUERY_FORBIDDEN`, substitutes audit-unavailable on append failure | `QueryAuthority::record_object_denial` called from `stream_query` on `BifrostError::QueryForbidden` (`crates/wyrd/wyrd-server/src/query/service.rs`); reuses `audit::record_audit_owned` on the existing tenant outbox; no object name or SQL in the payload | `wyrd-testing::server query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` (asserts `vala.query.sync` +1, `bifrost.query.read_decision` +0, and `WYRD_VALA_500_AUDIT_UNAVAILABLE` under the armed append fault) | PASS |
| **FIND-TASK-001-3** Bifrost-scoped `write`, `wildcard`, `any_of` rejected at direct decode and tenant role decode; exact `read` still accepted for every approved Bifrost resource | action constraint added to the single `Permission::validate` boundary (`crates/shared/wyrd-runtime/src/permission.rs`) | `wyrd-runtime permission::tests::bifrost_scope_is_rejected_on_a_non_read_action`; `wyrd-auth permission_resolver::pg_tests::rejects_bifrost_scope_on_a_non_read_action` (surfaces the role name) | PASS |
| **FIND-TASK-001-4** no scope-less `Permission` example and no valid narrow wildcard described | `architecture/v1/00-foundations/permission-model.md` wildcard statement and both delegation examples | `grep -nE "Permission \{\|\{ [A-Z]\w+, [A-Z]\w+ \}"` returns only the two `scope`-bearing forms; `mise run docs:check` | PASS |
| **FIND-TASK-001-5** query admission, denial audit, and lifecycle authorization/audit are inherent methods on one owner | `QueryAuthority` replaces `ControlAudit` and the free `authorize_audited` / `admit_query_capability` / `record_denial` functions; `mcp/probe.rs` calls `QueryAuthority::authorize` | `mise run lints`; `wyrd-server query::service::tests::bifrost_query_authz_precedes_oracle_role_lookup` | PASS |
| **FIND-TASK-001-6** every new test function has intent/invariant rustdoc | rustdoc added to the seventeen prior tests plus the three remediation tests in `permission_scope.rs`, `permission.rs`, `oracle/mod.rs`, `permission_resolver.rs`, `reader_authority.rs` | source inspection; no unchanged item documented | PASS |
| **FIND-TASK-001-7** exact focused command recorded and green for every named test | commands below | each command's own nextest summary | PASS |
| Original TASK-001 criteria and non-goals | unchanged behavior preserved; no grant table, migration, policy lookup, query-path DB lookup, cache, checker, string/glob language, table identity, registry, dependency, builtin role, compatibility path, new audit store, or new public error added | `test:bifrost:journey:oracle` (28/28), `test:bifrost:journey:server` (5/5), `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `docs:check` | PASS |

### Exact focused commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib \
  -E 'test(=permission::tests::scoped_permission_json_matches_the_approved_projection) | test(=permission::tests::scope_is_required_and_has_no_compatibility_decoder) | test(=permission::tests::bifrost_scope_is_rejected_on_an_unrelated_resource) | test(=permission::tests::bifrost_scope_is_rejected_on_a_non_read_action) | test(=permission::tests::malformed_scope_identity_is_rejected_at_decode) | test(=permission::tests::coverage_is_three_axis) | test(=permission::tests::covers_operation_admits_a_scoped_grant_without_authorizing_an_object) | test(=permission::tests::permission_set_keeps_scopes_that_do_not_subsume_each_other)'
# 8 tests run: 8 passed

mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
  -E 'test(=auth::permission_scope::tests::all_covers_every_object_and_is_covered_by_nothing_narrower) | test(=auth::permission_scope::tests::schema_scope_covers_every_table_in_that_exact_schema) | test(=auth::permission_scope::tests::table_scope_covers_only_the_exact_uid) | test(=auth::permission_scope::tests::scope_json_matches_the_approved_projection) | test(=auth::permission_scope::tests::malformed_identities_are_rejected) | test(=auth::permission_scope::tests::malformed_table_identity_is_rejected_at_decode)'
# 6 tests run: 6 passed

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E '\''test(=permission_resolver::pg_tests::resolved_set_matches_constant) | test(=permission_resolver::pg_tests::decodes_scoped_bifrost_grants_and_rejects_unscoped_rows) | test(=permission_resolver::pg_tests::rejects_bifrost_scope_on_a_non_read_action)'\'''
# 3 tests run: 3 passed

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features "test-support,bench-support" -P journey --run-ignored=all -E "test(=oracle::reader_authority::object_denial_precedes_reader_guard_and_materialization) | test(=oracle::reader_authority::catalog_promotion_between_prepare_and_materialize_restarts_all_tables)"'
# 2 tests run: 2 passed

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features "test-support,bench-support" \
  -E 'test(=oracle::tests::payload_permission_requires_the_resolved_table_scope) | test(=oracle::tests::payload_permission_is_absent_for_ungated_tables) | test(=oracle::tests::scoped_permission_digest_binds_the_authorized_table_set)'
# 3 tests run: 3 passed

mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support \
  -E 'test(=oracle::peer_authority::tests::oracle_peer_authority_rejects_tamper_replay_and_restart_fence) | test(=query::service::tests::production_query_paths_have_no_legacy_escape_hatches) | test(=query::service::tests::async_query_job_persistence_is_absent)'
# 3 tests run: 3 passed

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E "test(=query::service::tests::bifrost_query_authz_precedes_oracle_role_lookup)"'
# 1 test run: 1 passed

scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables)"'
# 1 test run: 1 passed

mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::invalidate_principal_removes_matching_entries_and_forces_re_resolve) | test(=tests::revocation_epoch_rejects_cache_hit_when_iat_predates_epoch)'
# 2 tests run: 2 passed
```

Selector confirmation: `mise exec -- cargo nextest list --locked -p wyrd-spec --lib -E 'test(/auth::permission_scope::tests/)'` and the matching `wyrd-runtime` listing were used to read the exact test paths above; every `test(=...)` expression selects exactly one test and each command's run count equals its expression count.

### Broader lanes and checks

```bash
mise run test:bifrost:journey:oracle    # 28 tests run: 28 passed
mise run test:bifrost:journey:server    # 5 tests run: 5 passed
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features "test-support,bench-support" -P journey --run-ignored=all -E "test(/^oracle::reader_authority::/)"'
# 9 tests run: 9 passed
mise run fmt
mise run lints
mise run codegen:check      # All checks passed, no generated drift
mise run check:client-tier
mise run check:pyo3-scope
mise run docs:check
git diff --check            # clean
```

### Pre-existing failures, unchanged by this remediation

- `mise run check:design-sync` — the same six `ValaQueryService` / `Bifrost*Payload` symbols missing from `architecture/wyrd-design.md`, identical to the reviewed candidate and the base. Outside this write set.
- `mise run check:unwrap-audit` — the same ten `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` findings. Outside this write set.

### Material limits and notes

- Authorizing at `attempt_protected_cut` also brings the typed-plan path (`prepare_typed_cuts` → `execute_typed_plan`) under the same object decision, which it previously bypassed. That is the single choke point every Oracle read path shares; the complete oracle journey lane (28/28), including distributed dispatch, peer stage graphs, and MCP, passes unchanged.
- `protect_and_materialize` and `protect_and_materialize_for_test` now take `&AuthorizedQueryContext` in place of a bare tenant. The context already carries the tenant, so no parameter was added, and the two reader-authority scenarios that are not about authorization use one shared `granted_context` helper.
- Proving the object denial's fail-closed audit substitution required forcing an append failure. One flag was added to the existing test-support `QueryControlAuditFaultController`, alongside the cancellation-audit fault it already owns; no new fault mechanism or production surface was introduced.
- Accepted journey cases still stream zero rows (the built-in tables are provisioned but not ingested into), so acceptance proves authorize/admit/audit/execute completion, not projection contents. Unchanged from the reviewed candidate.
