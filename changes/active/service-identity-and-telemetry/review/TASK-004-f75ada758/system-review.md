# TASK-004 system resilience review

Overall: **FAIL**.

Subject: immutable cumulative `3e7c59db7a0f5e9506273eab826875cc8b59f226...f75ada758827266ac0b25a05d6a7b530f86d845a`; remediation locator `c4dd61029...f75ada758`. HEAD remained the candidate. This is independent static review. No new discovery reports or implementation completion summary were used. No Cargo commands, benchmarks, source changes or fault experiments were run.

## Authority and topology

Read revision-13 spec, storage supplement, original TASK-004, prior verdict/validated findings and R2; root and applicable SDK instructions, spec-driven workflow and maintainer guidance; owning Bifrost identity/admission/Scribe/Oracle/Forge sections; operations deployment-and-release and reliability-and-recovery authority. TASK-004 owns the versioned catalog substrate and future in-baseline evolution proof; TASK-005 retains public PATCH and committed-edit refresh.

One logical `wyrd-server` surface may run combined or role-targeted pods. Combined targets share public serving, Scribe, Oracle and Forge coordination. Every Scribe owns stable node identity and an exclusive persistent volume for WAL/staged runs. Postgres fences/file-list/coordination, Iceberg SQL catalog and object storage are shared durable authority. Oracle uses local captures or authenticated peer fragments, never another pod's filesystem. Forge uses durable lease/task/operation identity. The candidate adds no process, service, independent recovery scheduler or acceptance ledger. The supplied verifier-response-data merge was considered at dependency/schema/client seams without treating it as unauthorized drift.

## Deployed and failure paths

| Runtime/interruption | Source evidence | Availability, surviving state and recovery |
| --- | --- | --- |
| Native stale write and retry | `gate/mod.rs`; `scribe/ingress.rs::{resolve_logical_frame,prepare_and_dispatch,accepted_retry_rows}`; `shards.rs::committed_duplicate_rows` | Native frames retain a positive version. Explicit older versions select retained schema/partitioning before preparation; warm mirror or exceptional fence equality permits only accepted retries. Unknown stale IDs are refused before WAL. Original native retry geometry is corrected. Canonical recovery remains defective below. |
| Cancellation around dispatch | Ingress reservations and shard mailbox/completion ownership | Before dispatch reservations release. After dispatch the shard can finish accepted work despite response loss; strict same-ID retry is therefore a required recovery path. No new detached completion owner was introduced. |
| WAL fsync and grouped SQL fence | `shards.rs::{append_and_sync_batch_commits,commit_batch_control_fences}`; SQL `{record,resolve}` | SLICE/COMMIT fsync precedes fencing. One tenant transaction inserts offered fences; full table/batch keys classify results independently of RETURNING order. Contradiction rolls back fresh fences. Ambiguous COMMIT uses a fresh batched exact read; absent fences retry identical inserts. Unavailable/contradictory reconciliation poisons Scribe admission and retains the complete group rather than ACKing or releasing ambiguity. This does not crash the shared server. |
| Crash with provisional duplicate WAL attempts | `replay.rs::ReplayAccumulator::commit`; shard `{resolve_replay_fence,resolve_replayed_commit,retain_replayed_batches}`; SQL `resolve_replay` | Durable canonical coordinates select the accepted attempt. Suppression uses batch plus inclusive LSN range, so refusing another attempt does not discard the original. Contradictions at canonical coordinates fail closed. Contested unfenced attempts refuse speculative selection; uncontested complete unfenced commits retain recovery behavior. |
| Seal/staged restart after definition edit | `persistence.rs::stage_generation`; `write_recipe.rs`; `hot_stage.rs`; `assembly.rs`; `staging_runtime.rs::resolve_context` | Full version/typed partition remain in seal and assembly identity. Native retained metadata resolves the original layout/Blooms/order against staged schema. Decode/recipe failures retain evidence and refuse affected-role readiness; no fallback to current properties was found. |
| Catalog/object outage | Catalog current/retained definition and metadata-cache owners, existing persistence/staging lanes | Cold historical resolution may fail, stopping that seal/recovery. WAL, generations and staged members remain authority. No new unbounded queue or shared-process termination path was found. No quantitative recovery bound is established here. |
| Bad Scribe state in combined pod | `wyrd-server/src/boot/mod.rs:691–706` | Replay error is caught, Scribe fault token is cancelled and role fence remains unactivated. Other roles retain their lifecycle; aggregate readiness can remain false under the existing role policy. |
| Mixed-version query / peer loss | `memtable.rs::{LiveCapture,ReadableSource::project}`, `hot_source.rs`, `tail_rpc.rs`, Oracle follower/exec and catalog pins | Full seal-key captures preserve sources sharing ordinal/time. Field-ID projection retains renamed values and fills missing nullable columns. Published predicates bind pinned current schema; native spec identity survives. Existing signed assignments, leases, deadlines, cancellation and terminal framing retain ownership; no partial-result success or interactive fallback was added. |
| Promotion loss/timeout | `forge/scribe_promotion.rs::{validate_durable_partition,validate_promoted_object,commit_promotion}` | Complete SQL tuple is compared with promotion evidence on worker revalidation. Existing fence renewal/commit window and snapshot task/operation evidence survive interruption. Post-submission timeout/cancellation is unknown acceptance requiring reconciliation, not clean rejection or permission to duplicate append. |
| Compaction/expiry/orphan GC | Native compaction `1ade704`; Forge compact/expire/orphan_gc; catalog fork `a34f388` | Native partition-scoped grouping includes spec plus typed tuple. Every retained definition location is excluded from expiry and included in orphan protection. Listing failure stops cleanup. Retain-all is allowed without an approved retry horizon; snapshot/data retention remains separate. |
| Client bind/record/drain | `wyrd-client/bifrost/{facade,handle,observe}.rs`; Python/TS callers | Declared writers resolve once before queueing. Producer keys include version so queued data is never rebound. Describe refusal leaves an unresolved target and insert refuses. Record admission is taken before async lookup/spawn; drains wait for settlement before closing producer owners. Existing direct-send/queue admission still owns later work; Python resolves outside the binding lock. |

Inspected pinned source at `/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/a34f388/crates/catalog/sql/src/catalog.rs`: creation/update publish retained address and pointer in one transaction, losing CAS rolls it back, and COMMIT failure checks authoritative pointer evidence before success. Native grouping source was inspected at `/home/thorrester/.cargo/git/checkouts/iceberg-compaction-3b4eb9269abd88f3/1ade704/core/src/file_selection/strategy.rs`. Dependency tests were not rerun.

## Material finding

### SYSTEM-R2-001 — INCORRECT — Canonical accepted retries are rebound to the current version

**Prior ID:** remaining scope of FIND-TASK-004-1.

**Obligation:** revision-13 REQ-037/038 and single-version contract; R2 Client alignment and admission; `architecture/bifrost-design.md:397–402,603–618`. Native and canonical accepted input must compare under the fence's original definition. The approved catalog-seam proof is TASK-004 work; this finding requires neither public PATCH nor distributed cache refresh.

**Locations:** `crates/vala/vala-bifrost-redux/src/gate/mod.rs:963–964`; `scribe/ingress.rs:242–272,629–640`; `scribe/shards.rs:1135–1150,3740–3750,3801–3854`; `crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:120–124,506–526`.

**Reachable producer-to-consumer path:**

1. Public OTLP reaches `Gate::dispatch_canonical`. Unchanged accepted rows, tenant, publisher and attribution produce the same deterministic `otlp_batch_id`, but Gate sends `table_version: None`.
2. `resolve_logical_frame` maps `None` to current definition. Only an explicitly supplied older version sets `retry_only`. Canonical rows are therefore prepared and stamped under the current version before any committed-mirror lookup.
3. The warm mirror includes table version in equality, so it misses after an edit. `accepted_retry_rows` runs only for `retry_only`, which is absent here.
4. The shard creates a provisional attempt under the new version. SQL `matches_logical` requires the original version; conflict classification returns an invariant error. Neither the fence owner nor another canonical caller retrieves the original definition and compares under it.

**Concrete schedule:** acknowledge an OTLP export at built-in version 1; change only its complete Bloom list through the real versioned catalog seam; resend exactly the same export/authority. `WyrdTestServer::commit_bloom_revision` (`wyrd-testing/src/bifrost/revisions.rs:220`) accepts a built-in TableRef, commits through the catalog and evicts its descriptor. No PATCH is needed to exercise this required substrate. The retry has the same batch ID and rows but version 2. Even unchanged geometry cannot satisfy SQL version equality. Restart/cold mirror gives the same result. Later replay can suppress the rejected provisional attempt, but does not supply the required successful retry acknowledgment.

**Consequence:** response loss, cancellation or delayed OTLP retry overlapping an allowed definition edit makes already accepted identical input appear contradictory. Original row loss is not alleged. Failure belongs at the request boundary; it does not justify stopping Scribe or its shared process.

**Smallest correction:** complete original-binding recovery at the existing ingress/mirror and fence owners. Warm accepted bindings must select retained preparation before current rules; cold fence conflicts must permit strict comparison under their original retained version outside the transaction before acknowledging. Preserve canonical server-owned projection, unknown-ID refusal, strict source/payload/order/count and authorization checks, tenant-group rollback and no preliminary accepted-ID SQL on successful fresh ingest. Do not weaken version equality, blindly accept spent IDs, add a ledger/service or make canonical clients describe synthetic schemas.

**Closure proof:** extend the real OTLP/catalog-seam journey with an acknowledged export, Bloom-only revision, and exact retry warm, after retained-root restart, and after publication/WAL retirement. Require successful acknowledgments, original exact rows once, and unchanged ingestion/origin. Changed source still refuses. A partition-edit variant should prove original geometry where supported. Native explicit-original-version sort-only proof cannot detect this failure.

## Prior closure and evidence assessment

| Prior finding | System-scope disposition |
| --- | --- |
| FIND-1 | Native original-version retry, retained recipe reconstruction and provisional WAL replay corrected; canonical binding remains open above. `ScribePublishedHotFileV1` also lacks the required table-version field, but no separate cleanup-loss claim is made because all definitions are retained. |
| FIND-2 | `NativeSliceProducer.partitions` and `PartitionPlan::union_into` preserve request-wide union before WAL, not just per-chunk fanout. |
| FIND-3 | Pinned native grouping includes spec plus typed tuple and preserves native table-scoped work. |
| FIND-4 | `validate_durable_partition` reconstructs SQL dimensions and compares complete partition to evidence; worker calls it before publication. |
| FIND-5 | LiveCapture/source registry uses complete SealKey plus generation, retaining distinct versions/dimensions sharing an ordinal. |
| FIND-6 | Inspected system owners have intent/failure/cancellation documentation; full rustdoc enumeration belongs to standards/maintainer review. No separate system finding. |
| FIND-7 | Supplied R2 evidence/new Oracle journey covers complete/partial/empty/reordered policies, actual hot/promoted/rewrite objects, exact identities and pruning/physical-byte/planning measurements. This is supplied/source evidence, not an independent execution. |

Supplied R2 verification covers native accepted-retry/replay, SQL grouping/rollback, retained-definition expiry, mixed-source/promotion, SDK/observe/Python/TS journeys and targeted libraries. It does not prove canonical retry across a version change: its canonical cold retry is reported only at version 1; accepted-revision proof sends native frames with the explicit original version. No result from the concurrent external run was available to this reviewer.

Skipped independent Cargo, fault and benchmark execution as instructed. Throughput remains honestly unmeasured and does not cause FAIL; the canonical accepted-retry failure does.
