---
id: TASK-008
title: Delete the per-row tenant column; prove the tenant per file
kind: implementation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-015]
acceptance: [AC-017]
depends_on: [TASK-007]
blocks: []
---

## Outcome and Value

Scans stop decoding and shipping a 36-byte tenant string on every row (23% of
staged decode today, nearly the whole payload of `COUNT(*)`), while tenant
isolation still fails closed.

## Requirements

1. Delete `data_tenant_id` from the managed envelope (`wyrd-spec`
   `vala::managed_columns`), every write path (Gate, Scribe memtable, WAL
   recipe, staging writers, Forge promotion/compaction), every table schema,
   and every scan projection (`oracle/exec.rs` `OracleScanProjection`).
2. Every staged and published Parquet file records its tenant in footer
   key-value metadata, written from the same authenticated binding (seal key /
   table binding). Forge rewrites carry it forward from the table binding.
3. Opening a file compares the footer tenant with the query's tenant once; a
   missing or different value fails closed with
   `WYRD_VALA_500_TENANT_TRIPWIRE` before any row is returned. The check lives
   in the one shared Parquet scan (TASK-007), so published, hot, and staged
   reads all get it.
4. Delete the per-row tenant `FilterExec` (`provider/tenant_filter.rs`) and
   the per-row `TenantTripwireExec`, including its codec extension, unless the
   per-file check needs the node to carry the tenant through a distributed
   plan; in that case keep one node that carries the tenant and does no
   per-row work.
5. In-memory rows are bound by their seal key; no per-row check.
6. No compatibility path: files without the footer tenant are refused.
7. Update `architecture/bifrost-design.md` (lines ~40-90: envelope list,
   "Every physical file retains `data_tenant_id`", tripwire paragraph, global
   batch identity wording) and any user docs that list the column.

## Verification

- A focused test: a file whose footer tenant is foreign or missing fails with
  `WYRD_VALA_500_TENANT_TRIPWIRE` before rows are returned.
- No Bifrost Arrow/Parquet schema, write recipe, or scan contains the
  `data_tenant_id` row column. Postgres columns named `data_tenant_id`
  (migrations, RLS, SQL queries) are out of scope and unchanged.
- Run only affected tests: the focused test above, unit tests for touched
  modules, and the tenant-isolation journeys (exact nextest expressions or
  the narrowest `test:bifrost:journey:<capability>` leaves). After a fix,
  re-run only what failed. Do not run the full `mise run test:bifrost` lane;
  the caller runs it once after this task. `mise run codegen:check` and
  `mise run docs:check` pass.
- `mise run fmt`, `mise run lints`, `git diff --check`.

## Evidence

| Requirement | Implementation | Verification | Result |
|---|---|---|---|
| R1 delete the per-row column from the envelope, writes, schemas, and projections | `wyrd-spec` `vala::managed_columns` (no `DATA_TENANT_ID`); `schema/managed_columns.rs`, `tables/managed_columns.rs` (envelope id 1008 retired); `scribe/execution_lanes.rs`, `scribe/parquet_writer.rs` (`stamp_tenant` deleted), `contracts.rs`; `oracle/exec.rs` `OracleScanProjection` | `scribe::execution_lanes`, `scribe::tests`, `tables::` and `schema::` unit tests assert that no `data_tenant_id` field exists; the `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` journey refuses any closure that reads the column | PASS |
| R2 the footer records the tenant from the authenticated binding, and Forge carries it forward | `parquet/footer.rs` `KEY_TENANT`/`tenant_key_value`/`BifrostFooterIdentity`; `scribe/parquet_writer.rs` `ArtifactPlan.tenant`; `scribe/claim_assembly.rs` `AssembleClaimRequest.tenant`; `parquet/writer_properties.rs` `bifrost_rewrite_writer_properties(.., tenant)`; `forge/managed/{policy,executor}.rs` | `scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence`, `generation_encoding_preserves_sort_tenant_and_artifact_identity`, `scribe::claim_assembly` (each assembled object verifies its footer tenant); journeys `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` and `scribe_promotion::forge_hot_object_promotes_unchanged_and_remains_exact` read the output after the rewrite | PASS |
| R3 one footer comparison per opened file; missing or foreign fails closed before any row | `oracle/exec.rs` `verify_scanned_footer_tenant`, called from `PublishedFooterLoader::load` and `tenant_proven_reader_metadata` (hot and staged); published footers are mandatory; files with `key_metadata` are refused; the leader's `Oracle::audit_tenant_refusal` emits one security audit event (`TenantFile`) | `parquet::footer::tests::footer_tenant_proof_refuses_missing_and_foreign_tenants`; `oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row`; journey `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` (a foreign footer under `count(*)` gives 0 rows and `QueryTenantInvariant`, both locally and distributed) | PASS |
| R4 delete `tenant_filter.rs` and `TenantTripwireExec` with its codec extension | `src/provider/` deleted (the catalog uses `IcebergStaticTableProvider`); the tripwire, codec tag/payload and follower/codec audit plumbing are deleted; the dead audit ownership in wyrd-server's Scribe `fragment_query_audit` is removed | `mise run lints`; `oracle::codec`, `oracle::follower`, and `oracle::dispatcher` unit tests | PASS |
| R5 in-memory rows are bound by the seal key | `scribe/staging_runtime.rs` and `claim_assembly.rs` take the tenant from `binding.tenant`; `oracle/mod.rs` `hot_metadata_key(cut.binding.tenant, ..)` | `scribe::` and `oracle::follower` unit tests; journey `write_read::scribe_write_flush_read_user_journey` | PASS |
| R6 no compatibility path | a missing footer tenant is refused; a missing published footer is refused | same focused tests as R3 (the missing case) | PASS |
| R7 docs | `architecture/bifrost-design.md` (the envelope, the per-file footer proof, `WYRD_VALA_500_QUERY_TENANT_INVARIANT`); `references/domain/{datafusion,olap-serving}.md`; `docs/src/content/docs/bifrost/{architecture,data-plane-internals,forge,index,quickstart,schema,writing-data}.svx` | `mise run docs:check` | PASS |

Commands: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=parquet::footer::tests::footer_tenant_proof_refuses_missing_and_foreign_tenants) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row)'` (2/2 pass). The touched-module unit set ran under `scripts/postgres/with-test-postgres.sh`, filtered by `-E 'test(/^(parquet|scribe::(parquet_writer|claim_assembly|staging_runtime|member_stager|execution_lanes|persistence|tail_rpc|tests)|oracle::(exec|follower|codec|dispatcher|query_stream|pruning|analytical|analytical_transport|tests)|catalog|forge::managed|tables|schema|contracts|storage::cache)::/)'` (334/334 pass). `wyrd-spec --lib -E 'test(/^vala::/)'` (444/444 pass). The journeys ran through `with-test-postgres.sh` with `db:migrate:inner` and `-p wyrd-testing -P journey --run-ignored=all`, using an exact expression for each of the 6 journeys named above (6/6 pass). `mise run fmt`, `mise run lints`, `mise run codegen:check` (the audit schema was regenerated: `tenant_row` became `tenant_file`), `mise run docs:check`, and `git diff --check` all pass.

Limits: `git grep data_tenant_id` cannot be empty. The name remains the Postgres tenant column (file_list, forge, audit_staging, RLS) and the MCP rejected-argument name; only the Arrow/Parquet row column was removed. The spec's `WYRD_VALA_500_TENANT_TRIPWIRE` does not exist; the existing `WYRD_VALA_500_QUERY_TENANT_INVARIANT` is used. The PeerCluster canonical-span journey was not run (TASK-006 harness).

### Exact proof on the integrated tree (TASK-007 + TASK-007-R1 + TASK-008 + TASK-008-R1), 2026-09-30

Executed by the lead in one run through the Postgres wrapper, after
`mise run db:migrate:inner`, one exact expression per test. Every line
selected exactly one test and passed (log:
`scratchpad/final-named.log`). The table above records the implementer's
earlier runs; this block is the current, reproducible proof.

Unit tests, each `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=<name>)'`:

| Name | Result |
|---|---|
| `parquet::footer::tests::footer_tenant_proof_refuses_missing_and_foreign_tenants` | PASS |
| `oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row` | PASS |
| `scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence` | PASS |
| `scribe::parquet_writer::tests::generation_encoding_preserves_sort_tenant_and_artifact_identity` | PASS |
| `schema::managed_columns::tests::with_managed_columns_appends_the_envelope_without_a_tenant_column` | PASS |
| `oracle::follower::tests::scribe_live_sources_keep_the_session_partition_count` | PASS |
| `oracle::live::tests::native_completion_reconciles_the_delivered_output` | PASS |
| `oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups` | PASS |
| `oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately` | PASS |
| `oracle::follower::tests::scribe_provider_projects_and_filters_a_nonzero_ordinal` | PASS |
| `scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication` | PASS |
| `resources::tests::scribe_follower_execution_shape_contract` | PASS |
| `oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer` | PASS |

Server unit tests, each `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=oracle::peer_service::tests::<name>)'`:
`scribe_fragment_failure_classes_survive_to_dispatch`, `stale_object_dispatch_status_is_not_found`, `empty_scribe_attempt_emits_schema_and_complete_footer` — PASS.

Journeys, each `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test <target> -P journey --run-ignored=all -E 'test(=<name>)'"`:

| Target | Name | Result |
|---|---|---|
| `forge` | `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` | PASS |
| `forge` | `scribe_promotion::forge_hot_object_promotes_unchanged_and_remains_exact` | PASS |
| `oracle` | `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads` | PASS |
| `oracle` | `published::published_cache_pruning_and_shutdown_are_production_governed` | PASS |
| `scribe` | `write_read::scribe_write_flush_read_user_journey` | PASS |
| `server` | `query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` | PASS |
| `oracle` | `analytical_activation::selected_peer_failure_is_terminal` | PASS |
| `oracle` | `peer_network::transport::peer_transport_uses_immutable_fenced_destinations` | PASS |
| `oracle` | `distributed::published_workers_and_live_scribes_share_one_plan` | PASS |

Static gates on the same tree: `mise run fmt`, `mise run lints` (exit 0),
`mise exec -- cargo clippy --locked -p vala-bifrost-redux --features test-support --all-targets -- -D warnings`
(clean after the TASK-008-R1 import and panic-doc fixes), `git diff --check`.
Deferred to the caller: the full `mise run test:bifrost` lane and the
capacity benchmark, run once after review.
