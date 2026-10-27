---
id: TASK-003-R1
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
status: implemented
kind: remediation
original_task: changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md
reviewed_base: ca2ac4a37e229a8f95667c073db11c70ad7ecb8b
reviewed_candidate: d1d02cced07582a8f817d3f5651e7895b96d046d
findings: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6]
---

# Preserve one accepted origin across admission, capture and delivery

Route directly to `$wyrd-implement`. This task corrects independently validated TASK-003 failures; it does not authorize an unrelated ownership redesign. Source was not changed during review.

Approved spec: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, with its auth, telemetry and storage supplements. Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md`. Prior verdict and independent ledger are `verdict.md` and `findings-validation.md` beside this task. The next review retains those artifacts and the cumulative base/candidate context, and audits remediation closure plus affected callers.

## Intended outcome and user clarification

An accepted action retains one truthful execution origin across Bifrost, gateway, verification, batching, retry and peer delivery. The shared ownership selection happens once at action admission, using existing authoritative logic. Downstream hashing, projection and storage consume accepted values. The account identity never substitutes for the exact versioned Service Card UID.

For stable account P running checkout v1 with UID S1, store `workload_card_uid=S1`; the same credential running v2 with UID S2 stores S2 after valid selection. A signed/scoped S1 context cannot claim S2. `sub_principal_id` and `act_principal_id` remain account identities; `subject_card_uid` identifies the observed Card, which may differ from the Service. No downstream worker, provider, Verifier or current credential may relabel the accepted execution. Rootless supported operations retain null workload rather than inventing one.

The user explicitly requested this cross-surface invariant during review. It clarifies existing approved semantics and receives no extra retroactive finding ID. The selected correction must fix the shared acceptance/handoff cause, not replace three downstream reads separately. Immutable descriptor projection from accepted UIDs may occur later, but cannot select a new owner or fail solely because an accepted Card was soft-deleted.

## Root cause and shared ownership flow

The source exposes two distinct concepts through interchangeable downstream inputs: verified authentication facts (`Principal` plus delegation chain) and an already accepted action's execution ownership. `owning_root` already implements correct selection, but Bifrost's frame/descriptor/hash stages can independently read raw `principal.workload_uid`. The gateway correctly admits a `GatewayCallContext`, then its sibling span projection does not consume it. Tests observing only calls and replay cases that also change descriptive payload columns cannot detect these omissions.

The user's explicit cross-surface clarification is consistent with existing approved semantics and is recorded as user direction, **not an additional independently discovered FIND**. The minimum coherent flow is:

| Stage | Existing owner and invariant |
| --- | --- |
| Verify credentials and select/validate any requested root | Existing auth/root-selection owners retain subject principal, current actor chain, effective permissions and signed graph. They do not replace account identity with execution identity. |
| Admit a new action | Use shared `wyrd_runtime::owning_root` once when deriving public execution ownership. Validate observed subject/binding under existing graph/binding owners; freeze workload/subject/Run/environment/request/account context on that action's existing accepted representation. Each distinct legitimate action still has its own admission. |
| Gateway | Existing `GatewayInvocation::admit_call` owns `AdmittedCall`/`GatewayCallContext`; requested public actions derive owner through the shared selector, and internal `Resolved` contexts retain their accepted source. Calls and every attempt consume that same retained context. |
| Verification | Existing direct/submitted acceptance and `AcceptedAuthority` preserve caller authority; `ResultRun`/`ClaimedExecution` retain validated exact binding owner, subject and environment. Queued runner gateway and results use the accepted binding owner (authority root only where already specified), independently of SYSTEM executor. Reusing `AcceptedAuthority::owning_root` delegates to the same selector; it does not justify looking up a latest root or reselecting from worker credentials. |
| Bifrost | Existing Gate accepted ingress context carries the one selected workload to OTLP identity, descriptors and `ScribeIngressFrame`; Scribe consumes it. Native replay binds stable accepted managed origin using the same durable identity owner. |
| Describe and deliver | Existing CardRegistry projects immutable descriptors from the accepted tenant/UIDs, including retained tombstones when necessary. This lookup does not decide ownership. Rows preserves each original context through mixed batching, local/peer delivery and retry. No frame-wide worker origin can replace a row's origin. |

No all-purpose context god object, extra graph engine, cross-store join, token descriptor fields or new public headers follow from this clarification. Extend the current accepted handoffs only where raw auth can be reinterpreted or a sibling projection omits fields. Keep precise binding ownership and public scope validation intact. The distinct lifetimes of authority acceptance and immutable descriptor projection explain why retained descriptor lookup can be delivery-time work without allowing delivery-time ownership selection.


## Validated diagnoses and selected corrections

### FIND-TASK-003-1 — Gateway attempt spans omit admitted origin

**Authority and materiality.** REQ-022/028/034/051–052/055–056; TASK-003 scenarios 5 and 7; storage supplement “Original action, not delivery identity” and “Shared Service and subject dimensions”; Bifrost row-attribution contract. Provider attempts are explicitly in scope. An ordinary configured call must remain queryable by its exact workload, subject, Run, environment, original request and account pair. Trace linkage alone does not replace these independent columns.

**Exact locations.** `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:403` (`otlp_spans`), `:438` (`spans_batch`), `:482` (`project`); downstream `src/scribe_outbox.rs:1089` (`Chunk::attributed`). Relevant trusted-admission seam: `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:564` (`decode_rows`) and `:945` (`enforce_builtin_source_contract`), with signal shared promotion validation at `tables/signal.rs:1060` and `:1246`.

**Reachable proof.** `GatewayInvocation::admit_call` retains a `GatewayCallContext`; `facts` and `CallCapture::from_facts` retain it after buffered/streamed settlement. `calls_batch` consumes its workload/subject/Run/environment. Sibling `otlp_spans` consumes only attempt fields and trace parent. `spans_batch` combines these under synthetic gateway resource labels, with no accepted context. `project` repeats only account attribution. `Chunk::attributed` carries the request specially for Calls; it supplies none for Spans. Local and peer `ScribeBatch::into_frame` can resolve only UIDs present in IPC. `managed_arrays` therefore stamps null workload/subject/Run and the delivery request; `stamp_dimensions` retains gateway service_name and null environment. WAL/recovery preserve this already-wrong output. Public routes, Batch HTTP operations, streaming and direct/queued verification converge here; separate ServerSpans shares `project_spans` but has no gateway context to infer.

**Consequence.** A correctly attributed call summary has provider attempts missing from the corresponding Service/subject/Run/environment queries and bearing a different request correlator. Mixed captures cannot recover those origins later. This is normal supported use, not hypothetical corruption.

**Minimum decision-complete correction.** Keep `CallCapture` as producer owner. Project each attempt with its capture's existing accepted workload, subject, Run, environment and original request, repeated in capture/attempt order. Use the existing row-carried attribution and registry descriptor stamping; leave descriptors to that owner. Preserve raw gateway resource labels, provider/GenAI values, attempt identity, span parentage and account pair. Do not add Card OTLP attributes to the projector with an absent authorized scope, or infer context after combining rows.

The correction must close the actual **trusted Rows** admission seam as well: carrying `wyrd_request_id` currently meets `enforce_builtin_source_contract`'s unconditional canonical `wyrd_` rejection, and a carried accepted environment differs from the resource-derived environment that public native validation expects. Distinguish trusted carried origin from public source-derived values using the existing `StampedAttribution::Rows` boundary. Accept its already-authorized managed request/origin, while retaining shape, Variant, correlation grammar/scope and non-attribution signal validation. Public Frame native writers must retain all reserved-field/forged-derivation refusals. Reuse the existing declared-Variant and canonical/table validation owners; do not weaken every native validator or trust arbitrary IPC. This is necessary completion of the same producer correction, not a seventh finding. ServerSpans remains on its existing behavior.

**Burden and focused closure.** A bounded producer/accepted-row correction; a query join, new writer, provider payload extension or generic inference service is larger and cannot repair stored origin. Extend `gateway::compatible::configured_gateway_headers_are_authorized_and_captured` to read actual attempt spans as well as summaries. Two captures with different origins and multiple attempts must retain exact workload/subject/Run/environment/request/pair and unchanged raw/trace/provider data. Use one existing streaming or verification-backed path. A focused Rows decode assertion must carry accepted environment and request successfully while its public Frame counterpart refuses forged data. Reuse existing peer retry/origin seams; no new lifecycle suite is required.

### FIND-TASK-003-2 — Public native drift input can forge shared dimensions

**Authority and materiality.** REQ-024/051–052, AC-007/024, TASK-003 scenario 4 and Bifrost's public pre-ACK refusal rule. Public write permission does not authorize invented canonical Service/environment fields. Telemetry's resource fallback is an explicit exception with source validation; drift rows have no such source.

**Exact locations.** `crates/vala/vala-bifrost-redux/src/tables/drift/observations.rs:26` (`arrow_fields`); inherited `tables/mod.rs:388` (`validate_predeclared`); accepting boundary `scribe/execution_lanes.rs:564` (`decode_rows`), retaining sink `:1240` (`stamp_dimensions`, especially `:1278`).

**Reachable proof.** Gate reserves audit/gateway/verification outputs, but permits drift observations under the normal table-write grant. A public native caller supplies the described observation schema with `deployment_environment='prod'` and no request environment; a rootless caller may also supply arbitrary Service name/version. Only the seven managed fields and `wyrd_` names are reserved. `validate_predeclared` checks shape, Variant and whole Structs, not dimension source. Stamping retains observed Service when no workload descriptor exists and retains environment when accepted environment is absent. The forged values ACK and persist; subject descriptors are overwritten silently. The normal SDK Drift producer already supplies null placeholders. Trusted Rows legitimately carry accepted non-null environment; public signals legitimately carry source-validated resource promotions; dynamic tables may own similarly named fields.

**Minimum decision-complete correction.** At the existing **public native Frame** admission boundary, refuse any non-null shared dimension on the non-signal built-in input path before stamping. Then derive its values from accepted context. Use the existing builtin definition/canonical distinction and existing refusal mechanism; no new table policy, error family or registry query. Do not install an unconditional null rule in `validate_predeclared`: trusted outbox/peer Rows use it too. Do not change telemetry resource fallback or dynamic user fields. A generic stamp fallback deletion would break approved telemetry and trusted origins.

**Burden and closure.** One bounded boundary rule and negative proof in `verification_runtime::native_signal_values_and_attribution_are_validated_before_ack` or an adjacent case in that existing server binary. Mutate each of the six dimensions on a valid drift batch, with rootless and valid-root context; assert whole-batch refusal/no rows, including environment without metadata. Null placeholders plus accepted environment must persist correctly. Retain positive trusted mixed-origin and native/OTLP resource-fallback checks. No SDK redesign or broad transport rerun is required.

### FIND-TASK-003-3 — Bifrost uses auth subject root rather than accepted acting owner

**Authority and materiality.** REQ-014/022/024/034, INV-003–004 and the auth supplement “User A calls Service B, and further delegation.” Current execution ownership is the outer actor's validated root; the account subject, actor and observed Card remain independent. One ordinary valid delegated export already violates ownership. Identical exports from two releases of one actor account may also collapse incorrectly.

**Exact locations.** `crates/vala/vala-bifrost-redux/src/gate/attribution.rs:90` (`frame_dimensions`), `gate/mod.rs:992` (`dispatch_canonical` batch context), `scribe/ingress.rs:728` (`prepare_and_dispatch_under` workload source). Existing selecting owner: `crates/shared/wyrd-runtime/src/request_context.rs:68` (`owning_root`).

**Reachable proof.** `DelegateToken::execute` retains `subject_workload_uid` separately from `actor_workload_uid`. Issuance puts subject root on the top-level principal and actor root in outer `act`. `AccessTokenClaims::into_verified` preserves that distinction; `flatten_act_chain` orders the current actor last. `owning_root` correctly selects that last actor's root. HTTP `caller_auth_context` and gRPC `AuthContext::from_verified` preserve both verified facts. Yet the three new Bifrost consumers read `principal.workload_uid`. Thus User A delegated to Service B stamps null workload, or a rooted A delegated to B stamps A's root. The descriptor and OTLP hash inputs follow the same wrong selection. Gateway public admission already uses `owning_root`, demonstrating cross-surface disagreement under the same accepted authority.

**Consequence.** Required exact Service ownership is absent/wrong, resource labels may fill Service fields despite validated actor ownership, and identical OTLP exports for two roots of the same actor account can share a fence because subject/actor IDs, top-level root and payload match. No malicious token, new root selector or tenant bypass is necessary.

**Minimum decision-complete correction.** Resolve the public action's accepted owner **once**, at the shared verified-to-accepted ingress boundary, using existing `wyrd_runtime::owning_root`. Carry the resulting immutable workload with the accepted frame/context to batch identity, descriptor projection and Scribe stamping. Those consumers must not reselect ownership from raw auth facts. Preserve the top-level principal and chain, operation permissions, graph-scope intersection, tenant, account pair and observed subject/Run. Keep trusted Rows' carried workload, and verification's explicit validated binding owner, independent of delivery authority. Do not implement three local `owning_root` replacements or overwrite the principal to impersonate the actor. No new ownership engine or JWT claims are needed.

**Burden and closure.** Small accepted-context closure using the existing selector and handoff; correcting only one downstream consumer leaves others wrong. Extend the existing delegated OTLP journey (`negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution`, or its same-owner neighbor) with one User and one Service account running two exact roots. Identical HTTP and gRPC exports must retain two row sets with the correct actor workload/Service descriptors and original A/B pair; exact retries remain one set each. Add native readback under delegated actor root. A rooted subject distinct from actor root proves selection rather than merely null filling. Existing tests varying actor accounts do not prove this case.

### FIND-TASK-003-4 — Native replay identity omits stable accepted managed origin

**Authority and materiality.** REQ-024/028/034, TASK-003 scenario 4 changed-owner batch replay, and the storage/telemetry exact-retry contracts. The defect concerns deliberate reuse of an explicit accepted batch ID with changed context, not a speculative random UUID collision.

**Exact locations.** `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:629` (`logical_data_identity`), `:563` (`LogicalIdentity::for_rows`), `:916` (`prepare_slice`); newly managed workload declaration at `tables/managed_columns.rs:101`. Existing correlation selector/digest at `tables/managed_columns.rs:134` and `scribe/preprocess.rs:500`. Downstream cache `scribe/shards.rs:1138`, durable comparison `crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:123`.

**Reachable proof.** Gate forwards caller batch UUID unchanged for native writes. Scribe stamps workload, subject, Run and account pair. Dynamic tables have no mandatory Service/environment dimensions, so changing valid workload A to B can change only managed fields. `logical_data_identity` removes every managed field, not just volatile request/time. Native never binds `correlation_data_identity`; only OTLP batch-ID derivation uses that digest. `prepare_slice` supplies the incomplete digest to WAL; `PreparedAppend::logical_identity` folds it with schema and slice ordinals. Both `committed_duplicate_rows` and SQL `matches_logical` therefore see a matching batch and ACK the changed origin while keeping only the first origin. A fresh request/time is intentionally irrelevant; the accepted workload and account pair are not volatile.

**Minimum decision-complete correction.** Bind stable managed correlations in the existing logical identity used by the complete slice/WAL/fence/cache path. Reuse the managed declaration's correlation/receipt distinction; exclude only genuinely volatile request/admission-time fields. Keep the client-supplied batch ID and existing conflict mechanism. Update `LogicalIdentity::for_rows` to compare exactly the same included inputs: otherwise a pre-stamp OTLP `LogicalIdentity` may be reused after managed origin changed and defeat the correction. Retain OTLP root/pair/environment identity separation and honest retry convergence. No second fence, pre-read, extra SQL, schema or persistence policy is needed.

**Burden and closure.** A hash-input and reuse-predicate correction at the existing identity owner; transport-specific checks miss durable/cold paths. Extend public native dynamic-table proof with fixed user rows/time, one explicit ID and valid origins. Exact retry succeeds once; changing workload, account pair, subject or Run refuses and keeps original rows. Cover workload conflict through both warm duplicate cache and cold/durable fence after existing restart/cache-reset support. A focused identity unit assertion should show request/time-only differences remain equal, stable-correlation changes differ, and reused identity equals recomputation after stamping. The current signal test changes non-managed dimensions too and does not expose this cause.

### FIND-TASK-003-5 — Cold trusted delivery loses an accepted deleted subject

**Authority and materiality.** REQ-034/051–052/055–056, TASK-003 scenarios 5/7 and storage supplement's retained original action. Accepted work cannot be relabeled or lost because the delivering process has different cache history. The allowed loss of an abruptly terminated bounded outbox does not authorize deterministic terminal refusal of a live, valid accepted action.

**Exact locations.** `crates/vala/vala-bifrost-redux/src/gate/attribution.rs:113` (`row_dimensions`), `:140` (`described`), `:179` (`read_descriptor`); `crates/wyrd/wyrd-server/src/scribe_outbox.rs:490` (`into_frame`). Existing SQL visibility owners: `crates/wyrd/wyrd-sql/src/queries/cards/get.rs:12` and `:82` (`get_card_for_reconciliation`).

**Reachable proof.** A rootless permitted caller invokes a gateway operation about a standalone registered observation-target Card. `GatewayInvocation::attribute` authorizes with a direct live SQL read, without warming `CardRegistry`. An authorized deletion may soft-delete this unreferenced Card while provider/stream work is in flight; `soft_delete_card_with_expected_kind` permits active Cards without incoming visible relationships. Later cold local/peer `row_dimensions` calls the live `get_card_by_uid`, whose SQL excludes `status='deleted'`. The descriptor is omitted. `stamp_dimensions` raises `CardScopeDenied` for the carried subject, `ScribeRefusal::from_scribe` maps it to Rejected, and `ScribeSink::tally` drops the completed action's writes. A warm descriptor cache instead succeeds. Tenant UID/name/kind/version remain in the retained tombstone; there is no required rename/update to reconstruct.

**Minimum decision-complete correction.** Keep public membership fresh/live and retain the existing registry descriptor owner/cache. Resolve **already accepted trusted** immutable UID descriptors using the SQL Card owner's existing retained-row lookup (`get_card_for_reconciliation`), rather than the public visibility lookup. Route only descriptor restoration through retained rows. In particular, `observation_targets` currently shares `read_descriptor`; changing that shared live read indiscriminately would authorize new rootless public writes about deleted Cards and is invalid. No new cache, delivery retry classification, queue persistence, lock or graph owner is justified. Descriptor restoration is projection of frozen identity, not fresh execution-ownership selection.

**Burden and closure.** Bounded separation of descriptor availability from public membership; retrying a permanently invisible Card forever is not a correction. Admit a rootless call about a deletable subject, delete it before capture, then deliver via cold and warm resolvers and existing peer path. Both must retain the original UID/descriptors/account/request, and a new rootless public write about the deleted Card must refuse. One cold resolver assertion plus existing peer production seam is sufficient; no full outage or lifecycle suite.

### FIND-TASK-003-6 — Mandatory changed-item documentation gate fails

**Authority.** Root AGENTS.md explicitly blocks missing rustdoc on every new/changed Rust item; `agent_docs/rust.md:233` includes fields, private helpers, tests and test helpers and requires accurate Errors/Panics sections. The task-review skill forbids waiving this gate. Runtime probability/materiality balancing cannot waive it.

**Evidence and scope.** Candidate `ResourcePromotionColumns` has six new undocumented fields at `tables/signal.rs:508`; changed `ingest_capture` at `wyrd-server/src/grpc/capture_peer.rs:102` lacks Errors despite malformed frame/registry/Scribe failures; changed assertion-bearing tests and helpers lack Panics. The exact corrected inventory below is the correction boundary. Every included item intersects actual changed source; unchanged nearby debt is excluded. Functions with a purpose doc but missing a section need only that section. The duplicate purpose/panic entries describe one item, not separate findings.

**Minimum correction and burden.** Add accurate item-local workflow/invariant/error/panic docs to these changed items, without changing behavior, assertions, timing, test placement, generated artifacts or lint policy. Reuse existing vocabulary. Bundle with functional remediation; no independent style round, new lint exemption or runtime test is needed. Static item/diff inspection plus applicable existing formatting/lint checks closes this gate.

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
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs` | 243 `prove_cancelled_read_terminates`; 853 `prove_variant_sql_sessions` |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs` | 148 `scribe_wal_fault_is_role_local`; 289 `failed_wal_replay_is_role_local` |




## Constraints and preserved behavior

Preserve tenant binding, effective permissions, signed/delegated graph intersections, fresh public membership checks, binding occurrence ownership, account pair, observed Card, Run, environment precedence and exact UID identity. Each distinct action still has its own authorization/admission; one shared rule does not erase those boundaries. Queued verification retains accepted authority and binding owner independently of its SYSTEM executor. Mixed-origin rows retain separate origins inside a single-tenant frame.

Preserve all canonical signal fields, raw OTel labels/Variant values, strict GenAI/type/presence validation, precision, byte bounds, trace parenting, attempt order, Batch HTTP semantics and nonblocking audit/capture behavior. Public forged managed/derived fields still refuse before ACK. Trusted Rows may carry accepted origin only through the existing private boundary; this is not a new public trust mode. Keep honest retry convergence, WAL/fence atomicity and retained version selection.

No new writer, graph owner, global context object, per-row SQL, cache, JWT descriptor, readable identity header, public API, alias, historical data conversion, registry join, batch-item attribution protocol, evidence narrowing, trace-semantic change or performance claim. No broad repository refactor, new check script, dependency, test harness, weakened assertion, timeout change or lint suppression. Existing tombstone retrieval is projection-only and cannot authorize new public work. If a material approved boundary actually needs changing, stop for specification revision rather than expanding this task.

## Acceptance criteria

| Finding | Required observable closure |
|---|---|
| FIND-TASK-003-1 | Calls and every provider-attempt span retain their own accepted exact workload/subject/Run/environment/request/account pair for two mixed origins and multiple attempts, including an existing streaming or verification-backed path. Trusted Rows carry accepted request/environment successfully; public Frame forgery refuses. Raw/provider/trace values remain exact. |
| FIND-TASK-003-2 | Every non-null shared dimension supplied on public native drift input refuses whole before ACK with no rows. Null placeholders receive valid accepted context. Trusted Rows, telemetry resource fallback and dynamic similarly named fields retain their approved behavior. |
| FIND-TASK-003-3 | One shared accepted-owner selection supplies hashing, descriptors and stamping; no downstream reinterpretation of principal subject root. One stable Service account with two roots and one delegated User preserves both roots under identical HTTP/gRPC exports and native readback, including a differing rooted subject. Exact retries remain exactly once and graph/tenant refusals stay intact. |
| FIND-TASK-003-4 | An explicit native dynamic-table batch ID accepts exact replay once and refuses changed workload/account pair/subject/Run, with unchanged user payload. Warm cache and cold durable fence agree. Identity reuse after stamping equals recomputation; request/time-only variations do not create false conflicts. |
| FIND-TASK-003-5 | Cold and warm trusted descriptor delivery after accepted subject deletion agree on the original UID/descriptors. Existing peer production delivery preserves them. A new public attributed action about the deleted Card still refuses. |
| FIND-TASK-003-6 | The corrected changed-item inventory above has meaningful item docs and accurate Errors/Panics sections; every newly changed remediation item follows the same gate. No behavior or test weakening accompanies documentation. |

## Focused verification and handoff

Read `mise.toml` and `TESTING.md` before executing. Use existing binaries/fixtures and extend their narrow cases rather than adding a full suite. Confirm any added unit-test selector after addition and before RED; use exact nextest selectors and retain selected test counts, the failing assertion/trace and final green evidence. Run Cargo commands sequentially. Existing lifecycle acceptance carries forward; do not rerun full journey suites, `verify:bifrost`, or `gate` for this remediation.

The following existing targeted cases can own the new assertions. Use the existing configured gateway case for attempt-span and deletion/delivery proof; use the existing native server case for drift rejection and dynamic replay proof; use the delegated OTLP case for shared owner selection. A small focused identity/Rows unit test covers hash reuse and trusted/public admission distinction. New test names remain implementer choices and must be recorded as exact commands in implementation evidence.

```bash
mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::configured_gateway_headers_are_authorized_and_captured)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::native_signal_values_and_attribution_are_validated_before_ack) | test(=verification_runtime::trusted_delivery_preserves_each_origin)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E 'test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)'"
```

Run the existing format/lint checks required by the actual write set (`mise run fmt`, `mise run lints` for Rust; owning SDK checks only if those sources change). Documentation correction itself needs static inventory inspection and formatting/lints, not new runtime tests. Regenerate and check consumed declarations only if the correction changes their sources; no new public contract is intended. Any focused positive regression added to protect a correction boundary should use the same existing harness and exact command rather than a broad capability lane.

Record a short shared ownership flow from each affected Bifrost/gateway/verification admission owner to its consumers, identifying where the shared selection happens and where accepted values are carried. This is evidence of root-cause closure, not permission to inventory or refactor unrelated authentication machinery. Include focused positive coverage for direct/queued verification through existing accepted-context seams where the write set touches them. Preserve all six stable finding IDs for remediation review.

## Implementation evidence

### Diagnoses

**`admission_clock` lane note.**
- Symptom: `admission_clock` failed when run bare.
- Evidence: the lane only runs under the Postgres wrapper.
- Cause: it was invoked without `with-test-postgres.sh`, so there was no database; the code is not defective.
- Fix site: none. It passes under its lane wrapper.

**`scribe::accepted_retry` fixture principal.**
- Symptom: three tests in `vala-bifrost-redux::integration` failed with `ContractViolation(TableVersionConflict)` after FIND-4 bound correlation columns into the logical identity:
  - `scribe::accepted_retry::pg_tests::a_retry_the_current_layout_refuses_is_acknowledged_under_its_own`
  - `scribe::accepted_retry::pg_tests::a_held_unclaimed_write_keeps_its_retained_input_charged_once`
  - `scribe::accepted_retry::pg_tests::an_unheard_edit_still_governs_new_admission`
- Evidence: the trace shows `sub_principal_id` and `act_principal_id` digests differing between the first attempt and its retry under one batch id. For example, batch `01a16f9a…` goes from `f6b29a21…` to `2dbef399…`.
- Cause: `tests/integration/forge/support.rs::ingest_at` minted `PrincipalId::new(Uuid::now_v7())` on every call. As a result, each "retry" came from a different account pair, which FIND-4 now correctly refuses.
- Fix site: `ingest_at`, which now uses one fixture writer per tenant (`PrincipalId::new(tenant.as_uuid())`).
  - Callers checked: `ingest_unclaimed` and `append_only`/`append_and_seal`. Both always use fresh batch ids, so only the stored principal value changes.
  - Other per-call principal builders (`forge_harness.rs::scribe_append_and_seal`, `oracle/peer_cluster.rs`, `oracle/capacity.rs`) never retry under one batch id.
  - No production change.
- Independent diagnostician report: it confirmed the same cause and fix site. It also noted that a changed origin under a reused batch id surfaces as `TableVersionConflict`. That refusal is the existing fence path and is out of scope here: the outcome is a refusal, as required.

### Acceptance criteria

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-1: calls and every attempt span keep their own accepted workload/subject/Run/environment/request/pair; trusted Rows carry request/environment; public Frame forgery refuses | `wyrd-server/src/components/gateway/capture.rs::CallCapture::spans_batch` overwrites the projected `deployment_environment`/`subject_card_uid`/`run_id` per attempt from the call's `GatewayCallContext`, and appends `workload_card_uid` and `wyrd_request_id`. `vala-bifrost-redux/src/scribe/execution_lanes.rs::enforce_builtin_source_contract` (`carried` = `StampedAttribution::Rows`) admits a carried `wyrd_request_id` and judges a canonical signal against `resource_environment`. | `gateway::compatible::configured_gateway_headers_are_authorized_and_captured`: two failover origins with two attempts each, plus the streamed Anthropic call, give five attempt spans. Each has origin = its call's, one trace per call, distinct span ids and the exact GenAI name. Rows unit test `scribe::execution_lanes::tests::row_attributed_spans_carry_their_accepted_environment_and_request`. RED: projector without context, then 0 spans; `carried=false`, then `InvalidFrame`. | PASS |
| FIND-TASK-003-2: a non-null shared dimension on a public native drift input refuses whole before ACK; null placeholders get the accepted context | `execution_lanes.rs::refuse_supplied_dimensions` is called from `decode_rows` for Frame attribution on non-signal built-ins | `verification_runtime::native_drift_dimensions_come_only_from_accepted_context`: six dimensions × (rootless, owned staging) all refuse; placeholders land with the accepted context. RED confirmed. | PASS |
| FIND-TASK-003-3: one shared accepted-owner selection feeds hashing, descriptors and stamping | `gate/auth.rs::AuthContext::workload` (`owning_root`) feeds `frame_dimensions`, the OTLP batch id and `ScribeIngressFrame.workload`; Scribe's `AdmittedRowContext.workload = frame.workload` | `otlp::negative::pg_tests::delegated_exports_are_owned_by_the_current_actors_root` covers two actor releases, a rooted subject and a User delegation, each exported over gRPC twice and HTTP once (one set each); native delegated readback runs in the FIND-2 journey. RED: the subject root gives `service_name` `otlp-owner-subject`. | PASS |
| FIND-TASK-003-4: replaying an explicit native batch id with a changed workload/pair/subject/Run refuses, warm and cold; request/time-only changes stay equal | `scribe/preprocess.rs`: `is_volatile_column`; `logical_data_identity` and `LogicalIdentity::for_rows` skip only volatile columns; `correlation_data_identity` is deleted and `otlp_batch_id` no longer folds it | `verification_runtime::native_batch_replay_is_bound_to_its_accepted_origin` covers warm and post-restart phases. Unit tests `logical_identity_ignores_server_receipt_columns_only` and `known_identity_is_reused_only_for_its_exact_inputs` pass. RED confirmed. | PASS |
| FIND-TASK-003-5: cold and warm trusted delivery after subject deletion keep the UID/descriptors; peer path preserved; a new public action refuses | `gate/attribution.rs`: `Lookup::{Live, Retained}`. `row_dimensions` reads `get_card_for_reconciliation`; `frame_dimensions` and `observation_targets` stay live | Configured gateway journey: two Cards (one warmed by a delivered capture, one never described) are held at the provider and deleted mid-call. Both captures keep their UID, kind, name, version, Run, account and request, and a new call about either refuses 403 `WYRD_VALA_403_BIFROST_CARD_SCOPE`. `peer::oracle_only_gateway_captures_through_the_peer_scribe` passes. RED (`row_dimensions` Live): the cold row never lands. | PASS |
| FIND-TASK-003-6: inventory items carry meaningful docs and accurate Errors/Panics sections; new remediation items follow the same rule | Doc-only edits to every inventory item; every remediation item carries rustdoc with `# Errors`/`# Panics` | Static inventory inspection; `mise run fmt`; `mise run lints` | PASS |

### Shared ownership flow

| Admission owner | Where the selection happens | Where accepted values are carried, and consumers |
|---|---|---|
| Bifrost Gate (HTTP/gRPC OTLP and native) | `AuthContext::workload` → `wyrd_runtime::owning_root(principal, chain)`, evaluated once per request | Read by `CardRegistry::frame_dimensions` (descriptors), by `otlp_batch_id` (identity) and by `ScribeIngressFrame.workload`. In Scribe ingress, `AdmittedRowContext.workload` stamps managed columns. No consumer reads `principal.workload_uid`. |
| Gateway | `GatewayInvocation::admit_call` produces `GatewayCallContext` (existing shared selector) | `CallCapture.subject` feeds `calls_batch` and `spans_batch`, with each attempt repeating its call's context and request. The outbox `Chunk::attributed` adds the pair. Rows decode accepts the carried workload/subject/Run/request/environment, and `row_dimensions` describes the carried UIDs (live or retained) for local and peer delivery. |
| Native replay | Scribe `logical_data_identity` over the stamped batch, excluding only volatile request/time | The same identity feeds the WAL, `committed_duplicate_rows` (warm) and `matches_logical` (durable). |
| Verification | Unchanged: `AcceptedAuthority` / `ResultRun` keep the binding owner | Results reach Bifrost through the same Rows boundary; the gate behavior they rely on is unchanged. |

### Commands

```bash
mise exec -- env WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::configured_gateway_headers_are_authorized_and_captured) | test(=peer::oracle_only_gateway_captures_through_the_peer_scribe) | test(=resilience::gateway_capacity_paces_streams_isolates_tenants_and_recovers) | test(=resilience::a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch) | test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run build:server && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::native_signal_values_and_attribution_are_validated_before_ack) | test(=verification_runtime::trusted_delivery_preserves_each_origin) | test(=verification_runtime::native_drift_dimensions_come_only_from_accepted_context) | test(=verification_runtime::native_batch_replay_is_bound_to_its_accepted_origin)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E 'test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution) | test(=negative::pg_tests::delegated_exports_are_owned_by_the_current_actors_root)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test scribe -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(/replay|retry|duplicate|fence|recover|idempot|identity/)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux -p wyrd-server --lib"
mise run fmt && mise run lints
git diff --check
```

Results:
- Gateway journeys: 5/5.
- Server journeys: 4/4.
- OTLP journeys: 2/2.
- Replay/retry integration: 20/20.
- `vala-bifrost-redux` + `wyrd-server` lib: 1431/1431.
- fmt, lints and `git diff --check` are clean.
- `codegen:check` is not needed: no contract, schema or generated source changed.

**Diagnosis: gateway span frames refused while FIND-1 was implemented.**
- Symptom: every `bifrost.traces.spans` frame was "rejected terminally", and the journey read 0 spans.
- Cause: the canonical span projection already carries `subject_card_uid` and `run_id` correlation columns (`tables/signal.rs::correlation_fields`). Appending them again produced duplicate names, which `decode_rows` refuses with `InvalidFrame`.
- Fix: in `spans_batch`, overwrite those projected columns and append only `workload_card_uid` and `wyrd_request_id`.

### Non-goals and limits

- No new writer, graph owner, context object, per-row SQL, cache, JWT descriptor, public header, API, alias, data conversion, registry join, check script, dependency, harness, timeout change, lint suppression or weakened assertion.
- ServerSpans behavior is unchanged.
- The public Frame still refuses carried `wyrd_request_id` and forged resource environments.
- Tombstone lookup projects descriptors only; public admission (`frame_dimensions`, `observation_targets`, gateway `is_observation_target`) stays live.
- FIND-6 is doc-only. Two existing test names promise more than their bodies check, and the docs describe the bodies:
  - `audit_detail.rs::all_detail_variants_round_trip` round-trips only `Ingest`;
  - `scribe/staging.rs::publication_manifest_recovers_rows_and_source_fence_fail_closed` checks no fence.
  Renaming them was out of scope.
- A changed origin under a reused native batch id refuses through the existing fence's `TableVersionConflict`. The refusal is correct, but its code describes the version path.
