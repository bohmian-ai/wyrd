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
