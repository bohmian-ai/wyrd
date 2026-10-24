---
id: TASK-005
kind: implementation
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
status: proposed
depends_on: [TASK-004, TASK-004-R2]
requirements: [REQ-025, REQ-027, REQ-031, REQ-032, REQ-035, REQ-036, REQ-038, REQ-042, REQ-044, REQ-047, REQ-048]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-010, AC-012, AC-013, AC-016, AC-018, AC-021]
---

# Configure and safely evolve a tenant's Bifrost tables

## Outcome and Value

A tenant administrator can inspect a table's actual configuration and explicitly change its partition, sort, or Bloom policy without replacing its schema or overwriting another administrator's work. Dynamic tables also support nullable custom-column additions and renames that retain stored values. The same table owner serves system and dynamic tables.

For example, an administrator describes a span table, enables `span_id` Blooms using its current table version, and receives the committed layout and next table version. Restarting a server preserves that choice. A second administrator's edit using the old table version fails without changing anything. An ordinary writer cannot perform either edit merely because it can emit or query spans.

## What Changes and What Must Stay True

Implement the exact creation, partial-edit, supported-type and schema-edit contracts in the approved revision-13 [table supplement](../supplements/table-storage-and-correlation.md#one-layout-contract-and-tenant-configuration). Its [explicit alteration contract](../supplements/table-storage-and-correlation.md#explicit-table-alteration-contract) fixes the PATCH body, positive table version starting at 1, strict decoding, atomicity, resolved response and 400/403/409 outcomes. Creation stays POST; re-registration never alters a table.

**Single version.** The single table version is defined by [TASK-004-R2](../review/TASK-004-c4dd61029/TASK-004-R2-single-table-version.md): one positive integer per table, 1 at creation, incremented once per effective definition edit (including Bloom-only edits), never by data commits. R2 owns its allocation, the atomic native-metadata/retained-reference publication with expected-version CAS, retention, carriage through every tier, describe/register exposure and writers sending it. This task adds no allocator, counter, properties-only CAS or second field: PATCH supplies the expected table version, the shared catalog commit checks and bumps it, and the response returns the committed version. Revision 13 fixes `table_version` in responses and `expected_table_version` in PATCH, with no aliases: positive signed-64-bit values, canonical decimal strings in JSON and `bigint` in TypeScript. This task adds no numeric-JSON shortcut or lossy conversion. Revision 13 is approved; this task remains proposed until readiness review.

Use existing tenant-wide `bifrost_table:write` DDL authority, tenant-bound lookup and nonblocking audit, without role-name bypass. Query/record grants confer no administration authority. Context never selects a tenant. Protect canonical system schema, sensitivity, managed fields and immutable time axis.

Creation uses approved defaults; PATCH omissions preserve current lists and supplied lists replace them. Empty Blooms are valid with no mandatory floor. Only nullable top-level custom additions and stable-ID top-level renames are schema edits; validate the whole post-edit schema/layout against the supplement's exhaustive matrix. No deletions, type/nullability changes, nested edits or implicit rewrites.

TASK-004 and R2 own the table version substrate, accepted-batch binding, publication reconciliation, WAL, mixed-spec reads and compaction. This task exposes explicit mutations through that substrate. Failed edits change no metadata/cache and consume no version; committed edits refresh one coherent current descriptor (schema, layout and version together) across replicas after proven commit. Startup respects stored overrides while validating canonical schemas. Accepted batches retain original versions through retry and recovery.

Mutations ship through HTTP and existing Rust/Python/TypeScript Bifrost facades, including existing sync/async projections. MCP describe exposes the same table version/layout through ordinary read authority. Describe already returns the version and writers already send it under R2; this task adds only the mutation and its projections. Preserve CLI query and declaration consumers; add no CLI administration command, MCP write-tool spelling or gRPC DDL service.

TASK-003 may execute later: use the generic table owner rather than copies of future canonical names/schemas. It owns the catalog rename and nine-table schema/default replacement; this task supports their eventual definitions. Amend owning Bifrost architecture sections; TASK-008 owns final docsite instructions.

Extra dimensions can fragment files and Blooms cost hashing/memory/storage. Retaining in-baseline versions is required. No historical migration, backfill, alias, dual contract, destructive reset, new writer tuning or performance SLO.

## Approach

1. Extend shared contracts and validation using R2's table version.
2. Add validated expected-version mutation through R2's catalog publication commit, typed HTTP and audited DDL authorization.
3. Honor stored overrides during startup and committed versions in serving caches.
4. Close existing SDK/MCP/declaration consumers, exports, stubs and schemas.
5. Prove the workflows below and amend owning architecture prose.

## Ordered Implementation Scenarios

### Scenario 1 — Creation and describe resolve one bounded table contract

**Behavior.** A tenant creates a dynamic table with omitted layout and observes hourly partitioning of `wyrd_ingested_at`, descending time sort and empty Blooms at table version 1. A supplied required UTC time column drives the default instead. Explicit supported layouts resolve exactly, including nullable extra partition sources. Equivalent re-registration returns the same table without a version change; a conflicting declaration refuses rather than alters it. The same validator accepts eligible system columns without permitting schema replacement.

| Declaration case | Expected result |
| --- | --- |
| Supported normalized primitive transforms and boundary parameters | Resolve to the requested stable-field layout |
| Naive/nullable/wrong-unit time axis; unsupported type/transform pair | Refuse before creating metadata or a physical table |
| Unknown/reserved name, redundant partition, duplicate sort, invalid parameter, or limit overflow | Refuse the complete declaration |
| Omitted versus empty Bloom list at creation | Resolve declared default versus explicitly empty set |

**RED.** Add the planned [creation journey](#creation-proof). Observe missing default/partition behavior or an invalid declaration accepted; assert description and absence of partially created tables on refusal. Add focused pure validation cases under the existing layout owner where the matrix cannot be usefully diagnosed through the journey, recording exact selectors first.

**GREEN.** Resolve creation and description through one validator and catalog; preserve existing registration identity and compaction properties. Rerun this journey for each matrix correction.

**REFACTOR.** Remove duplicated declaration defaults and mandatory Bloom-floor injection in touched consumers while the creation journey remains green.

### Scenario 2 — An explicit layout edit preserves omissions and rejects races

**Behavior.** An authorized administrator describes a system or dynamic table and PATCHes with its table version. Omitted lists remain unchanged; supplied lists replace the complete lists, including empty Blooms. A valid partition/sort/Bloom change commits through R2's versioned catalog publication. A multi-property edit commits once and returns the exact resolved description at the next version. Two administrators editing the same version yield one success and one 409, including two Bloom-only edits, with no lost update. A failed or ineffective edit changes neither configuration nor cached state.

**RED.** Add the planned [versioned layout journey](#layout-proof), covering system and dynamic cases with one assertion shape, explicit empty versus omitted lists, combined edits, an invalid post-edit layout, no-op rejection, and competing edits. Assert unchanged describe results after every refusal.

**GREEN.** Add the authorized explicit mutation at the shared catalog boundary, using full-result validation and R2's expected-version publication commit; do not add a separate comparison. Rerun creation and layout journeys.

**REFACTOR.** Consolidate creation/edit validation without conflating their different omitted-field meanings; keep both journeys green.

### Scenario 3 — Compatible custom schema edits retain stored identity and data

**Behavior.** After writing a dynamic table, the administrator adds a nullable custom column and renames an existing custom column referenced by the layout. Describe reports the new names and preserved IDs; old data retains renamed values and reads the added field as null; new writes use the committed schema. The whole request refuses if any schema or layout dependency is invalid. Protected system/managed fields and all edits outside the approved safe-edit matrix refuse without consuming a version or changing data.

**RED.** Add the planned [schema evolution journey](#schema-proof), writing and reading through the public client before and after edits, plus catalog-level field-ID assertions at the existing engine test tier where not wire-visible. Cover collision/chained rename and each unsupported edit category with a compact case table. Confirm a mixed valid/invalid request commits nothing.

**GREEN.** Extend the existing schema mapping and catalog evolution path with only the two approved operations, preserving stable IDs and full post-edit layout validation. Rerun creation, layout and schema journeys.

**REFACTOR.** Remove any separate rename/layout-name bookkeeping that can be derived from the preserved field identity, retaining all three proofs.

### Scenario 4 — Committed overrides survive startup and refresh serving caches

**Behavior.** Warm descriptions/admission on multiple serving instances, then commit an override for one tenant/table. New admissions observe the committed version; a writer holding the old version gets R2's stale-version refusal for new batches; an already accepted batch retains its original version through retry, restart and publication. Startup validates the canonical system schema without reverting its authorized layout. Another tenant's same-named table keeps its configuration. Queries read old and new files correctly, including missing historical Blooms and mixed partition specifications.

**RED.** Add the planned [persistence journey](#persistence-proof), reusing the existing retained-root restart and multi-node harness. Assert public describe and read outcomes plus retained batch identity, rather than treating an in-process cache replacement as sufficient restart proof. Exercise failed mutation as well as successful cache refresh. TASK-004's physical evolution proofs remain the lower-tier oracle; do not duplicate the storage engine suite.

**GREEN.** Make built-in ensure distinguish schema invariants from persisted tenant layout, and make existing registration/schema/layout caches follow committed versions across instances through the shared owner, never mixing one version's schema with another's layout. Rerun earlier journeys and this persistence proof.

**REFACTOR.** Retire immutable-current-layout assumptions in existing caches; reuse the established catalog/read publication mechanisms instead of adding an independent configuration service or cache. Keep all prior proofs green.

### Scenario 5 — The HTTP boundary enforces DDL authority and exact refusal shapes

**Behavior.** The exact PATCH contract accepts only a canonical decimal-string `expected_table_version` for the current positive signed-64-bit value and a supported effective edit. Unknown fields, explicit nulls (including nested layout lists), malformed bodies, immutable time-axis edits and unsupported operations receive 400; stale versions receive 409; callers with only record or query grants receive 403. Each failure returns one typed catalog error and leaves metadata/data unchanged. Lookup is tenant-bound and does not reveal another tenant's registration. Every permission decision uses the existing nonblocking audit path; an audit sink failure never turns an allowed edit into a denied edit or an allowed operation into data loss.

**RED.** Add the planned [HTTP contract journey](#http-proof) against the real server, asserting statuses and exact stable codes for each negative class, response schema, committed version, and unchanged describe state. Include OpenAPI request/response coverage in the existing runtime contract owner.

**GREEN.** Wire the typed adapter, strict request decoding, permission and audit decision to the shared service/catalog operation. Rerun the five journeys.

**REFACTOR.** Use existing typed error projection, authorization and audit mechanics; eliminate handler-only validation that duplicates the shared owner while preserving transport-shape checks.

### Scenario 6 — Existing clients and discovery expose the same configuration

**Behavior.** Rust, Python and TypeScript clients create/describe, alter using the observed table version, inspect the returned version, and read renamed/added values through their normal Bifrost facade. Their stale edit and missing-grant errors preserve the server catalog code. Versions above 2^53 through 9223372036854775807 round-trip exactly (TypeScript bigint, JSON decimal strings); numeric JSON, zero, overflow and noncanonical strings refuse. Version exhaustion refuses an effective edit without mutation. Sync/async facades project the same contract, with no client-owned durable validation. An MCP reader describes the same resolved configuration after an authorized SDK edit; read tools remain discoverable without administration grants, and describe still enforces its ordinary read permission. Existing CLI queries and Card-declared table registration continue through the same table owner.

**RED.** Add focused [Python](#python-proof) and [TypeScript](#typescript-proof) journeys and extend the existing [MCP discovery journey](#mcp-proof). Rust client coverage is provided by scenarios 1–4. Before implementing projections, observe missing mutation support or stale generated declaration failures. Add equivalent focused consumer regression proof if a touched CLI or Card-declaration behavior changes, recording its exact selector before RED.

**GREEN.** Complete Rust facade and blocking projection, Python native/package/export/stub closure, TypeScript native/wrapper/generated type closure and MCP describe serialization. Reuse the shared transport and error catalog. Rerun all focused journeys.

**REFACTOR.** Remove obsolete granularity spellings and duplicate layout shapes from touched consumers; keep generated contracts derived and Python stubs authored. No compatibility aliases remain.

## Acceptance Criteria

| Obligations owned here | Observable proof and violation |
| --- | --- |
| REQ-025, 035–036, 048; AC-012, 021 | Creation/describe exposes approved defaults or exact explicit bounded layouts through the common owner. Re-registration never edits. A naive time axis, unsupported transform, or silently inferred partition violates the contract. TASK-003 owns exact nine-table definitions; TASK-004 owns fanout/routing. |
| REQ-035, 038, 042, 044, 047; AC-013, 016, 018, 021 | Layout/schema journeys prove complete-list replacement, omission preservation, empty Blooms, atomic expected-version CAS, nullable additions and stable-ID renames. A stale edit, hidden Bloom floor, changed field ID or partial commit is failure. |
| REQ-038; AC-013 | Persistence journey preserves accepted-batch identity and readable old/new data across committed edits, restart and cache refresh. Startup reverting an override or a new admission using an obsolete current version is failure. TASK-004 owns underlying physical guarantees. |
| REQ-031–032, 047; AC-021; INV-001–008 | HTTP/SDK/MCP proofs retain tenant separation, permission checks, managed-field protection, originating attribution and exact error projection. Query/record grants authorizing PATCH, a foreign-table disclosure, or client-owned durable decisions is failure. |
| REQ-027, 031; AC-013 | Fresh baseline stores the new shape with no aliases, old contract reader, historical migration or automatic rewrite. Runtime in-baseline evolution remains fully supported. Static diff and architecture review establish this limit. |

## Implementation Reference

Inspected paths guide implementation; they are not an allowlist.

- **Contracts/declarations:** `crates/wyrd-spec/src/vala/api.rs` owns `PhysicalLayoutWire`, `RegisterTableRequest`, `BifrostTableDescription`; there is no PATCH; R2 adds the public table version to describe/register. Extend that owner and TASK-004's partition shape. `wyrd_types::schema::check_supported` and `spec_to_field` own conversion; `wyrd-server/src/components/cards/service.rs::declared_tables` feeds registration. Close affected Card-table declaration consumers without another layout model.
- **Layout/catalog:** `crates/vala/vala-bifrost-redux/src/catalog/layout.rs::PhysicalLayout::resolve` serves both classes. `catalog/bifrost_catalog.rs`: `register_dataset` → `create_table_locked` validates/serializes creation; `describe_table` projects schema/layout; `ensure_builtin` supplies defaults and `validate_physical_table` checks them on repeats. Extend these for explicit mutation and schema-versus-layout reconciliation. `registration` / `cached_registration` currently retain immutable layouts; revise that assumption instead of adding a configuration service.
- **Persistence/startup:** `crates/vala/vala-sql/src/queries/olap_catalog.rs` owns `get_by_fqn`, `upsert_table`, `registered_physical_layout`, with `BifrostTableRow` as control projection. Reuse it for the control projection only; edit CAS is R2's catalog publication commit, not an upsert. `wyrd-server/src/components/platform/builtins.rs::BuiltinTables::ensure_tenant` serves provisioning and boot. Preserve eager creation and stored overrides.
- **HTTP/authority:** `wyrd-server/src/bifrost/{routes,service}.rs`: `router`, `register_table`, `describe_table`, `audit::authorize`, typed `Permission::bifrost_table_write/read` and `into_public` own the workflow. Extend those owners and runtime OpenAPI, not the permission engine.
- **Rust/writer consumers:** `crates/shared/wyrd-client/src/bifrost/{facade,blocking,query,table}.rs` own `Bifrost::register`, `describe_table`, `TableConfig::{register_request,describe_with,resolve,with_layout}` and shared transport. Extend facades and thin SDK re-exports. `wyrd_queue::RowPreflight::from_description` and `wyrd_types::writable_schema` must reflect the resolved schema without client-owned durable behavior.
- **Python:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs` owns `PyTableConfig`, `apply_layout`, native describe; `python/wyrd/bifrost/_client.py` owns public sync/async facades and layout/description types. Close native registration, public exports, `__all__`, authored stubs and public tests.
- **TypeScript:** `sdks/wyrd-sdk-ts/native/src/lib.rs` decodes layout/wraps Bifrost; `wyrd/src/index.ts` owns `TableConfig`, `Bifrost.register`, `describeTable`. Extend these and regenerate `card-types.ts` and N-API `index.d.ts`; no separate transport/validator.
- **MCP/CLI:** `wyrd-server/src/mcp/bifrost.rs::describe_table` serializes the shared description. `wyrd-cli/src/query/mod.rs` uses `Bifrost::query_only`; no DDL command exists. Preserve discovery, query and declaration workflows without inventing administration interfaces.
- **Tests:** `wyrd-testing/tests/bifrost/scribe/describe.rs` already uses real SDK registration, described schemas, ingestion and reads. `scribe/support.rs` uses `WyrdTestServer::start_bound`; `scribe/recovery.rs` uses retained-root `restart_bound`. Reuse existing `scribe` target and multi-node harness; engine field-ID assertions belong in redux's existing integration target. Python model registration uses `wyrd_server`, TypeScript uses `serverTest()`, and MCP discovery drives real `rmcp` against `/mcp`.

## Verification and Evidence

All new names below are **planned**, not existing tests. Package, target, runner and setup were confirmed from Cargo manifests, current journey owners, `TESTING.md`, `.config/nextest.toml` and `mise.toml`. After adding each test, confirm its exact selector with nextest list / runner collection before RED; record any equivalent selector adjustment. Run Cargo-backed commands sequentially in this shared checkout. Use repository Postgres and local harness storage; never introduce credentials, synthetic host load or a replacement server.

### Creation proof

Planned creation/type-matrix journey:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::registered_layouts_are_bounded_and_resolved)"'
```

### Layout proof

Planned omission/replacement and competing-edit journey:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::table_patch_preserves_omitted_lists_and_compares_versions)"'
```

### Schema proof

Planned nullable-addition/stable-ID-rename journey:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::custom_schema_edits_preserve_field_ids_and_values)"'
```

### Persistence proof

Planned restart, multi-instance refresh and accepted-batch continuity journey:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::tenant_layout_overrides_survive_restart_and_refresh)"'
```

### HTTP proof

Planned real-HTTP request/error/authorization/OpenAPI journey:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=describe::table_patch_http_contract_refuses_invalid_requests)"'
```

### Python proof

Planned public Python journey in the existing model-registration story, covering the new edit/version workflow and exact stale/permission error projection:

```sh
mise run py:setup
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/bifrost/test_register_a_table_from_a_model.py::test_table_configuration_preserves_versions_and_values'
```

### TypeScript proof

Planned same public story and behavior through TypeScript:

```sh
mise run ts:build && mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/register-a-table-from-a-model.test.ts -t "^table configuration preserves versions and values$"'
```

### MCP proof

Extend this **existing** exact discovery test to observe committed table version, stored layout override, ordinary read authority and unchanged tool discovery:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)"'
```

### Static and owner checks

Run the narrow owning engine integration lane after focused proofs; it protects the shared schema/layout/publication seam reused from TASK-004:

```sh
mise run test:bifrost:integration:redux
mise run fmt && mise run lints
mise run check:deps check:tenant-isolation
mise run py:format py:lints py:typecheck
mise run ts:format ts:lints ts:typecheck ts:napi:check
mise run codegen:check
```

Run code generation from its source owner when needed before drift checks; never edit generated output by hand. Validate architecture changes by reviewing only the owning table/declaration/public-surface sections against the approved supplement. Record HTTP/code assertions, before/after descriptions and versions, field IDs at their owning test tier, restart/cache observations and exact authorized read results. No performance improvement is claimed by these proofs. Integrated all-surface capability suites and final docsite checks run once at change review, not as extra full suites for this task.

## Material Stop Conditions

- TASK-004-R2 cannot retain accepted versions or atomically publish an
  expected-version edit. Report the missing prerequisite rather than shipping a metadata-only mutation or a second publication coordinator.
- Correctness would require a different public edit shape, new operation/type,
  permission model, historical migration, automatic rewrite or changed default. Return the conflict to specification authority instead of adding an option.
- Existing cache/publication mechanisms cannot make committed configuration
  observable consistently across instances without a material new lifecycle owner. Report the ownership gap before adding one.
- TASK-003's canonical definitions cannot be configured without weakening
  canonical schema/sensitivity/managed-field protection. Resolve the shared owner boundary rather than duplicating built-in definitions.

## Authority Links

- [Approved specification revision 16](../spec.md), and the complete approved
  [table supplement](../supplements/table-storage-and-correlation.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md#table-and-row-identity),
  [declared types](../../../../architecture/bifrost-design.md#declared-types-and-the-registered-schema), and [public surface](../../../../architecture/bifrost-design.md#public-surface). Approved conflicting table/evolution/default contracts are amended in these owners, rather than preserved as alternate rules.
- [Permission authority](../../../../architecture/wyrd-security-posture.md#permission-model),
  [root contributor rules](../../../../AGENTS.md), [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md), [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).
- [Spec-driven workflow](../../../spec-driven-development.md),
  [implementation execution](../../../implementation-execution.md), [testing](../../../../TESTING.md), [agent guide](../../../../agent_docs/index.md).

## Remediation R1 Evidence

Remediation: `review/TASK-005-6216c0641/TASK-005-R1-atomic-alteration-and-contract-closure.md`.
Commits `ae00c0c03..b9129296e`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 nested strictness | `wyrd-spec/src/vala/api.rs`: `deny_unknown_fields` on `DataTypeSpec`, `FieldSpec`, `SortKeyWire`; `present` on timestamp `tz`, `buckets`, `width`, `time_column` | `alter_table_request_decodes_strictly`; `describe::table_patch_http_contract_refuses_invalid_requests` | PASS |
| FIND-2 own committed result | `BifrostCatalog::alter_table` projects from the committed metadata and re-read row before the tenant commit (`describe_metadata`) | `describe::an_edit_returns_its_own_committed_definition` (deterministic pause after publication) | PASS |
| FIND-3 cache fencing | `catalog/cut_cache.rs::ListenedCache` (shared by the cut cache and the catalog's `CurrentEntries`); fills fenced by generation, served only while listening | `cut_cache_serves_only_fresh_listened_cuts` (stale fill vs invalidation, eviction, reset; disconnected fill); `scribe::accepted_retry::an_unheard_edit_still_governs_new_admission` (red with fencing disabled); `ensured_builtin_answers_from_the_registration_cache` | PASS |
| FIND-4 atomic publication | Fallible projection values are derived before publication; the single transaction is not implemented | — | SPEC_REVISION_REQUIRED |
| FIND-5 rustdoc | `announce`, describe.rs `pod`/`flush`/`CONFLICT`/`field_ids`, support.rs clients, OpenAPI and MCP tests, partial-progress contracts on catalog/service/client/blocking/napi/PyO3 `alter_table` | `mise run lints`, `ts:napi:check` | PASS |
| FIND-6 native Python stub | `python/wyrd/_wyrd/__init__.pyi`, `_wyrd/bifrost.pyi` | `test_native_bifrost_stub_matches_runtime_signatures`; `py:lints`, `py:typecheck`; Python edit journey | PASS |
| FIND-7 handler span | `wyrd-server/src/bifrost/routes.rs` `bifrost.table.alter`, `skip_all` | `mise run lints` | PASS |
| FIND-8 defaulted inputs | `scripts/gen_ts_types.py` `SPLIT_REQUEST_ROOTS`; `FieldSpec2Input`, `DataTypeSpecInput` | `codegen:check`, `ts:typecheck`, TS edit journey omitting `nullable` | PASS |
| FIND-9 orphan cursor oracle | `forge/production_routes.rs` uses `layout::forge_data_root` | exact traced selector | PASS |
| Duplicate publisher removed | `wyrd-testing/src/bifrost/revisions.rs` helpers call `BifrostCatalog::alter_table` from the described version; `commit_table_revision` deleted | the six selectors below | PASS |

Commands (each under `scripts/postgres/with-test-postgres.sh` after `db:migrate:inner` where Postgres is needed):

- `cargo nextest run --locked -p vala-bifrost-redux -P journey --run-ignored=all -E 'test(cut_cache_serves_only_fresh_listened_cuts) | test(ensured_builtin_answers_from_the_registration_cache) | test(registration_cache_keeps_found_uids_only) | test(an_unheard_edit_still_governs_new_admission) | test(a_retry_the_current_layout_refuses_is_acknowledged_under_its_own)'`: 5 passed.
- `cargo nextest run --locked -p wyrd-testing --test scribe --test forge --test oracle --test otlp -P journey --run-ignored=all` for `write_read::accepted_batch_revision_survives_alteration_and_restart`, `write_read::mixed_runtime_specifications_preserve_rows_and_pins`, `scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth`, `distributed::bloom_policy_versions_answer_exactly_through_compaction`, `negative::pg_tests::otlp_replay_across_a_definition_edit_is_acknowledged_once`, `negative::pg_tests::otlp_replay_across_a_partition_edit_is_acknowledged_once`: 6 passed.
- `cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(/^describe::/)'`: 7 passed, including persistence/restart.
- `cargo nextest run --locked -p wyrd-mcp -P journey --run-ignored=all -E 'test(/::agent_discovers_only_authorized_tables_and_layout$/)'`: passed.
- `pnpm exec vitest run tests/integration/register-a-table-from-a-model.test.ts -t "^table configuration preserves versions and values$"`: passed.
- `uv run python -m pytest -q tests/integration/bifrost/test_register_a_table_from_a_model.py::test_table_configuration_preserves_versions_and_values`: passed.
- `mise run test:bifrost:integration:redux`: 933 passed.
- `mise run fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `git diff --check`: pass.

Limits:

- FIND-4 needs a spec revision. `architecture/wyrd-security-posture.md` bars the operator pool from handlers. Migration `20260619000000_iceberg_catalog.sql` grants the tenant login no access to `iceberg_catalog`; tenant work reaches a pointer only through the read-only definer `vala.oracle_catalog_metadata_location`. So the native pointer CAS on the catalog login and the TenantConn/RLS projection cannot share one transaction without a new security decision: either a write-capable definer, or tenant SQL on the catalog login. The partial-progress window is documented on every entry point.
- Without a live listener, the registration and definition caches no longer serve. Admission then reads its authority uncached, as the cut cache already does.
- Non-goals stayed excluded. No unrelated files changed.

## Remediation R2 Evidence

Remediation: `review/TASK-005-a9d1fd2c2/TASK-005-R2-tenant-bound-atomic-publication.md`. Commits `b39e4fb5e..1a34d4ded` on prior candidate `a9d1fd2c2`.

Reuse: `BifrostCatalog::alter_table` stays the one publisher. It stages the edited metadata with public Iceberg APIs (`TableUpdate::apply`, `MetadataLocation`, `TableMetadata::write_to`). It then publishes inside its existing TenantConn transaction through one new narrow definer, `vala.bifrost_publish_table_definition`, which follows the `oracle_catalog_metadata_location` pattern. That definer is the only gap. Neither fork pin changed: the definer does the pointer CAS and records the retained version reference, and the unchanged fork still owns every other catalog commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4 single commit/rollback | `alter_table`: definer swap + `upsert_table` + `announce_definition_change` + projection in one TenantConn; uncertain COMMIT resolved from the pointer | `catalog::bifrost_catalog::production_pin_tests::a_dropped_edit_publishes_nothing` (RED at a9d1fd2c2: described `TableVersion(2)` after the drop; GREEN after) | PASS |
| FIND-4 committed edit, restart, peer refresh, accepted retry | unchanged R1 listener/cache owners | `describe::tenant_layout_overrides_survive_restart_and_refresh`, `describe::an_edit_returns_its_own_committed_definition`, `scribe::accepted_retry::an_unheard_edit_still_governs_new_admission` | PASS |
| FIND-4 separate-login boundary and CAS | migration `20261026000000_bifrost_definition_publication.sql`; `olap_catalog::publish_definition` | `pg_oracle_membership::pg_tests::definition_publication_is_tenant_bound_and_transactional`: direct catalog write denied; refuses another tenant, an unregistered table, another identity, a foreign directory, a skipped version and a stale pointer; rollback publishes nothing; commit records exactly v2 | PASS |
| FIND-4 concurrent data commits | bounded replan (`PUBLICATION_ATTEMPTS`) when the swap misses at an unchanged version | the six revision journeys and the describe suite (red before the replan, see diagnosis) | PASS |
| FIND-5 helper contract | `revisions.rs::alter_table_from_current` "Repetition, cancellation, and retry" section, referenced by all 8 helpers; `alter_table` and `service.rs` cancellation prose matches the single commit | `mise run lints` | PASS |

Diagnosis (unexpected journey failures after the first atomic cut):
- **Symptom:** `describe::tenant_layout_overrides_survive_restart_and_refresh` and `write_read::mixed_runtime_specifications_preserve_rows_and_pins` failed with `TableVersionConflict` at the unchanged version 1.
- **Evidence:** a `Forge commit notice recorded` line for the same table appears right before each failure. The definer's `metadata_location = p_expected_location` swap missed.
- **Cause:** Forge promotion commits move the pointer without changing the definition. The old `Transaction::commit` path rebased the edit through the fork's `update_table`; the fixed-base swap did not.
- **Fix site:** `BifrostCatalog::alter_table` replans from the moved pointer. All callers (server, SDKs, `revisions.rs`) route through it.
- **Independent diagnostician:** same cause and fix site; no SQL change needed.

A second red came from an inverted swap condition in the replan. Instrumentation showed the pointer unchanged after a "successful" edit. It was fixed in the same commit.

Commands, each run under `scripts/postgres/with-test-postgres.sh` after `db:migrate:all:inner`:
- `cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/a_dropped_edit_publishes_nothing$/)'`: 1 passed.
- `cargo nextest run --locked -p vala-sql --test integration`: 66 passed.
- `cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(/^describe::/)'`: 7 passed.
- The six revision journeys (R1 selectors, `test(/::name$/)` form): 6 passed.
- `mise run test:bifrost:integration:redux`: 934 passed.
- `mise run fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `git diff --check`: pass.

Limits:
- The TS/Python edit journeys were not rerun. Client code is unchanged, and the server path is covered by the Rust/HTTP describe journeys.
- No native or Python declaration changed, so codegen was not rerun.
- An unreferenced staged metadata file is left behind on a dropped or replanned edit; storage cleanup is a non-goal.
- Non-goals stayed excluded: no grants, no role, no operator transaction, no fork change, no journal, no public contract change.
