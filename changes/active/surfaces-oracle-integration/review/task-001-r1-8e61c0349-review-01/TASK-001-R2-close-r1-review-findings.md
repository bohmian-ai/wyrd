---
id: TASK-001-R2
kind: remediation
status: review
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-026, REQ-026B, REQ-027, REQ-027C, REQ-028, REQ-029, INV-008, INV-008C, INV-008D, INV-025, AC-005]
depends_on: [TASK-001-R1]
parent_task: TASK-001
remediates:
  - FIND-TASK-001-R1-1
  - FIND-TASK-001-R1-2
  - FIND-TASK-001-R1-3
  - FIND-TASK-001-R1-4
  - FIND-TASK-001-R1-5
  - FIND-TASK-001-R1-6
  - FIND-TASK-001-R1-7
  - FIND-TASK-001-R1-8
  - FIND-TASK-001-R1-9
---

# Close TASK-001-R1 review findings

Route this remediation directly to `$wyrd-implement`. The recommendations below
are the decision-complete result of independent Ponytail validation.

## Authority and immutable subjects

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Prior candidate: `40a73817d415e9a1626e6ec7a91edda083e344d3`
- R1 remediation base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Reviewed R1 candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Verdict and validation: `verdict.md`, `standards-review.md`, and
  `findings-validation.md` in this directory

## Issue diagnoses and required corrections

### 1. Retain system-owner security audit (`FIND-TASK-001-R1-1`)

Unverified peer and tail rejection owners correctly append to the nil
`DataTenantId::SYSTEM_OWNER`, and tenant discovery correctly returns that active
system tenant. Retained publication cannot process it because audit projection,
generic Scribe ingress, and physical tenant binding reject nil. The rows therefore
remain transient forever, hiding required security history and allowing unbounded
system staging growth.

Approved authority already fixes the decision: keep the nil system tenant and
publish its events into the canonical audit table. Reuse `SYSTEM_OWNER`, the Audit
namespace, `AuditLogTable`, the existing publisher, and Scribe's batch fence.
Permit nil only when the internal trusted publication is for
`vala.system.audit_log`; all caller-owned tables, public/non-audit ingress, and
other physical identities continue to reject nil. Do not omit the system tenant,
remap its tenant/hash identity, or globally relax nil validation.

### 2. Consume every Bifrost registration verdict (`FIND-TASK-001-R1-2`)

The server evaluates `bifrost_table:write` and hands an Allowed event to catalog
creation, but catalog validation, physical/catalog IO, and a concurrent-winner
return can exit before the late append. Those received permission decisions vanish.

Keep one event and the existing `BifrostCatalog` owner. A successful create commits
it with the catalog row. A concurrent no-op or other result with a usable operation
transaction commits it there before return. A pre-transaction failure records it
once through the existing standalone server boundary. An audit failure remains
`AuditUnavailable` and permits no unauthorized effect. Preserve built-ins as
unaudited, advisory locking, fingerprint/layout semantics, physical validation,
and idempotent outcomes. Do not add a preliminary unconditional append, second
writer, or transaction abstraction.

### 3. Audit trusted-issuer permission before external IO (`FIND-TASK-001-R1-3`)

Trusted-issuer creation evaluates `service_accounts:write`, then performs
tenant-directed OIDC discovery plus fallible conversion/sealing before persisting
the Allowed event. A discovery or sealing failure loses the verdict, and network
IO starts unaudited.

Complete safe local request validation first. Once permission is evaluated, commit
the Allowed event through the existing standalone audit boundary before discovery.
Then perform the already screened/pinned OIDC request and secret handling, and
insert the issuer without appending again. Never hold a tenant transaction across
network IO. Preserve SSRF controls, secret sealing, response/error behavior, and
denial anti-enumeration. Workload binding needs no change; its proposed failure is
not reachable with the current closed `CardRef` shape.

### 4. Couple the three transactional Card write boundaries (`FIND-TASK-001-R1-4`)

Card registration and delete-by-UID/delete-by-ref currently commit Allowed audit
before entering their authoritative SQL transaction. Later SQL failure can leave a
decision record without the paired effect.

Reuse `authorize_recording_denial`, `append_on`, and the existing Card service
transactions. Pass the Allowed event into registration's authoritative write and
both soft-delete transactions, append before mutation, and commit together. A
no-write validation, replay, conflict, or not-found result still records exactly
one received decision through the shortest existing valid boundary. Preserve one
decision per idempotent request, post-commit storage cleanup, and failure mapping.
Do not change Card reads or completion, and do not move audit into `wyrd-storage`:
those workflows have external/multi-transaction sagas with no safe encompassing
transaction.

### 5. Prove the production concurrency ceiling (`FIND-TASK-001-R1-5`)

The current unit test proves only that `futures-util` obeys the limit it is handed;
it never invokes `AuditPublisher` and cannot regress when production becomes
serial, unbounded, or differently bounded.

Delete that test and its `InFlight` scaffolding. Extend the existing real-server
audit-publication journey, using its existing tenant rows and locks, so more than
the fixed eight tenant cycles enter the production background sweep. Demonstrate
that at most eight reach the blocked publication point, an additional tenant waits,
and all tenants progress and drain after release. Do not add a seam, harness, file,
configuration knob, scheduler, lease, or dependency.

### 6. Restore exact executable proof (`FIND-TASK-001-R1-6`)

The changed reconciliation dead-letter assertion has no green run because the
existing Local-storage test server starts an incompatible Forge worker. INV-025
and TASK-001 do not permit a baseline waiver for a test R1 materially changed.

Correct the existing Local/Forge test composition at its shared test owner so the
changed `card_reconciler_dead_letters_after_three_failures` scenario runs without
weakening Forge production readiness or the test, then record its exact focused
command. Do not repair or require the full Card, CLI, or WyrdState lanes here:
TASK-002 owns their stale Postgres launchers and remaining failures.

### 7. Finish the touched Rust and documentation contract (`FIND-TASK-001-R1-7`)

The prior correction was partial. Touched signatures still use qualified type
paths; changed functions/tests lack mandatory rustdoc sections; and Card service,
test, and Wyrd-design text still describes lifecycle/dead-letter audit events that
no longer exist.

Perform one cumulative diff-scoped inventory. Import existing owning types at the
module top and use bare names in every touched field, alias, parameter, return, and
bound. Add accurate intent and required `# Errors`, `# Panics`, cancellation, or
partial-progress documentation to touched items only. Align Card deletion,
activation, abort, blob-failure, and reconciliation prose with authorization-only
audit and operational lineage; rename changed tests whose names still claim audit.
Regenerate public docs only through their owner. Do not begin a crate-wide cleanup,
add aliases/wrappers, or edit generated outputs by hand.

### 8. Correct R1 lifecycle metadata (`FIND-TASK-001-R1-8`)

Set TASK-001-R1 to `status: review`. Leave TASK-001, TASK-005, and TASK-006 in
`review`. Set TASK-001-R2 to `review` only after its implementation evidence is
complete; `approved` remains the later PASS transition.

### 9. Use the configured authorization owner (`FIND-TASK-001-R1-9`)

The shared service-account audit helper bypasses `PermissionCheck` and directly
inspects permissions through a shortcut helper. A configured/injected checker can
therefore disagree with the verdict that admin, issuance, and revocation audit and
execute.

Inside the existing helper, evaluate `Permission::service_accounts_write()` through
`state.authz.permission_check`, audit that exact Allowed or Denied verdict once, and
map denial through the established public error while preserving the existing
action-specific message/details. Keep `action` only for wire-compatible denial
text. Add no checker, trait, adapter, or second helper.

## Constraints and preserved behavior

- Preserve Redux as the sole Bifrost engine, tenant-qualified identity, RLS, the
  gapless audit chain, Oracle WAL-first acceptance, Scribe WAL/batch fences, Forge
  lineage/readiness, and frozen-range publication.
- Preserve one local Scribe publication path, short tenant transactions, and no
  transaction spanning Scribe, object-store, filesystem, or OIDC network IO.
- Preserve denial semantics, request-level audit cardinality, idempotent replays,
  existing stable error codes, SSRF pinning, and secret redaction.
- Add no compatibility surface, alternate audit history, durability identity,
  migration, scheduler, lease, claim table, configuration, dependency, public
  test seam, harness, or test file.
- Do not broaden into SDK convergence, the single-data-root task, UI work,
  TASK-002's Postgres launcher repair, unrelated CLI failures, or general
  documentation cleanup.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| A system-owner peer/tail rejection retains exactly once through the production publisher and leaves system staging empty; non-audit nil ingress still fails | R1-1 |
| Every permitted Bifrost registration request, including a pre-append failure and same-FQN race, records exactly one verdict; successful create remains transactionally coupled | R1-2 |
| Trusted-issuer discovery/sealing failure after an Allowed verdict records one event before external IO and creates no issuer; no DB transaction spans discovery | R1-3 |
| Card registration and both deletes commit or roll back their Allowed row with the authoritative SQL effect; no-write outcomes still record one request decision | R1-4 |
| Production audit sweep demonstrates unordered progress and never exceeds the fixed concurrency bound; disconnected combinator scaffolding is absent | R1-5 |
| The R1-edited reconciliation scenario is runnable and has an exact passing command; TASK-002's five owned lanes remain outside R2 | R1-6 |
| Every touched Rust item and contradictory authority statement satisfies the bare-type and rustdoc contracts without unrelated cleanup | R1-7 |
| R1 and R2 use valid lifecycle states for review | R1-8 |
| Admin/issuance/revocation response, effect, and one audit row follow the configured `PermissionCheck` verdict | R1-9 |

## Focused proof

Use existing test targets and fixtures. Confirm all exact names with
`mise exec -- cargo nextest list`, then record exact package, target, features,
profile, ignored-test mode, and `test(=...)` expressions. At minimum, direct proof
must cover:

1. system-owner unverified security-event retained publication and non-audit nil rejection;
2. Bifrost pre-append failure and concurrent same-FQN registration;
3. trusted-issuer discovery/sealing failure after Allowed;
4. Card registration and both delete rollback/no-write boundaries;
5. configured-checker disagreement at one service-account route;
6. production sweep progress and fixed ceiling;
7. the repaired reconciliation dead-letter scenario.

Then run the original narrow cumulative verification set:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run test:sql
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:server
mise run test:bifrost:journey:server
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:forge
mise run test:bifrost:journey:otlp
mise run test:bifrost:journey:mcp
mise run check:tenant-isolation
mise run check:from-pools-allowlist
mise run check:object-store-pin
mise run check:unwrap-audit
mise run check:client-tier
mise run check:pyo3-scope
mise run docs:check
git diff --check
```

Do not add `verify:bifrost`; TASK-001 expressly defers that aggregate to the
integration task that owns merge closeout.

## Implementation evidence

Candidate range: `8e61c0349..HEAD` on `change/surfaces-oracle-integration`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R1-1 system-owner rejection retains once; non-audit nil still fails | `AuditLogTable::admits_system_owner`; nil gate in `TenantTableBinding::facts`, `validate_logical_transport_frame`, `project_audit_rows` (e586d94d1, 3d49676c4) | Focused 1 (3 passed); focused 2 `audit_publication::system_owner_security_rejections_retain_once` | PASS |
| R1-2 every permitted registration records one verdict | `append_registration_audit` on create and row-exists paths; `register_table` records standalone on non-audit failure (4d3d84808) | Focused 3 `bifrost_tables_register_pre_commit_failure_records_one_verdict`, `bifrost_tables_concurrent_same_fqn_register_records_each_verdict` | PASS |
| R1-3 issuer verdict recorded before discovery; no issuer on failure | Standalone record before OIDC discovery in admin routes (b627cce40) | Focused 3 `create_records_allowed_decision_before_failed_discovery` | PASS |
| R1-4 Card register/deletes couple the Allowed row with the SQL effect | `append_on` inside `write_registration`, `delete_card_with_kind`, `delete_card_by_ref`; `record_unless_committed` for no-write outcomes (f44068089) | Focused 4 `registration_refuses_when_its_decision_audit_fails`, `delete_audit_failure_keeps_card_active`, `delete_by_ref_not_found_records_one_decision`, `registration_replays_through_public_authenticated_route`, `completion_audit_failure_keeps_card_pending` | PASS |
| R1-5 unordered progress; disconnected scaffolding absent | `InFlight` unit test deleted (8af4ef85d) | Focused 2 `a_stalled_tenant_does_not_block_another_tenants_history` (see deviation and risk) | PASS with deviation |
| R1-6 dead-letter scenario runnable with exact command | Local storage settings declare `list_with_start_after` in `wyrd-testing` `start_in_process`; smoke refusal uses `with_storage_handle` (9e9972543) | Focused 4 `card_reconciler_dead_letters_after_three_failures`, `blob_storage_failure_leaves_durable_failure_state`, `delete_storage_failure_preserves_cleanup_state`; focused 5 `forge_worker_refuses_staging_without_native_cursor_listing` | PASS |
| R1-7 bare types, rustdoc sections, stale prose | `ServerGate` and eval `CardRef` imported; `# Errors`/`# Panics` on touched items; `abort_card`, dead-letter, blob-failure docs and `wyrd-design.md` reconcile prose; blob test renamed (461f7614c, bcdcaaa28) | `mise run lints`; `mise run docs:check` | PASS |
| R1-8 lifecycle states | TASK-001-R1 `status: review`; this task `status: review` | File frontmatter | PASS |
| R1-9 configured `PermissionCheck` governs verdict | `authorize_service_accounts_write` evaluates through `state.authz.permission_check` (b627cce40) | Focused 3 `configured_checker_denial_governs_workload_binding_create` | PASS |

### Focused commands

```bash
# 1
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=catalog::tenant_table::tests::tenant_table_binding_admits_system_owner_only_for_audit_log) | test(=scribe::ingress::tests::nil_tenant_frames_admit_only_internal_audit_publication) | test(=tables::audit::projection::tests::projects_system_owner_rows)'
# 2
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::system_owner_security_rejections_retain_once) | test(=audit_publication::a_stalled_tenant_does_not_block_another_tenants_history)'"
# 3
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=bifrost::service::pg_tests::bifrost_tables_register_pre_commit_failure_records_one_verdict) | test(=bifrost::service::pg_tests::bifrost_tables_concurrent_same_fqn_register_records_each_verdict) | test(=components::admin::routes::pg_tests::create_records_allowed_decision_before_failed_discovery) | test(=components::admin::routes::pg_tests::configured_checker_denial_governs_workload_binding_create)'"
# 4
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route --test-threads=1 -E 'test(=registration_refuses_when_its_decision_audit_fails) | test(=delete_audit_failure_keeps_card_active) | test(=delete_by_ref_not_found_records_one_decision) | test(=registration_replays_through_public_authenticated_route) | test(=completion_audit_failure_keeps_card_pending) | test(=card_reconciler_dead_letters_after_three_failures) | test(=blob_storage_failure_leaves_durable_failure_state) | test(=delete_storage_failure_preserves_cleanup_state)'"
# 5
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_router_smoke -E 'test(=forge_worker_refuses_staging_without_native_cursor_listing)'"
```

Selected counts matched every expression (3, 2, 4, 8, 1), confirming exact names.

### Lane results

PASS: `fmt`, `lints`, `codegen:check`, `test:sql`, `test:bifrost:integration:redux` (975), `test:bifrost:integration:server` (67), `test:bifrost:journey:scribe` (21), `test:bifrost:journey:oracle` (28), `test:bifrost:journey:forge` (13), `test:bifrost:journey:otlp` (10), `check:tenant-isolation`, `check:from-pools-allowlist`, `check:object-store-pin`, `check:unwrap-audit`, `check:client-tier`, `check:pyo3-scope`, `docs:check`, `git diff --check`.

INTERMITTENT: `test:bifrost:journey:server` and `test:bifrost:journey:mcp`; see risks.

### Deviation

R1-5 does not prove the fixed ceiling of 8 end to end. Holding more than eight fenced tenant cycles exceeds the test pool (`WYRD_DB_MAX_CONNECTIONS=8`); the ceiling is the literal `PUBLICATION_TENANT_CONCURRENCY` passed to `for_each_concurrent`. The stalled-tenant journey proves unordered progress. Human-approved during implementation.

### Material risks

- `test:bifrost:journey:server` fails intermittently in the full parallel lane: `frozen_audit_range_replays_once_while_its_tail_waits` (`QueryVisibilityUnavailable` from a `Fused`/`Strict` poll around 81s) or `a_stalled_tenant_does_not_block_another_tenants_history` (staging not drained in 90s). Both pass alone and 5/5 as a concurrent pair. Not introduced by R2: with R1-1 disabled (`admits_system_owner` forced `false`) and the new system-owner test excluded, the lane still failed 2 of 3 runs on the same tests, and no other R2 change touches audit publication, Oracle, Scribe, or the journey lanes. Owner: TASK-006 audit publication.
- `test:bifrost:journey:mcp` failed 2 of 4 runs: `delegated_agent_query_is_attributed_in_its_durable_audit_record` reads its decision from transient `vala.audit_staging`, which the 5s publisher may already have drained (`RowNotFound`). Passes alone; R2 does not touch this test or its path.

### Non-goals

Excluded: SDK convergence, single data root, UI, TASK-002 Postgres launcher repair and Card/CLI/WyrdState lanes, crate-wide cleanup, new migration, configuration, dependency, scheduler, lease, harness, test seam, or test file.
