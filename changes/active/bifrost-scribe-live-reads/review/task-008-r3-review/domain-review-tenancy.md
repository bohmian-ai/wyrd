# Independent tenancy and security review

## Subject and limits

Candidate `2f188cb6185061a43db36122aad68b5e253308d1`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; latest correction parent `9c3d7ecb982435919924dfa8e6930352b27a9b7e`. HEAD remained the candidate at the end of inspection. Read-only static review; no builds, tests, mise, benchmarks, commits, or source edits. This report is the only artifact written by this reviewer. Other current discovery reports were not read.

Reviewed the authenticated tenant-to-physical-source boundary across memory, staged, hot, published, cache, fragment execution, and leader refusal audit. The explicit maintainer decisions stand: no live snapshot cap, Postgres tenant columns excluded, the existing `WYRD_VALA_500_QUERY_TENANT_INVARIANT` code retained, FIND-007-3 excluded.

## Authority and coverage

| Authority | Applied boundary |
|---|---|
| AGENTS.md §§2, 3, 9–12 and architecture/agent-rules.md | Durable server ownership, authenticated tenant isolation, single canonical audit path, language/runtime verification ownership |
| architecture/wyrd-design.md, Bifrost; wyrd-doctrine.mdx | One tenant-qualified physical analytical table and shared server/client contract |
| architecture/bifrost-design.md, table identity, Oracle execution/admission/audit and maintenance | Authenticated footer identity, no row tripwire, resource and stream ownership |
| architecture/wyrd-security-posture.md, authorization, peer identity and data isolation | Peer transport alone never grants tenant authority; typed receiver validation precedes tenant IO |
| references/domain/olap-serving.md and iceberg.md | Signed source cuts, tenant-qualified cache, retained staged resources |
| spec.md revision 20 REQ-003–005, REQ-014–015, INV-002/004–008; TASK-007/TASK-008 and prior final verdict/R3 | Acceptance and permitted follow-on corrections |
| spec-driven-development.md and maintainer-style.md | Immutable cumulative subject and concrete source-based findings |

The current Bifrost design and explicit user authority supersede older security-posture wording about a per-row tripwire or Oracle audit WAL; these obsolete descriptions do not justify reopening approved decisions.

## Source trace and assessment

| Boundary | Source evidence | Assessment |
|---|---|---|
| Authenticated memory selection | `oracle/peer_service.rs` `ScribeFragmentExecutor::execute` checks target role/node/fence and verified claims, tenant equality and assignment digest before follower resolution. `oracle/follower.rs` preflight validates exact binding, signed closure and epoch/range. `scribe/shards.rs::snapshot_at` passes that binding; `memtable.rs::collect_readable_batches` selects tenant AND table AND partition. | PASS. Cap deletion removes only count/byte authority, preserving all tenant, schema and projection checks. |
| Staged authority | `tail_rpc.rs::staged_lease` selects tenant/table/range in `hot_source.rs::staged_sources`; `live_leaf` builds staged keys from the assignment tenant and local leased paths. The lease moves into `HotParquetExec`, and the stream retains it. | PASS. No path supplied by the public caller becomes tenant authority; no remote local-file read was introduced. |
| Producer footer identity | `parquet_writer.rs::prepare_sorted_candidate` rejects disagreement among binding tenant, seal tenant and frozen seal key; `ArtifactPlan` uses that seal tenant. `staging_runtime.rs` supplies `context.binding.tenant` to claim assembly. `claim_assembly.rs` passes it to the ordered writer. Both Forge managed execution paths pass `self.binding.tenant` through policy to rewrite writer properties. | PASS. Staged, assembled and rewritten output derives footer tenant from the same existing physical-table owner. |
| Missing/foreign/duplicate footer and cache | `parquet/footer.rs::verify_footer_tenant` requires exactly one field and exact tenant spelling. `exec.rs::PublishedFooterLoader::load` verifies on every cache hit/miss before returning metadata. `tenant_proven_reader_metadata` verifies hot/staged metadata before constructing the Arrow reader. Cache keys include tenant/table/object identity; cache does not authorize. | PASS. COUNT(*) and projection cannot omit the proof. The foreign file produces no decoded rows. |
| Refusal classification and audit | Shared footer check returns typed `QueryTenantInvariant`; peer stream classification and `live.rs::live_error` preserve it. `is_availability_loss` excludes tenant errors. `Oracle::audit_tenant_refusal` appends exactly the first tenant-file refusal through existing canonical non-blocking audit; both query classes use it. | PASS. Tenant refusal cannot become successful omission or capacity retry. Shutdown adds no new authorization decision or alternate audit path. |
| Digest/proto deletion | `assignment_authority.rs` uses v6 and still signs complete tenant/table/closure/reader/epoch/partition authority; deleted size fields are absent from conversions and reserved by proto numbers and names. | PASS. Existing v5 signed bytes cannot silently validate as v6; no compatibility bypass introduced. |
| Absolute data root | `BifrostDataRoot::prepare` resolves the operator root once with `std::path::absolute` before deriving roots and taking the existing lock. Staged resolver reads only leased server-created paths; footer checks remain mandatory. | PASS. Changes filesystem addressing, not tenant scope or authorization. |
| Shutdown and cancellation | `build_peer_grpc` injects the server shutdown token. `execute_fragment` races it against the next native frame and returns Unavailable; dropping the adapter stream drops the native stream and its retained lease/snapshot ownership. A returned unavailable status remains availability loss before rows and fatal after rows at the existing live-source consumer. | PASS for this domain. No tenant checks, failure identity, successful footer requirement or caller cancellation ownership were removed. A token is observed when the response body is polled; transport backpressure timing is a system-review concern rather than proof of a security regression. |

Symbol sweep over `crates`, `sdks`, `architecture`, and `docs` found the removed cap names only in the required proto reservations. Remaining generic “bounded snapshot” wording describes a tenant/partition/projection cut and is not an executable cap. One peer-service test comment still calls its capacity class a snapshot refusal; the runnable classifier remains valid for the surviving shard-mailbox refusal and does not affect tenancy.

## Verification evidence

Inspected the existing footer missing/foreign/duplicate test, hot-file before-row refusal test, public local/distributed foreign-footer journey, shard/memtable tenant isolation checks, signed assignment normative vector and conversion tests, and retained peer lifecycle scenarios. TASK-008 records focused negatives and exact integrated journeys. TASK-006's 2026-09-30 diagnosis records the old cap's 503 cause, relative-path NotFound cause, and corrected shutdown journey at 12.5 seconds, along with 897 spec, 14 conversion, 105 scoped redux, and 7 distributed journey successes. These are supplied evidence; this review does not claim newly executed proof or benchmark acceptance.

No required tenancy/security defect was found in the cumulative paths or follow-on corrections. FIND-007-4 is a standards finding, not a tenant-security finding; its two changed helper signatures preserve their argument types and classification bodies.

## Proposed findings and result

Proposed findings: **none**.

Overall domain result: **PASS**, within the stated static and supplied-evidence limits.
