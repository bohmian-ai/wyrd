# TASK-008-R1 independent maintainer review

**Result: PASS. Proposed material findings: none.**

## Immutable subject and limits

Candidate `23eafa368bca19208faf8311eb7b5421e3660b38`; R1 parent
`6e7add054e33701ca5ecb52a5c859948b15161a3`; TASK-008 base
`f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; cumulative TASK-007 base
`a7582db587c6170a290760f1741673125612b797`.

Reviewed original TASK-007 and TASK-008, approved spec revision 20
(REQ-014/015, AC-016/017), their remediation packets, and the cumulative
changed surfaces. Unrelated TASK-006 benchmark and lifecycle changes remain
outside this audit. Maintainer decisions on FIND-007-3, Postgres tenant columns
and `WYRD_VALA_500_QUERY_TENANT_INVARIANT` are preserved.

This is a static maintainer audit. I ran no tests, Postgres wrapper, full mise
lane or commit and changed no source. The implementation evidence and
`task-008-review/final-named.log` were inspected as recorded evidence; they are
not represented as execution by this reviewer. Source inspection establishes
R1's identity-preserving imports and documentation correction directly.

Authority applied: AGENTS.md §§3–6, 11–12 and 16; agent-rules' import,
ownership, documentation and verification requirements; maintainer-style
(layout, owner/method shape, function shape, tests and documentation);
spec-driven-development evidence precision; Wyrd design's Bifrost ownership;
Bifrost design's table identity, live-tail, query and audit boundaries; routed
Rust-core, DataFusion and OLAP guidance. No CodeGraph index is present.

## Changed-surface coverage

| Changed surface | Owners, callers and relevant proof inspected | Assessment |
|---|---|---|
| Contract/envelope deletion | `wyrd-spec/src/vala/{managed_columns,mod,audit_detail}.rs`; both generated `bifrost_audit_event.json` copies; redux `contracts.rs`, `schema/managed_columns.rs`, `tables/{managed_columns,mod}.rs`, gateway fixture and result-feature/item documentation | Deleted constant, exports, envelope and schema declarations agree. `TenantFile` agrees with generated `tenant_file`; retired field IDs remain documented. No new SDK implementation or language declaration is introduced. |
| Ingest and persistence composition | `scribe/execution_lanes.rs` decode, source fingerprint, managed-column append and reserved-input tests; `persistence.rs`, `tests/scribe_persistence_path.rs` fixtures | The existing envelope owners lose tenant stamping. Principal, request, timestamp and batch assertions remain meaningful. No replacement authoring or persistence mechanism is added. |
| Footer identity | Complete `parquet/footer.rs`, `BifrostFooterIdentity::{new,key_values,verify}`, `tenant_key_value`, `verify_footer_tenant`, `footer_value` and footer tests | One typed tenant field extends the existing identity. Duplicate/missing-key parsing is reused. Pure deterministic helpers do not need an artificial service owner. |
| Staged/hot writer and assembly | `parquet_writer.rs` `ParquetBatchEncoder`, `ArtifactPlan`, `RollingArtifactWriter`, encode and sealed inspection; `member_stager.rs`; `ClaimAssembler::encode`; `ScribeStagingRuntime::assemble` | Tenant travels from the binding through existing request/writer values. Sorting, rolling and artifact evidence retain their existing owners. Removed row stamping does not survive behind another wrapper. |
| Forge writer configuration | `ForgeManagedRewrite::{plan,rewrite_plan}` calls to `ForgeTablePolicy::to_core_config`; `parquet/writer_properties.rs` rewrite recipe and test | Both rewrite callers supply `self.binding.tenant` to existing writer properties. The recipe documents origin and output footer behavior; R1 uses a module import for its parameter. |
| Published/hot/staged reads | `PublishedFooterLoader::load`, `verify_scanned_footer_tenant`, `tenant_proven_reader_metadata`, `OracleIcebergScanExec`, `HotParquetExec`, `hot_stream`, `hot_metadata_key`; `ObjectMetadataKey::tenant_id`; footer and hot refusal tests | The scan owns IO, cache and lifetime; the pure footer proof runs before decoding. Reader conversion and refusal errors have substantive documentation. New identity types are visible in imports. Published encrypted-file refusal remains explicit. |
| Projection and source construction | `OracleScanProjection`, catalog provider constructors and `provider_error`, signed follower closure/preflight, count fallback and distributed selective-query fixture | Projection remains named and schema-bound. The existing Iceberg provider replaces the deleted wrapper. No tenant filter or compatibility codec remains. |
| Cumulative live source | `ScribeTailResolver` constructors, complete `live_leaf`, resolver call path; `FetchLiveTailService`/`LiveTailBatches::into_parts`; `HotParquetExec::with_staged_lease`; projection, pruning, lease and partition proofs | Existing memory and hot-file sources own the work. Exactly the session's number of memory groups, including empty groups, is easy to follow. Lease transfer and retention are documented at the existing owners. Deleted `staged_tail.rs` has no surviving second decoder caller. |
| Cumulative native completion | `LiveFrame`, `NativeCompletion`, `NativeOutputTally`; server `ScribeFragmentExecutor::execute`, wire sibling; complete `LiveFrameDecoder::accept` and counter helpers; native completion/frame tests | A small stateful tally and value envelope keep counters and fingerprint discoverable. The decoder owns reconciliation and terminal ordering. Remote encoding retains its existing owner. No in-process IPC or hashing was added. |
| Session execution and routing | `ScribeResources::follower_execution`, peer executor caller and shape proof; `LiveScribeExec` constructor, fragment and execute methods | Partition count derives from the existing local resource plan, rather than a second setting or route count. Runtime/cancellation responsibilities stay with the established execution owner. |
| Audit and server composition | `Oracle::audit_tenant_refusal`, both execution callers and tenant error classification; codec/follower collaborator deletions; analytical/transport/query-stream/pruning fallout; server `boot/mod.rs` and Scribe build/shutdown in `state.rs` | Authenticated leader context owns refusal audit. Dead follower query-audit dependency and lifecycle fields disappear together. Peer security audit remains with its separate owner. No new publisher or audit mechanism appears. |
| Tests and evidence | Changed footer/writer/schema tests, follower pruning/projection/lease/partitions, native decoder tests; distributed refusal journey and support injector; retained peer-service proof names; integrated exact-proof table and log | Outcomes and negative branches remain visible. R1 edits no assertion or test semantics. Recipes identify package, feature, target and exact name, with setup for journey targets. Current runs are distinguished from earlier attribution. |
| Architecture and docs | Bifrost design tenant changes; routed DataFusion/OLAP edits; `docs/src/content/docs/bifrost/{architecture,data-plane-internals,forge,index,quickstart,schema,writing-data}.svx` | The changed tenant descriptions and envelope tables describe file proof and remove row filtering. No changed documentation asks maintainers to recreate the deleted mechanism. |
| Manifests and module registration | Cumulative workspace/redux manifest and lock addition for the already present async OpenDAL filesystem backend; `lib.rs`, `scribe/mod.rs` and deleted provider registration | The dependency remains in the scan-owning crate and pinned native dependency universe. No additional framework, feature or test harness is introduced by R1. |

## Prior findings and R1 regression check

| Prior ID | Current source/evidence | Maintainer closure |
|---|---|---|
| FIND-007-4 | `exec.rs` imports `DataTenantId`, `ArrowReaderMetadata`, and test `WriterProperties`; footer helpers, `hot_metadata_key` and fixture return use bare names. `claim_assembly.rs`, `forge/managed/policy.rs` and `parquet/writer_properties.rs` import and use `DataTenantId`. | **Closed.** These are the same concrete types and feature scopes. The test import removed from claim assembly is supplied by its existing `use super::*`. Qualified constructor expressions remain permitted. |
| FIND-007-5 | TASK-008 integrated exact-proof section lists individual redux names, three peer-service names, and each journey's target/name with complete common command templates; `final-named.log` records one selected passing test for each. TASK-007 retained recipes preserve honest historical/reconstructed/deferred attribution. | **Closed for maintainer reproducibility.** An implementer can select the recorded proof without guessing target or feature. No production or harness change is needed for this evidence obligation. |
| FIND-007-6 | `schema/managed_columns.rs:46` documents the renamed test's panic conditions: exact envelope field order/absence and principal/request nullability. The body still asserts the full list and required fields. | **Closed.** Documentation explains the actual assertions; no assertion was weakened. |

R1's Rust changes are imports, equivalent declaration spellings and one
rustdoc addition. They introduce no control-flow, state, ownership, transport,
error, feature or test-assertion change. Its remaining diff adds proof recipes
and recorded outcomes. I found no correction-induced regression or material
maintainer issue in the cumulative authorized surfaces.

## Findings and calibration

Validated by this discovery scope: **no proposed material finding**. No
personal preferences or unrelated old inconsistencies are promoted into
blocking findings. This result does not replace independent behavior,
standards, resilience or domain audits, or the final validation ledger.
