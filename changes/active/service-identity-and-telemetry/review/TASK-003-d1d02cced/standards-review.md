# Repository standards review

**Result: FAIL.** One consolidated mandatory rustdoc finding, `REPO-003-001`, remains. No source was changed.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/schema-change-task-3`; base `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b`; candidate `d1d02cced07582a8f817d3f5651e7895b96d046d`. First review of the complete 328-file `git diff ca2ac4a37...d1d02cced`, with no carried-forward acceptance. HEAD still names the candidate. The only working-tree additions during review are review reports.

Read the approved revision-16 specification, the three supplements and original TASK-003 for scope. This report audits repository rules across the full changed surface, including private Rust and test items, Python, TypeScript, native bindings, SQL, generated artifacts, documentation, examples and mise. It does not substitute for behavior, security, data or resilience acceptance and makes no redesign recommendations. There is no `.codegraph/` directory.

## Authority coverage

All rows also inherit root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md` and `agent_docs/maintainer-style.md`. All Rust rows additionally inherit `agent_docs/rust.md`; caller-visible failures inherit `agent_docs/errors.md`; machine interfaces/generated declarations inherit `agent_docs/agent-surfaces.md`. `TESTING.md` governs verification and test layout throughout.

| Changed surface | Additional governing authority | Source coverage |
|---|---|---|
| Root rules, architecture and task evidence | `architecture/README.md`; spec-driven authority order; approved spec revision 16 | Entire changed authority/task hunks; spec and supplement approval/scope, TASK-003 scenarios and recorded diagnoses/checks |
| `crates/wyrd-spec/src/{auth,card,gateway,ids.rs,reference.rs,vala,setup-prompt.md}` | `wyrd-design.md`: runtime identity, configured Service, telemetry setup, stock SDK context, Bifrost, error catalog; auth/telemetry/storage supplements | Pure selector and public error/type/re-export changes; canonical names, managed constants, describe fields and generated consumers |
| `crates/shared/wyrd-{auth-verify,runtime,telemetry}` | `wyrd-security-posture.md`: request principal, permission model, audit; `wyrd-design.md`: trace context | Grant/catalog substitutions, identity routing, telemetry export names; no dependency/feature additions |
| `crates/shared/wyrd-client/src/{bifrost,observe,error.rs}` | Client model; Bifrost public surface; seven-field/subject-column contract | Correlation through facade/handle/observe/queue, schema discovery, trace SQL and domain-field projection; accompanying source/integration tests |
| `crates/shared/wyrd-queue` | Bifrost producer/admission ownership and reserved-column contract | `batch_builder`, producer correlation fields and error text; changed inline tests |
| Redux `catalog`, `namespaces`, `contracts`, `forge/identity`, `forge/leader`, `parquet`, `scan_sizing` | `bifrost-design.md`: table/row identity, table versions/publication, storage, query and maintenance | Logical/physical namespace projection, grants, described fields, layout/Bloom use and changed tests |
| Redux `gate`, `scribe` | `bifrost-design.md`: ingest, durability/visibility, live-tail/recovery, resource/failure invariants; security posture trust/tenant boundaries | Registry descriptor lookup and actual SQL types, auth context and canonical/native dispatch, source conformance, row stamping, managed arrays, WAL/replay/persistence fields and tests |
| Redux `tables/**` | Table/row identity; telemetry schema supplement; storage supplement exact ledger | All nine table ledgers and defaults, shared envelope/dimensions, resource/span/log/metric projections, canonical validators, Arrow/Parquet/Iceberg and layout proofs |
| Redux `oracle/**` and integration Forge tests | Immutable planning/source authority, query admission, maintenance | Catalog root/query resolution and source-consumer substitutions; integration fixture placement and changed helpers |
| `crates/vala/vala-sql` | Security posture Postgres tenant binding, Bifrost SQL/catalog authority, SQL ownership from root; SQL connection/transaction principles | New migration in full; actual `TenantConn`/`OperatorPool` production signatures and named statement-owner exception; namespace identities and integration tests |
| `crates/wyrd/wyrd-auth` | Security posture token issuance/permission model | Grant scope decode/issuance substitutions; no new auth transport or durable SDK behavior |
| Server `bifrost`, `boot`, `grpc`, `http`, `mcp`, `oracle`, `query` | Bifrost serving boundary/public surface; security posture request/tenant boundary; agent surfaces guide | Registry composition, HTTP/gRPC adapters, request-context errors, MCP SQL examples, query floor, typed error projections and changed tests |
| Server `components/gateway`, `components/verification`, `verification`, `scribe_outbox` | Runtime/stock-provider context, Bifrost attribution/audit contract, security trust boundary; operations audit failure boundaries | Accepted context, batch HTTP context forwarding, call captures/result/input rows, source SQL/readers, local/peer delivery and modified private/test items |
| Server/CLI/MCP integration tests and `wyrd-testing/{src,tests}` | Root test tiers; `TESTING.md` layout and capability lanes | Full changed test/fixture surface including native/OTLP refusals, nine-table describe, trusted origins, gateway headers, lifecycle read helpers and sandbox-worker helper |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs` | `crates/wyrd/wyrd-sql/AGENTS.md` in full; security Postgres tenant binding | Migration test's renamed logical namespace; no changed production connection API in this crate |
| Python SDK Rust, Python, authored stubs, unit/typecheck/agent/integration tests | `sdks/wyrd-sdk-python/AGENTS.md` in full | Public `Correlation` inline declaration and stub re-export; native parameter extraction/registration owner; `record`/`insert` facade forwarding; observe/state stubs; public-import journeys and negative tests |
| Rust SDK source/integration support and stories | Client model, root Rust and test rules | Re-exports and query/correlation consumers, authenticated stories and changed support helpers |
| TypeScript native/native-testing, generated `.d.ts`/`.d.cts`, public source and tests | `sdks/wyrd-sdk-ts/AGENTS.md` in full | N-API correlation extraction, both generated native declarations, Rust-generated wire types, wrapper forwarding, test-only binding boundary and real server journeys |
| Docs content, generated error page and `llms-full.txt` | `docs/AGENTS.md` in full; `docs/CONTRIBUTING.md`; `docs/PRODUCT.md`; source error catalog | Changed prose/output/reserved-name literals and their generator/source inputs; no new guide, component or diagram |
| Examples in Rust/Python/TypeScript/SQL and `mise.toml` | Root platform/client/command rules; relevant public contract and existing journey ownership | Canonical SQL/correlation literals and example readers; mise descriptions only (no task membership, commands or feature-union change) |

There are exactly four nested `AGENTS.md` files in this repository and all were read: Python SDK, TypeScript SDK, docs and wyrd-sql. No touched subtree has an additional rule file. Operational authority read was limited to the changed audit-history paragraphs and their surrounding failure/deployment sections. Narrow architecture coverage included the Bifrost identity/stamping/catalog and resource owners and security Postgres exception, rather than treating supplied checks as authority.

## Rule results

| Applicable rule | Result | Evidence and limit |
|---|---|---|
| Canonical owner and vocabulary; no public legacy catalog alias | PASS, static scope | `catalog/table_ref.rs`, `namespaces.rs`, permission-scope validation and changed clients/readers use `bifrost`; old catalog strings retained in explicit refusal tests do not introduce an alias. Internal Vala crate/SQL names are intentionally retained. Full docsite cutover is TASK-008, not silently added here. |
| Pure spec; server-owned durable behavior; thin first-class SDKs | PASS | `auth/workload.rs::deployment_environment_selector` is pure; descriptor IO remains server-side `gate/attribution.rs`; Python/TS forward through shared client/queue. No Cargo manifest, dependency or feature changed in this range. |
| Production SQL capability types, transaction/clock ownership | PASS | New `CardRegistry::read_descriptor` takes `&mut TenantConn<'_>` (qualified spelling) and `described` acquires through `WyrdPostgres::tenant_conn`; it neither commits nor nests transactions. Scribe persistence/file-list/replay retain `OperatorPool`; tenant file-list/batch-commit APIs retain `TenantConn`. `oracle_reader_authority.rs::OracleTableCuts` retains its expressly documented single-statement raw-connection exception; its changed field documentation does not create a new exception. Test-only pools are not production signatures. No new caller-owned coordination timestamp is introduced. |
| Migration immutability and definer scope | PASS | `20261027000000_bifrost_catalog.sql` is a new migration, not an edit to an applied one. Both definers pin empty `search_path`, derive physical namespace from `wyrd.current_tenant()`, and qualify catalog relations. Its `NOT VALID` constraint intentionally leaves old development rows unconverted while enforcing new writes. |
| Stable errors and boundary validation | PASS, standards scope | New environment selector returns existing `WyrdError::Validation`; `IngestError::RequestContext` carries the shared refusal through existing mappings. Canonical validators return `BifrostError`; PyO3/N-API convert at the edge. No handwritten catalog or duplicate problem-json implementation added. Behavioral completeness is independently reviewed elsewhere. |
| Universal managed schema has one owner | PASS | `tables/managed_columns.rs::MANAGED_COLUMNS` declares all seven fields, policy duplication removed; queue/spec constants and describe projections use the shared owner. Shared domain dimensions remain separate. |
| Python declaration/export/transport boundary | PASS, static parity | Public `Correlation` changes inline in `_client.py`; `bifrost/__init__.pyi` re-exports that one declaration. Native and facade positional arguments change together; observe `record` reads the same key. Existing registration/exports are retained, and changed integration tests import public modules. No new runtime/GIL-lifetime owner or PyO3 feature is introduced. |
| TypeScript native/generated/public projection | PASS, static parity | Native `insert`/`record` parameters match generated `subjectCardUid` in both declaration formats; public `Correlation` and forwarding use the same spelling. `card-types.ts` comment changes originate in Rust contract comments. No V8 handle lifetime, durable behavior or production test-binding export is added. Generator reruns are supplied evidence, not rerun here. |
| Documentation/generated source relationship | PASS for changed literals | New reserved-column remediation is derive metadata in `wyrd-spec/src/vala/error.rs` and appears consistently in generated errors/llms output; native/wire declaration changes have owning Rust sources. Source paths and generator tasks were inspected; no claim is made that Git alone proves which editing tool produced an artifact. |
| User journeys and test tier placement | PASS, standards scope | Real SDK/server journeys exist in Rust/Python/TS and MCP/HTTP for the changed surface. Newly ignored Rust journeys belong to existing dedicated Postgres lanes with `--run-ignored=all`; this is lane placement, not evidence of suppressing a failure. External test modules use existing real-server binaries. No new binary/dependency or host-load generator added. |
| Platform/dependency/secret/runtime constraints | PASS | Added production code uses portable Rust/Arrow/SQL primitives and existing caches/runtimes. No platform gating, credential switching, forbidden OS mechanism, unsafe code, new runtime construction, dependency or secret-bearing credential field added. |
| Every new/changed Rust item has meaningful rustdoc and required sections, including private/test items | **FAIL** | `REPO-003-001`, exact inventory below. Green lint runs do not enforce this private/test documentation gate. |
| Test correction diagnosis discipline | No confirmed violation | Task records diagnoses for Bloom floor, observation-schema mismatch and MCP fixture expectations; source changes follow those owners. Worker-launch priming in `sandbox_worker.rs` is outside the telemetry behavior but does not delete a deadline/assertion or prove a failure was hidden. No separate diagnosis artifact was supplied. This report does not infer a mandatory violation merely from that absence. |

## REPO-003-001 — Missing mandatory rustdoc across changed Rust items

**Classification:** VIOLATION. **Obligation:** root `AGENTS.md`, Code: “Every new or changed Rust item has rustdoc … `# Errors` on fallible items, `# Panics` when it can panic. Missing rustdoc blocks merge.” `agent_docs/rust.md`, Rustdoc explicitly includes fields, private helpers, test helpers and tests, regardless of visibility. The task-review skill explicitly preserves this mandatory gate.

**Source-proven failure:** new `ResourcePromotionColumns` declares six undocumented private accumulator fields at `tables/signal.rs:508–513`. The new `canonical_catalog_has_no_alias` at `catalog/table_ref.rs:129` documents its purpose but calls assertions, `expect` and `panic!` without `# Panics`. The materially changed capture-peer method at `wyrd-server/src/grpc/capture_peer.rs:102` returns `Result<_, Status>`, now also failing on registry unavailability, without `# Errors`. Existing changed tests and private implementations below have the same gate gaps. This is a current-diff obligation, not a request to document unrelated untouched debt.

**Consequence/materiality:** the explicit merge gate is unsatisfied; private ingestion/projection ownership, error conditions and test/helper failure assumptions remain absent from the required source documentation. Passing tests or public-only compiler documentation lints cannot waive that gate.

**Smallest correction:** add accurate item-local rustdoc only to the changed items listed below, describing their actual workflow/invariants; add the missing `# Errors` and `# Panics` sections with actual failure causes. Reuse existing nearby documentation vocabulary. Preserve source behavior, assertions, deadlines, errors and test placement. No helper, dependency, lint exemption or runtime test is needed. This is one bounded documentation correction bundled with any functional remediation, not a separate style round.

### Missing item documentation

Paths below are repository-relative; line numbers identify candidate declarations.

| File | Exact affected items |
|---|---|
| `crates/vala/vala-bifrost-redux/src/tables/signal.rs` | 508 `service_namespace`, 509 `service_instance_id`, 510 `process_pid`, 511 `telemetry_sdk_name`, 512 `telemetry_sdk_language`, 513 `telemetry_sdk_version` on `ResourcePromotionColumns` |
| `crates/vala/vala-bifrost-redux/src/catalog/table_ref.rs` | 107 `new_and_fqn_round_trip`, 114 `parse_fqn_accepts_known_namespace`, 155 `parse_fqn_rejects_path_traversal` |
| `crates/vala/vala-bifrost-redux/src/catalog/tenant_table.rs` | 307 `tenant_table_binding_uses_authenticated_organization`, 411 `tenant_table_binding_rejects_tenant_in_logical_name` |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs` | 1891 `auth_context` test helper |
| `crates/vala/vala-bifrost-redux/src/namespaces.rs` | 177 `bifrost_namespace_datasets_variant_round_trips` |
| `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs` | 2001 `parquet_writer_writes_bloom_and_page_index` |
| `crates/vala/vala-bifrost-redux/src/scribe/wal.rs` | 4035 `slice_payload_decoder_rejects_invalid_table_identity` |
| `crates/vala/vala-bifrost-redux/src/tables/logs/records.rs` | 108 `DomainTable::physical_layout` implementation |
| `crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs` | 195 `DomainTable::physical_layout` implementation |
| `crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs` | 161 `DomainTable::physical_layout` implementation |
| `crates/vala/vala-sql/tests/integration/pg_file_list_cluster_nodes.rs` | 165 `vala_file_list_rejects_duplicate_stream_range`, 306 `vala_file_list_rls_blocks_cross_tenant_select`, 360 `vala_operator_pool_can_insert_and_select_file_list` |
| `crates/wyrd-spec/src/vala/api.rs` | 2770 `bifrost_wire_field_spec_carries_correlation_metadata` |
| `crates/wyrd-spec/src/vala/audit_detail.rs` | 1089 `all_detail_variants_round_trip`, 1293 `serde_cannot_bypass_secret_classification`, 1379 `audit_event_golden_shape_distinguishes_absent_and_present_detail` |
| `crates/wyrd/wyrd-server/src/bifrost/convert.rs` | 30 `bifrost_tables_namespace_parse_rejects_unknown` |
| `crates/wyrd/wyrd-server/src/bifrost/service.rs` | 702 `bifrost_tables_register_reserved_namespace_is_rejected`, 716 `bifrost_tables_list_and_describe_reflect_registration` |
| `crates/wyrd/wyrd-server/src/query/floor.rs` | 337 `query_floor_accepts_select_with_allowlisted_functions`, 343 `query_floor_rejects_non_select`, 364 `query_floor_rejects_disallowed_function`, 378 `query_referenced_tables_extracts_relation_names` |

### Missing `# Errors`

| File | Exact affected item |
|---|---|
| `crates/wyrd/wyrd-server/src/grpc/capture_peer.rs` | 102 `ingest_capture`: malformed frame, registry unavailable and Scribe refusal status paths |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs` | 500 `mcp_rejects_credentials_and_joins_request_and_process_cancellation`: returns `Result<(), McpJourneyError>` and propagates server/HTTP/MCP lifecycle failures |

### Missing `# Panics`

Each listed declaration is changed in the candidate and contains an assertion, explicit panic or `expect` (including debug assertions for `TenantTable::from_facts`). The section must identify the actual test/helper invariant; generic filler is insufficient. Items also missing their purpose docs are intentionally included in both inventories. Locations are grouped by file to keep the correction bounded.

| File | Exact affected declarations |
|---|---|
| `crates/shared/wyrd-client/src/bifrost/mod.rs` | 786 `table_config_rejects_a_reserved_column` |
| `crates/shared/wyrd-client/src/observe/tests.rs` | 1274 `drift_projects_one_tall_row_per_feature`; 1367 `record_describes_a_dataset_table_once`; 1401 `concurrent_first_records_describe_each_table_once`; 1450 `record_refuses_reserved_and_system_tables`; 1483 `record_refuses_an_unknown_dataset_table` |
| `crates/shared/wyrd-client/tests/integration/pg_bifrost_e2e/variant_tables.rs` | 234 `query_results_copy_into_another_table`; 347 `unsupported_type_is_refused_by_sdk_and_server`; 406 `model_allowing_extra_keys_is_refused`; 422 `each_unsupported_type_is_refused_when_declared` |
| `crates/shared/wyrd-client/tests/integration/startup_image_journey.rs` | 324 `kind_seed` |
| `crates/shared/wyrd-queue/src/batch_builder.rs` | 844 `builds_user_columns_plus_correlation_columns`; 880 `invalid_rows_are_refused_with_stable_codes` |
| `crates/shared/wyrd-queue/src/producer.rs` | 1201 `invalid_config_is_refused_before_any_task`; 1234 `a_thousand_tables_share_one_budget_and_idle_producers_hold_nothing`; 1267 `dropped_handle_keeps_retrying_its_retained_batch`; 1319 `ambiguous_sends_retry_one_identity_until_acked`; 1443 `flush_timeout_retains_batch_until_later_ack`; 1487 `failed_shutdown_keeps_serving_controls_and_retries`; 1632 `accepted_batch_losses_settle_exactly_once`; 1706 `arrow_batch_enqueue_seals_nested_columns_through_the_retry_owner`; 1748 `arrow_batch_enqueue_never_waits_for_publication`; 1800 `stalled_sends_bound_concurrency_and_grow_the_next_batch`; 1859 `prepared_rows_reject_atomically_before_reservation`; 1969 `multi_row_admission_is_all_or_none`; 2021 `byte_and_linger_triggers_seal_without_a_flush`; 2071 `admitted_rows_cannot_take_the_sealing_headroom` |
| `crates/shared/wyrd-runtime/src/permission.rs` | 1634 `scoped_permission_json_matches_the_approved_projection`; 1679 `bifrost_scope_applies_to_record_write_and_query_read_only`; 1736 `bifrost_scope_is_rejected_on_an_unrelated_resource`; 1755 `bifrost_scope_is_rejected_on_a_non_read_action`; 1782 `malformed_scope_identity_is_rejected_at_decode` |
| `crates/vala/vala-bifrost-redux/src/catalog/table_ref.rs` | 107 `new_and_fqn_round_trip`; 114 `parse_fqn_accepts_known_namespace`; 129 `canonical_catalog_has_no_alias`; 155 `parse_fqn_rejects_path_traversal` |
| `crates/vala/vala-bifrost-redux/src/catalog/tenant_table.rs` | 156 `from_facts`; 275 `physical_binding_facts_match_materialized_capacities`; 307 `tenant_table_binding_uses_authenticated_organization`; 411 `tenant_table_binding_rejects_tenant_in_logical_name` |
| `crates/vala/vala-bifrost-redux/src/forge/identity.rs` | 46 `durable_identity_conversion_fails_closed` |
| `crates/vala/vala-bifrost-redux/src/gate/error.rs` | 632 `ingest_busy_grpc_retry_metadata`; 659 `scribe_errors_project_through_gate_without_fallback_identity` |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs` | 1695 `ingest_frame`; 2114 `gate_authorizes_record_writes_against_the_resolved_table_uid` |
| `crates/vala/vala-bifrost-redux/src/namespaces.rs` | 177 `bifrost_namespace_datasets_variant_round_trips` |
| `crates/vala/vala-bifrost-redux/src/oracle/follower.rs` | 1586 `admitted_session_shape_contract` |
| `crates/vala/vala-bifrost-redux/src/oracle/mod.rs` | 4994 `query_floor_extracts_joined_and_cte_tables` |
| `crates/vala/vala-bifrost-redux/src/oracle/planner.rs` | 275 `quoted_dotted_identifier_resolves_alongside_canonical_name` |
| `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs` | 197 `writer_recipe_encoding_contract`; 305 `writer_recipe_blooms_exactly_the_resolved_union` |
| `crates/vala/vala-bifrost-redux/src/scan_sizing.rs` | 675 `scan_sizer_counts_uncompressed_covered_kept_chunks` |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs` | 1423 `decoded_size_frame` |
| `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs` | 2001 `parquet_writer_writes_bloom_and_page_index` |
| `crates/vala/vala-bifrost-redux/src/scribe/promotion.rs` | 442 `record` |
| `crates/vala/vala-bifrost-redux/src/scribe/seal_key.rs` | 460 `seal_key_canonical_key` |
| `crates/vala/vala-bifrost-redux/src/scribe/staging.rs` | 1412 `publication_manifest_recovers_rows_and_source_fence_fail_closed` |
| `crates/vala/vala-bifrost-redux/src/scribe/tests/scribe_persistence_path.rs` | 417 `production_shard_snapshot_serves_exact_projection_and_lsn_range`; 717 `shard_wal_failure_reaches_the_durable_completion` |
| `crates/vala/vala-bifrost-redux/src/scribe/wal.rs` | 4035 `slice_payload_decoder_rejects_invalid_table_identity` |
| `crates/vala/vala-sql/tests/integration/pg_file_list_cluster_nodes.rs` | 165 `vala_file_list_rejects_duplicate_stream_range`; 306 `vala_file_list_rls_blocks_cross_tenant_select`; 360 `vala_operator_pool_can_insert_and_select_file_list` |
| `crates/vala/vala-sql/tests/integration/pg_olap_catalog.rs` | 28 `active_table_inventory_is_operator_visible_ordered_and_status_filtered` |
| `crates/wyrd-spec/src/auth/permission_scope.rs` | 331 `scope_json_matches_the_approved_projection`; 357 `malformed_identities_are_rejected`; 436 `malformed_table_identity_is_rejected_at_decode` |
| `crates/wyrd-spec/src/vala/api.rs` | 2770 `bifrost_wire_field_spec_carries_correlation_metadata`; 2789 `bifrost_wire_register_request_omits_physical_layout`; 2805 `bifrost_wire_register_request_keeps_explicit_empty_layout_lists` |
| `crates/wyrd-spec/src/vala/audit_detail.rs` | 1089 `all_detail_variants_round_trip`; 1293 `serde_cannot_bypass_secret_classification`; 1379 `audit_event_golden_shape_distinguishes_absent_and_present_detail` |
| `crates/wyrd/wyrd-auth/src/issuance.rs` | 1139 `decodes_scoped_bifrost_grants_and_rejects_unscoped_rows` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs` | 500 `mcp_rejects_credentials_and_joins_request_and_process_cancellation` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/discovery.rs` | 266 `execute_sql_example` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs` | 519 `agent_debugs_otel_error_trace_through_mcp`; 740 `agent_reads_canonical_trace_genai_logs_and_metrics_through_sql` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs` | 263 `describe_evidence`; 307 `correlate_trace`; 322 `follow_subject_and_results`; 355 `read_custom_data` |
| `crates/wyrd/wyrd-server/src/bifrost/convert.rs` | 30 `bifrost_tables_namespace_parse_rejects_unknown` |
| `crates/wyrd/wyrd-server/src/bifrost/service.rs` | 702 `bifrost_tables_register_reserved_namespace_is_rejected`; 716 `bifrost_tables_list_and_describe_reflect_registration`; 791 `bifrost_tables_list_requires_read_permission`; 808 `bifrost_tables_describe_requires_read_permission` |
| `crates/wyrd/wyrd-server/src/http/otlp.rs` | 628 `writer_busy_maps_to_429`; 641 `writer_closed_table_name_is_per_signal`; 762 `table_not_found_maps_to_404`; 775 `decode_error_maps_to_400_with_signal_table_context`; 800 `metrics_decode_error_names_metrics_table`; 813 `logs_decode_error_names_logs_table` |
| `crates/wyrd/wyrd-server/src/oracle/forwarding.rs` | 883 `query_input` |
| `crates/wyrd/wyrd-server/src/oracle/peer_authority.rs` | 1032 `forward_claims` |
| `crates/wyrd/wyrd-server/src/query/floor.rs` | 337 `query_floor_accepts_select_with_allowlisted_functions`; 343 `query_floor_rejects_non_select`; 364 `query_floor_rejects_disallowed_function`; 378 `query_referenced_tables_extracts_relation_names` |
| `crates/wyrd/wyrd-server/src/verification/drift.rs` | 1426 `one_statement_decides_completeness_and_scores_from_one_cut` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs` | 939 `migrations_apply_and_are_idempotent` |
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs` | 3945 `expect_physical_layout` |
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs` | 1475 `prove_window_blocked_scribe_stop` |
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs` | 243 `prove_cancelled_read_terminates`; 853 `prove_variant_sql_sessions`; 1045 `prove_late_failures` |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs` | 148 `scribe_wal_fault_is_role_local`; 289 `failed_wal_replay_is_role_local` |


**Focused closure proof:** compare the remediation diff to these item inventories and inspect the rustdoc on every changed item, including fields and test helpers; ensure fallible items describe their real errors and panic sections describe their real invariant failures. Run format and the already-required narrow lint/documentation checks for the documentation write set. Do not add runtime tests merely to prove prose, suppress the gate, or rerun broad journey suites for this documentation-only correction.

## Verification provenance and limits

Independently run: immutable Git identity/status checks and `git diff --check ca2ac4a37...d1d02cced` (PASS); source/diff/declaration inspection across the surfaces above. Temporary in-memory source inspection was used to locate changed functions and missing documentation sections; multiline attributes and false `Result` substring candidates were checked against source before retaining findings. No script, index, patch or source archive was added.

Supplied evidence only: TASK-003 Implementation Evidence records PASS for format/lints, codegen, dependency and tenant isolation checks, docs, Python formatting/lints/typecheck, TS formatting/lints/typecheck/N-API parity and focused journeys. These were not rerun by this reviewer. No Cargo-backed command or broad test suite was run; the shared-checkout sequential-build rule was preserved. Source evidence, rather than recorded green checks, establishes the mandatory rustdoc failure and the production SQL capability assessment.

No behavioral, security or durability PASS is inferred from this standards report. No additional material repository-rule finding is proposed. The candidate is immutable and all applicable nested authorities were available; this is FAIL for a proven mandatory gate, not BLOCKED for missing authority.
