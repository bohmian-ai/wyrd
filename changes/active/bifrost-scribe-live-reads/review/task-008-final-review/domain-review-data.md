# Persistent data and durability review

**Overall: PASS. Proposed findings: none.**

## Immutable subject and scope

Candidate `9c3d7ecb982435919924dfa8e6930352b27a9b7e`, parent
`23eafa368bca19208faf8311eb7b5421e3660b38`, cumulative TASK-007 base
`a7582db587c6170a290760f1741673125612b797`, TASK-008 base
`f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed the cumulative
TASK-007/008 persistent-data boundary and final delta, against approved spec
revision 20 REQ-014/015, AC-016/017 and INV-001/002/004/005/006.
TASK-006 work was examined only where publication/lifecycle, real-server
proof, or harness removal intersects those boundaries. The subject's explicit
maintainer decisions and exclusions stand.

This is an independent domain discovery report. It does not use another
current discovery report's conclusions. Prior verdict/remediation artifacts
were read as historical hypotheses, not as proof of current source.

## Authority and source coverage

Authorities: supplied AGENTS.md, architecture/agent-rules.md,
architecture/references/languages/spec-driven-development.md,
architecture/references/languages/maintainer-style.md, applicable Wyrd
protocol/doctrine, and architecture/bifrost-design.md table identity,
durability/visibility and Scribe/Oracle/Forge ownership contracts. No CodeGraph
index exists. No build or test was run.

| Boundary | Source traced | Assessment |
|---|---|---|
| Authenticated rows → WAL → visible ACK | scribe/ingress.rs preparation and actual-held-buffer charging; execution_lanes.rs native/managed projection; shards.rs `process_group` (3110), `sync_group` (4026), `insert_committed_group` (4057), `acknowledge_visible` (3185) | Tenant identity remains outside rows in the prepared append/seal key. WAL slice sync, COMMIT sync/fence, insertion, then ACK remain ordered. The cumulative shards runtime is unchanged; its delta is a test request shape. Removing row stamping does not move the durable fence. |
| Frozen generation → authenticated staged artifact | scribe/member_stager.rs `encode_runs` (160), parquet_writer.rs `ParquetBatchEncoder::prepare_sorted_candidate` (384), `ArtifactPlan` (271), `RollingArtifactWriter::seal_open_artifact` (591), `inspect_sealed_artifact` (739), parquet/footer.rs | Binding tenant, explicit seal tenant and frozen seal tenant must agree before encoding. Every sealed artifact receives schema/object/tenant metadata, is reopened to verify the exact identity, then receives a checksum. Fsync/preflight precede the durable ready record. |
| Staged member → assembled object → fenced publication | persistence.rs `persist_once` (1889), `stage_member` (1921), `publish_claim` (2103); staging_runtime.rs `register_member`, `assemble` (703), `publish` (739); claim_assembly.rs `gather` (131), `encode` (251) | Registered staging precedes manifest advancement/WAL retirement. Claim gather rereads the key-bound record and validates recorded run length/digest; assembly derives its tenant from the same context binding used for publication and proves the output row total. Failed assembly/publication retains durable members and does not retire their authority. |
| Recovery, live snapshots and publication overlap | staging_runtime.rs `restore_context` (621), `restore_authority`; tail_rpc.rs `open_live_batches` (562), tenant/table source selection; follower.rs `live_leaf` (779); exec.rs `hot_stream` (2883) | Recovery reconstructs the tenant-qualified context from the durable assembly key and checks schema/recipe identity. Live reads take the staged lease and exclude generations already served from memory; the shared scan retains that lease in plan/stream ownership. Publication can advance authority without deleting runs still leased by an open read. |
| Scribe-hot promotion and Forge rewritten outputs | forge/scribe_promotion.rs object-path/checksum/evidence verification; parquet/promoted_object.rs; forge/managed/executor.rs `plan` (182), `rewrite_plan` (271); managed/policy.rs `to_core_config` (254); parquet/writer_properties.rs `bifrost_rewrite_writer_properties` (119) | Promotion retains the exact Scribe object bytes rather than reencoding them. Both rewrite configuration callers pass their owning table binding's tenant; the installed writer properties stamp it on every rewritten output. Existing noncommitting rewrite/handoff and fenced publication remain the durable owner. |
| Published/hot/staged data → query results | exec.rs `PublishedFooterLoader::load` (994), encrypted task rejection (1429), `verify_scanned_footer_tenant` (1115), `tenant_proven_reader_metadata` (1141), shared `hot_stream` (2883); storage/cache.rs tenant-qualified metadata keys; follower.rs `live_leaf` | Every published metadata load, including a cache hit, proves the expected tenant before returning metadata. Hot/staged scans perform the same proof before reader construction and before pruning/decode. Missing, duplicate or foreign footer values produce QueryTenantInvariant. The authenticated assignment supplies the staged key tenant; local staged paths remain Scribe-owned. |
| Final scan-metric delta and adjacent TASK-006 changes | exec.rs `RemoteScanMetrics::record_footer` (292), query collector (445); live.rs `LiveFrameDecoder::accept` (777); final.diff and cumulative source | Metrics fold only after unchanged native count/fingerprint or wire count/digest/terminal validation. The added atomics do not mutate files, catalog state, row identity, ACK, leases or publication. Process harness removal deletes test scaffolding; real production callers above remain. Client HTTP default URL/schema changes alter connection configuration without changing these durable owners. |

## Obligation and proof assessment

| Obligation | Implementation evidence | Furnished verification evidence | Result |
|---|---|---|---|
| REQ-015/AC-017: tenant row column removed from managed storage; staged and assembled files carry authenticated footer tenant | Managed envelope/schema/projection deletions; writer identity verification; assembly context binding | Schema absence and generation/footer encoding tests; exact-tree oracle live/follower 19/19 and reported oracle journeys 40/40 | PASS |
| REQ-015/AC-017: Forge outputs remain tenant-bound; publication and rewrite preserve results | Both managed rewrite callers stamp binding tenant; promotion preserves bytes and validates Scribe evidence | Retained exact named promotion/rewrite journey evidence and reported current Bifrost journey results | PASS |
| REQ-015: missing/foreign footer refuses file before its rows; no compatibility fallback | Mandatory published loader and shared hot/staged tenant proof before reader/decode; strict single footer key | Retained focused footer and hot refusal tests, distributed COUNT(*) refusal journey | PASS |
| REQ-014/AC-016: staged reads use shared Parquet scan with pruning and session partitions | follower live_leaf → HotParquetExec → shared hot_stream; scan owns staged lease | Staged row-group pruning and lease-drop tests; current reported 19/19 live/follower units | PASS |
| INV-001: ACK/WAL/order/promotion retained | process_group durable fence/insertion/ACK; stage registration before manifest advance; unchanged fenced publication owners | Furnished Bifrost journeys and retained write/flush/read and recovery journey evidence | PASS |
| INV-004/005/006: valid terminals and query-owned source protection remain | Lease transfer into shared scan; native/wire completion validation precedes metric folding | Current reported live/follower units; retained completion and publication-overlap proofs | PASS |
| New metric changes introduce no persistent-data regression | Only post-validation scan evidence aggregation added; no write, identity, fence, file or catalog mutation | Current reported metric/follower unit set and lints; static producer-to-consumer trace | PASS |

The wall-clock publication entry added by adjacent TASK-006 code calls the
existing due-claim/publish owner. It neither marks an uncommitted object
published nor releases a staged member early; failure still leaves queryable
staged authority. This report does not qualify the separate TASK-006 acceptance
criteria or benchmark.

## Verification limits and findings

All runtime results above are user-furnished or retained historical evidence;
none was rerun. The reported full Bifrost aggregate was 8/9 lanes, with its sole
failure in the now-deleted process harness; reported post-edit focused checks
and real SDK/server journeys provide the relevant proof within this static
review restriction. The known 120-second lost-Scribe termination issue and
maintainer decisions are excluded as instructed. Capacity qualification remains
caller-owned. No benchmark result is inferred from correctness tests.

No reachable data-loss, duplicate-publication, tenant-footer production,
recovery, ACK/WAL, or shared-scan regression was found in the reviewed scope.
No source edits, commits or tests were performed. HEAD remained the immutable
candidate during this review. **PASS; empty proposed finding ledger.**
