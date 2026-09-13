---
id: TASK-001-R1
kind: remediation
status: review
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-026, REQ-027C, REQ-028, REQ-029, INV-008, INV-008C, INV-008D, AC-005]
depends_on: [TASK-005, TASK-006]
parent_task: TASK-001
remediates:
  - FIND-TASK-001-1
  - FIND-TASK-001-2
  - FIND-TASK-001-4
  - FIND-TASK-001-5
  - FIND-TASK-001-6
  - FIND-TASK-001-7
  - FIND-TASK-001-8
  - FIND-TASK-001-9
  - FIND-TASK-001-10
  - FIND-TASK-001-11
  - FIND-TASK-005-1
  - FIND-TASK-005-2
  - FIND-TASK-005-5
  - FIND-TASK-005-6
  - FIND-TASK-005-7
  - FIND-TASK-005-8
  - FIND-TASK-005-9
  - FIND-TASK-005-10
  - FIND-TASK-006-1
  - FIND-TASK-006-2
  - FIND-TASK-006-3
  - FIND-TASK-006-4
  - FIND-TASK-006-5
  - FIND-TASK-006-6
  - FIND-TASK-006-7
  - FIND-TASK-006-8
  - FIND-TASK-006-9
---

# Close the combined TASK-001, TASK-005, and TASK-006 review findings

Route this one remediation task to `$wyrd-implement`. It replaces separate remediation tasks for TASK-001, TASK-005, and TASK-006.

## Authority and immutable subjects

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- TASK-001: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5..40a73817d415e9a1626e6ec7a91edda083e344d3`
- TASK-005: `0af5eef72f83a95167f6ee3bdc873df9f9cc4254..1f6d3b6316bfd87c8a52e36b623b80d608304e19`
- TASK-006: `32a0aafecbda96a86103cd389e42728ea33e0c54..358cf636eda48ea31a3416e2c5b21b30873f83dd`
- Cumulative implementation candidate: `40a73817d415e9a1626e6ec7a91edda083e344d3`
- Independent validation: `findings-validation.md` in this directory

## Validation disposition

All original findings are accounted for. The validator consolidated duplicates at their shared correction boundary.

| Disposition | Finding IDs | Result |
|---|---|---|
| Remediate | `FIND-TASK-001-1`, `FIND-TASK-005-1` | Audit both outcomes at each receiving permission owner. |
| Remediate | `FIND-TASK-001-2`, `FIND-TASK-005-2` | Remove canonical audit from paths that evaluate no permission. |
| Rejected | `FIND-TASK-001-3`, `FIND-TASK-005-4` | Oracle WAL relay is intentionally at least once; valid duplicates are not a current defect. Add no durability mechanism. |
| Remediate | `FIND-TASK-001-4`, `FIND-TASK-001-10`, `FIND-TASK-006-9` | Correct focused, MCP, docs, and cumulative evidence. |
| Remediate | `FIND-TASK-001-5`, `FIND-TASK-005-5`, `FIND-TASK-006-5` | Restore approved SQL capability owners. |
| Remediate | `FIND-TASK-001-6`, `FIND-TASK-005-6`, `FIND-TASK-006-4` | Remove redundant tenant predicates from `TenantConn` queries. |
| Remediate | `FIND-TASK-001-7`, `FIND-TASK-006-7` | Repair the validator-enumerated rustdoc defects. |
| Remediate | `FIND-TASK-001-8`, `FIND-TASK-005-8`, `FIND-TASK-006-6` | Import signature types and use bare names. |
| Remediate | `FIND-TASK-001-9`, `FIND-TASK-005-9`, `FIND-TASK-006-8` | Align the enumerated architecture, reference, operations, and public docs. |
| Remediate | `FIND-TASK-001-11` | Put all three completed implementations into `review`. |
| Historical closure | `FIND-TASK-005-3` | TASK-006 already supplied the frozen upper bound and deterministic replay identity. Do not reimplement it. |
| Remediate | `FIND-TASK-005-7` | Replace the one-production-implementation dynamic Gate audit trait with static composition. |
| Remediate | `FIND-TASK-005-10` | Delete the entire obsolete tenant-isolation exemption mechanism. |
| Remediate | `FIND-TASK-006-1`, `FIND-TASK-006-3` | Keep only the complete publication cycle callable and prove replay through the existing journey. |
| Remediate | `FIND-TASK-006-2` | Bound concurrent tenant publication so one tenant cannot block another. |

## Required corrections

### 1. Audit each receiving permission verdict exactly once

Correct the existing authorization owners rather than adding a global audit service:

- Bifrost catalog list, describe, and registration;
- Card routes;
- storage routes;
- Eval run creation;
- service-account administration and revocation; and
- existing route-local authorization helpers reached by those operations.

Each evaluated verdict writes one tenant-owned `Allowed` or `Denied` event before the operation proceeds or refuses. Use the operation transaction where one already exists and fail closed on audit failure. Bifrost create keeps its existing same-transaction append in `register_dataset`; do not add a duplicate preliminary append. Preserve Gate, Oracle query lifecycle, `/v1/authz/check`, denial anti-enumeration, and Oracle's WAL-first exception.

### 2. Delete canonical audit from non-permission transitions

Remove the canonical append in `wyrd-server/src/auth/login.rs`. Remove card registration, reconciliation, and blob-lifecycle appends in `wyrd-server/src/components/cards/service.rs` when those paths evaluate no permission. Trace and remove every non-permission caller of `wyrd-storage/src/audit.rs::write`, including the service and sweeper callers; delete the storage audit helper if no production caller remains.

Keep actual permission-decision audit, including principal revocation, and add its missing denied outcome. Preserve existing operational tables, lineage, and structured tracing.

### 3. Restore the existing SQL ownership boundary

- Store the existing `ValaPostgres` owner in `AuditPublisher`; do not store `PgPool`.
- Keep cross-tenant discovery on `OperatorPool`; change `list_active_tenant_ids` to accept `&OperatorPool`.
- Delete `append_audit_connection(&mut PgConnection)` and keep the append workflow on `append_audit(&mut TenantConn)`.
- In `audit_staging` reads, locks, updates, and deletes that already accept `TenantConn`, remove every manual `data_tenant_id = wyrd.current_tenant()` predicate. Retain `wyrd.current_tenant()` only where an inserted tenant column needs its value.

Do not add a connection abstraction.

### 4. Use static Gate audit composition

Remove `Arc<dyn GateAudit>`. Parameterize the existing Gate owner over its audit implementation so production selects `PostgresGateAudit` statically and unit tests select `RecordingAudit`. Keep Postgres ownership in the server. Do not add an adapter, service, factory, or second trait.

### 5. Keep partial publication stages private

Only `AuditPublisher::publish_tenant` may expose the complete freeze → publish → settle cycle. Make `publish_range` and `settle` private; callers must not be able to delete staging without successful Scribe acceptance.

Revise the existing server publication journey without adding a public fault seam or new harness:

1. Freeze and commit the range with existing SQL.
2. Hold the tenant chain-head row so settlement blocks.
3. Spawn `publish_tenant` and observe retained Scribe acceptance.
4. Abort that blocked cycle before settlement.
5. Release the lock and call `publish_tenant` again.
6. Prove the same frozen range is absorbed by Scribe's batch fence, retained rows are unique, tail rows wait for the next range, and staging drains.

Keep the existing multi-pod Scribe scenario as the canonical-ingest control.

### 6. Bound cross-tenant publication

Replace the serial tenant sweep with bounded unordered concurrency inside `AuditPublisher`, using the already-installed `futures-util`. Use one fixed internal bound; do not add configuration, a scheduler, lease, owner token, claim table, service, or global lock.

Extend the existing server publication journey: seed two tenants, block the earlier tenant at its chain head, prove the later tenant reaches retained history within the test budget, and prove concurrent work never exceeds the fixed bound. Do not add a test file or harness.

### 7. Repair only the enumerated Rust surface defects

- Restore module rustdoc in `wyrd-server/src/audit/publication.rs`.
- Add accurate `# Errors` sections to the fallible `freeze` and `read_range` methods.
- Document cancellation and partial progress for the publisher's async workflow.
- Add the required rustdoc to the materially changed Gate test seam and tests.
- Correct `execution_lanes.rs::append_managed_columns` documentation: correlation columns are conditional, not unconditional.
- Import and use bare signature types at `gate/mod.rs:1287,1299,1313`, `audit/publication.rs:240`, and `source_boundary_recovery.rs:220` in the reviewed candidate.

Do not turn this into a crate-wide documentation sweep.

### 8. Align the enumerated documentation

Correct the authorization-only audit and frozen-range publication descriptions in:

- `architecture/bifrost-design.md:685-688`;
- `architecture/references/domain/vala-architecture.md:77-79`;
- `architecture/references/languages/rust-core.md:601-605`;
- `architecture/operations/reliability-and-recovery.md:175-181`; and
- the changed Bifrost `architecture.svx`, `data-plane-internals.svx`, and `forge.svx` pages.

Audit records permission decisions. Scribe, Forge, Oracle protection/recovery, publication, reconciliation, and storage mechanics retain their operational lineage or diagnostics instead. Publication recovery uses the watermark plus the persisted frozen upper bound.

### 9. Delete the obsolete tenant-isolation exemption completely

In `scripts/check_tenant_isolation.py`, delete the removed `audit_outbox`/`OperatorAudit` exemption constant, its special-case branch, the now-unused function flags, and stale explanatory prose. Preserve all live tenant-boundary checks.

### 10. Correct lifecycle and evidence

Set TASK-001, TASK-005, and TASK-006 to `review`. Record exact focused commands for every named changed scenario, including the changed MCP query journey. Run the narrow cumulative lanes below. Do not require the prohibited `verify:bifrost` aggregate; TASK-001 expressly deferred it until merge work is complete.

## Acceptance criteria

1. Every affected permission evaluation produces exactly one canonical tenant audit event for both outcomes before proceed/refusal, and audit failure prevents the operation without changing intended denial semantics.
2. Authentication, reconciliation, blob/storage backend lifecycle, Scribe, Forge, Oracle protection/recovery, and retained publication transitions that evaluate no permission append no canonical authorization event; their operational evidence remains.
3. `AuditPublisher` owns `ValaPostgres`, cross-tenant discovery accepts `&OperatorPool`, canonical tenant writes accept `TenantConn`, and `TenantConn` SQL has no redundant tenant equality.
4. Gate audit uses static composition with the production and test implementations; no one-production-implementation dynamic trait remains.
5. Only the complete publication cycle is callable. The existing real-server journey proves committed freeze, tail growth, abort-before-settlement replay, Scribe deduplication, retained uniqueness, and final drain without a new public seam or harness.
6. Holding one tenant's publication does not prevent a second tenant from reaching retained history, while total concurrent tenant work stays within one fixed internal bound.
7. The enumerated rustdoc, signature, architecture, reference, operations, public-doc, and tenant-check defects are corrected without unrelated cleanup.
8. TASK-001, TASK-005, and TASK-006 are in `review`, and the exact focused, MCP, docs, SQL, integration, journey, codegen, and boundary evidence is recorded against the cumulative result.
9. No Oracle WAL identity, receipt table, relay watermark, or other new audit durability mechanism is added. Existing at-least-once Oracle relay behavior remains unchanged.

## Verification

For every named Rust test added or changed, first confirm its exact name with `mise exec -- cargo nextest list`, then run it with an exact package, target, and `test(=...)` expression. Include exact selectors for the authorization outcomes and failures, frozen-range replay, two-tenant progress, and the existing multi-pod Scribe scenario.

Run:

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

## Non-goals and preserved behavior

- Preserve Redux as the sole Bifrost engine, tenant-qualified identity, RLS, the gapless audit chain, Oracle WAL-first acceptance, Scribe WAL and batch fences, Forge lineage/readiness, and frozen-range publication.
- Preserve short tenant transactions; no transaction or chain lock spans Scribe IO.
- Preserve one local Scribe publication path on Scribe-bearing roles.
- Do not add a compatibility surface, second audit history, direct Iceberg audit writer, scheduler, lease, claim table, configuration knob, dependency, test harness, or test file.
- Do not broaden into SDK convergence, the single data-root task, UI work, or unrelated cleanup.

## Implementation evidence

Candidate range: `8377fff9f..HEAD` on `change/surfaces-oracle-integration`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Each receiving verdict audited once, both outcomes, fail closed | `wyrd-server/src/audit/mod.rs` `authorize`, `authorize_recording_denial`, `authorize_service_accounts_write`; Card, storage, Eval, admin, revoke, `issue_key` routes; Bifrost list/describe/register (create keeps the `register_dataset` append) | `pg_card_registration_route`: `registration_replays_through_public_authenticated_route`, `registration_refuses_when_its_decision_audit_fails`, `delete_audit_failure_keeps_card_active`; `storage_e2e::storage_routes_refuse_and_audit_an_unprivileged_caller`; `pg_eval_v1_protocol::open_requires_eval_run_permission`; admin `pg_tests` (13); `bifrost::service::pg_tests`; CLI `apply_refuses_when_completion_decision_audit_fails`; MCP `connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation`, `query::pg_tests::delegated_agent_query_is_attributed_in_its_durable_audit_record` | PASS |
| 2. No canonical audit from non-permission transitions | `auth/login.rs`; `components/cards/service.rs` (reconcile, activation, cleanup, blob failure now tracing/operational state); `wyrd-storage/src/audit.rs` deleted with service and sweeper callers | `mise run test:sql` (includes `wyrd-storage` `pg_sweeper`); `card_reconciler_dead_letters_after_three_failures` reads `reconcile_attempts` (blocked, see risks) | PASS (one blocked scenario) |
| 3. SQL ownership restored | `AuditPublisher` holds `ValaPostgres`; `list_active_tenant_ids(&OperatorPool)`; `append_audit_connection` deleted; tenant predicates removed | `mise run test:sql`; `mise run check:from-pools-allowlist`; `mise run check:tenant-isolation` | PASS |
| 4. Static Gate audit composition | `Gate<P, I, A>`; `ServerGate` selects `PostgresGateAudit` | `vala-bifrost-redux` `gate::tests::gate_enforces_bifrost_record_write`, `gate::tests::batch_identity_acceptance_is_unchanged`, `gate::tests::gate_constructs_with_injected_seams_without_server_boot`; `mise run test:bifrost:integration:redux` (973 passed) | PASS |
| 5. Only the complete publication cycle callable; replay journey | `publish_range`/`settle` private; journey fences staged rows at settlement with a savepoint-scoped delete (the application role holds no UPDATE on staging) | `wyrd-testing::server audit_publication::frozen_audit_range_replays_once_while_its_tail_waits`, `audit_publication::audited_transitions_retire_only_into_retained_history`; Scribe control `horizontal_ingest::multi_pod_concurrent_batches_are_owned_and_visible` | PASS |
| 6. Bounded cross-tenant publication | `PUBLICATION_TENANT_CONCURRENCY = 8` with `buffer_unordered` | `audit::publication::tests::bounded_sweep_never_exceeds_its_fixed_concurrency`; `audit_publication::a_stalled_tenant_does_not_block_another_tenants_history` | PASS |
| 7. Enumerated rustdoc, signature, docs, tenant-check defects | publication module docs, `# Errors` on `freeze`/`read_range`, cancellation docs, bare types, `append_managed_columns` doc; architecture, reference, operations and `.svx` pages; `check_tenant_isolation.py` exemption deleted | `mise run lints`; `mise run docs:check`; `mise run check:tenant-isolation` | PASS |
| 8. Lifecycle and evidence | TASK-001, TASK-005, TASK-006 `status: review`; this matrix | Commands below | PASS |
| 9. No new audit durability mechanism | No Oracle WAL identity, receipt table, relay watermark, config, dependency, harness, or test file added | Diff audit of `8377fff9f..HEAD` | PASS |

### Commands

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

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=gate::tests::gate_enforces_bifrost_record_write) | test(=gate::tests::batch_identity_acceptance_is_unchanged) | test(=gate::tests::gate_constructs_with_injected_seams_without_server_boot)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=audit::publication::tests::bounded_sweep_never_exceeds_its_fixed_concurrency)'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(/^components::admin::routes::pg_tests::/)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits) | test(=audit_publication::a_stalled_tenant_does_not_block_another_tenants_history) | test(=audit_publication::audited_transitions_retire_only_into_retained_history)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=horizontal_ingest::multi_pod_concurrent_batches_are_owned_and_visible)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=query::pg_tests::delegated_agent_query_is_attributed_in_its_durable_audit_record) | test(=connectivity::pg_tests::mcp_rejects_credentials_and_joins_request_and_process_cancellation)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_eval_v1_protocol -E "test(=open_requires_eval_run_permission)"'
WYRD_STORAGE_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features storage-emulator --test storage_e2e -E "test(=storage_routes_refuse_and_audit_an_unprivileged_caller)"'
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_card_registration_route -E "test(=registration_replays_through_public_authenticated_route) | test(=delete_audit_failure_keeps_card_active) | test(=registration_refuses_when_its_decision_audit_fails)"'
WYRD_CLI_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E "test(=card_lifecycle::pg_tests::apply_refuses_when_completion_decision_audit_fails)"'
```

### Pre-existing blockers and risks

- `mise run test:cards:integration` and `mise run test:cli:journey` cannot start: `setup:postgres` depends on `setup:db-roles`, deleted in `c9bf7322c`. Their suites ran through `scripts/postgres/with-test-postgres.sh` instead.
- Six `pg_card_registration_route` tests that call `start_bound()` with a `Local` storage backend fail identically at the pre-change base `8377fff9f^`: the harness composes a Forge worker, which exits with "Forge worker staging backend does not support native list_with_start_after" (gate added in `66bec4bf1`), so `/healthz` never serves. This blocks `card_reconciler_dead_letters_after_three_failures`, whose assertion this remediation changed.
- `wyrd-cli --test cli` `card_lifecycle::pg_tests::card_lifecycle_cli_journey` (artifact digest mismatch returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, not `…507_ARTIFACT_VERIFY_FAILED`) and `multi_card_service_get_hydrates_complete_and_metadata_bundles` (Model fixture lacks `signature.inputs`) fail identically at `8377fff9f^`; unrelated to audit. The former `apply_surfaces_backend_completion_failure` also failed at base; its renamed replacement passes.
- Denial audit is per request, so an idempotent replay records a second decision.
- `wyrd_spec` `AuditDetail::Storage` variants are no longer produced; the wire type is unchanged.
