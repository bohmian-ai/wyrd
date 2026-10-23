# TASK-004 R3 system recovery review

**FAIL — FIND-TASK-004-1 is partially corrected; three material recovery/resource gaps remain.**

Immutable candidate: `5956466a1299a99521ed9264b2a711d3f1957e3e`; prior `f75ada758827266ac0b25a05d6a7b530f86d845a`; original base `3e7c59db7a0f5e9506273eab826875cc8b59f226`. HEAD was checked as the candidate. This is scoped independent static review of no-claim accepted-retry recovery and its affected shared fence/resource owners. No build, Cargo, database, benchmark or fault experiment ran. Reports only; unrelated working changes are excluded.

Read the shared subject/navigation map, approved revision-13 identity/evolution/grouping requirements and storage supplement, original TASK-004, prior system review and validated ledger, R3 task and appended evidence, owning Bifrost admission/retry/resource authority, recovery authority, agent maintainer guidance, TESTING and spec-driven authority. Read the remediation diff and complete changed admission, decode, preparation, grouped fencing, completion and reconciliation functions, their Gate/native callers, replay consumers and relevant journey/SQL tests. No CodeGraph directory exists.

## Recovery matrix

| Supported path / interruption | Candidate source and result |
| --- | --- |
| Warm exact canonical retry after edit | Gate `dispatch_canonical` supplies no claim. `prepare_and_dispatch` first asks the existing mirror for its version; retained-definition preparation and strict mirror equality then acknowledge. Corrected. |
| Cold exact canonical retry after Bloom-only edit | Current preparation succeeds; SQL returns `AcceptedUnder`; provisional slices suppress, ingress prepares the shared original source under the retained version and `accepted_retry_rows` proves equality. This closes the previously demonstrated Bloom case. |
| Cold retry whose original rows violate the current partition rules | Preparation returns through `?` before durable identity acquisition. No accepting-version recovery occurs. **SYSTEM-R3-001.** |
| Same-version contradiction in mixed SQL group | Existing `classify` error still rolls back the transaction. The unchanged integration test covers this case. |
| Changed-version contradiction in mixed group | `AcceptedUnder` commits fresh peers before equality or explicit-claim refusal. **SYSTEM-R3-002.** |
| WAL fsync / ambiguous SQL COMMIT | SLICE and COMMIT sync still precede fences. Fresh-connection batched resolution retains absent/retry and poison-on-unavailable/contradictory behavior. No new normal-ingest SQL statement was found. Changed-version classification inherits the gap above. |
| Crash after a provisional changed-version attempt | `resolve_replay` still selects the original immutable fence coordinates, suppresses other attempts, and checks version/digest/count/request at canonical coordinates. `resolve_replay_fence` suppresses only the corresponding batch/LSN interval. No new resurrection of a noncanonical attempt was found. |
| Cancellation before / after dispatch | Local admission owns work before dispatch; shard owns accepted work after dispatch. Response loss can therefore require the exact retry paths above. The new hint is not an ACK. |
| Publication, WAL retirement, restart | Durable fences retain accepting version and original coordinates after local evidence retires; retained catalog definitions remain the resolver. The new Bloom journey retries before publication, so publication/WAL-retirement recovery is source-supported but not newly demonstrated by that journey. Current-preparation refusal remains defective in every cold state. |
| Fresh canonical export crossing hourly partitions | New unconditional no-claim retry shadow keeps source Arrow alive after prepared-byte accounting drops it. **SYSTEM-R3-003.** |

Topology and unaffected recovery evidence carry forward from the prior system review: stable per-Scribe WAL volume/node ownership; shared Postgres/catalog/object authority; role-scoped replay failure/readiness handling; retained staged definitions; tenant-bound Oracle captures/peers; promotion reconciliation and conservative definition retention. Nothing in this scoped diff establishes a reason to reopen findings 2–7. Persistent promotion/Forge changes receive a separate specialist review.

## Retained findings

### SYSTEM-R3-001 — INCORRECT — A cold accepted retry still fails current-definition preparation

**Prior scope:** FIND-TASK-004-1. **Obligation:** spec REQ-035–038 and AC-013; supplement lines 680–710; `architecture/bifrost-design.md:604–619`; R3 FIND-1 explicitly requires exceptional recovery after current-contract preparation refusal.

**Source:** `scribe/ingress.rs:565–595`, especially `prepare_and_dispatch_under(frame).await?` at 581; preparation at 635–678; `scribe/seal_key.rs:358–385` rejects a 33rd complete partition. Only successful dispatch can return `AcceptedUnder`, so the saved original frame is never used after decode/preprocess fails.

**Concrete supported producer/schedule:** export 33 valid OTLP spans in one hour with 33 distinct `scope_name` values. `SpansTable::physical_layout` (`tables/traces/spans.rs:149`) initially partitions only hourly, so this is one physical partition and is accepted. Through the real catalog seam, commit an authorized layout edit adding identity(`scope_name`) to that same time axis; REQ-035 explicitly permits system-table partition overrides under the dynamic-table rules. Evict the committed mirror by restart/retirement or use another cold Scribe. Gate projects the identical export with the identical deterministic ID and no version (`gate/mod.rs:951–974`). Current preparation now sees 33 tuples and returns `InvalidFrame` before the existing fence is consulted. Warm recovery works because it selects version 1 first; cold recovery refuses already accepted identical input. This does not require native changed-version success or public PATCH.

**Consequence:** response loss or delayed exporter replay across a supported grouping edit produces a permanent false refusal despite authoritative original acceptance. Existing accepted rows are not alleged lost. The new Bloom-only journey cannot expose it because its current preparation remains valid.

**Smallest correction:** use the existing exceptional accepted-ID acquisition boundary after an eligible current-contract preparation refusal, recover its original version, and compare the retained source under that definition on the bounded CPU owner. Unknown IDs must keep the original refusal. Keep trust checks, original-version native strictness, and zero preliminary accepted-ID SQL on successful fresh ingest. Add the exact no-claim grouping/fanout journey warm/cold and after publication/retirement.

### SYSTEM-R3-002 — INCORRECT — Changed-version contradictions commit fresh group peers before proof

**Prior scope:** FIND-TASK-004-1 / AC-023 preservation. **Obligation:** supplement lines 693–710 and Bifrost lines 593–618 require contradiction rollback and rollback of uncommitted fresh fences before outside-transaction retained comparison; R3 selected correction says the same.

**Source:** `vala-sql/src/queries/scribe_batch_commits.rs:519–534` converts any differing positive version into `AcceptedUnder`, even when digest/count differ. `scribe/shards.rs:3847–3850` unconditionally commits the transaction before returning it. `append_and_sync_batch_commits` marks that ID spent, then `process_group` inserts and `acknowledge_visible` completes fresh peers (`shards.rs:3113–3248,3727–3755`). Only afterward does ingress compare or refuse the conflicting request (`ingress.rs:581–588`). The appended R3 evidence acknowledges this departure.

**Concrete supported producer/schedule:** accept native batch A at version 1; make a Bloom-only edit to version 2. On a cold mirror, submit A's spent ID with valid current-version schema, explicit version 2 and changed payload, alongside fresh B in the same tenant/shard drained group. Native clients supply IDs and explicit version claims, so this contradiction does not depend on forging a deterministic canonical Gate ID. Current preparation succeeds. SQL classifies A as `AcceptedUnder(1)` and B as `Committed`, commits both results, suppresses A, inserts/ACKs B. Ingress then refuses A because an explicit claim is intentionally strict. B's fence/rows have already become durable and visible. The same ordering also commits peers before an unclaimed retry's retained equality is established or fails.

**Consequence:** a genuine changed-version contradiction no longer aborts the tenant group's fresh acceptance. Failure is discovered after sibling visibility/ACK, so rollback cannot restore the required group result. Replay faithfully preserves B and cannot repair that ordering. This is an observable acceptance/atomicity regression, not an objection to harmless private mechanics, and does not allege A's contradictory rows are inserted.

**Smallest correction:** treat `AcceptedUnder` as an unresolved provisional result at the existing group owner: roll back fresh fences before bounded outside-transaction comparison and resume only on equality. Explicit changed native claims remain conflicts. Keep group ordering, tenant ownership, existing fresh INSERT count and batched uncertainty reconciliation. Extend the existing SQL/group proof with changed version plus changed digest and fresh peers; the unchanged rollback test changes only digest at the same version and misses this regression.

### SYSTEM-R3-003 — INCORRECT — Fresh canonical retry shadows retain uncharged source buffers

**Affected boundary:** FIND-TASK-004-1's source/resource-ownership constraint. **Obligation:** `architecture/bifrost-design.md:532–545`: every byte category is owned, and temporary overlap is reserved explicitly; R3 requires retained source ownership under existing accounting.

**Source:** every fresh no-claim canonical request creates `unclaimed_retry` before admission (`ingress.rs:576–581`). Its `CanonicalIngress` clones batches and `LogicalIdentity`, with `owner: None` (`:936–957`), and remains alive throughout `durable_rx.await` (`:702`). These are reference-counted buffers: **this does not add a full-payload copy or hash pass.** It does extend their lifetime.

**Concrete supported producer:** one normal `ExportTraceServiceRequest` containing spans in two adjacent valid hours. Spans use the hourly `start_time_unix_nano` layout; `ingest_decoded_resource_spans` projects one canonical batch, drops the generated request and transfers its sole decode owner (`gate/mod.rs:799–820`). `admit_transport_frame` takes that owner and resizes its one lease (`ingress.rs:434–458`; `contracts.rs:276–277`); no separate request lease continues to cover the source. For more than one partition, `PartitionPlan::materialize` uses Arrow `take` to allocate separate slice buffers (`seal_key.rs:250–278`). `prepare_rows` then drops its original `rows`/`known`, while `prepared_slice_bytes` charges only the new slices' Arrow plus WAL (`preprocess.rs:686–718,829–843`). `charge_prepared` resizes the sole lease to that value (`ingress.rs:778–790`). The shadow still holds the original Arrow until the durable result, outside that accounting. The same source lifetime was released before the shadow was added.

**Consequence:** while WAL/Postgres or shard scheduling delays fresh multipart exports, actual retained memory includes the original projected source in addition to the charged prepared set. Across N admitted waiting exports, the omission is the sum of those original source buffers; item-count bounds do not make byte accounting exact or preserve the configured memory envelope. This affects ordinary fresh OTLP, without any catalog edit or retry. No unbounded-memory or measured-throughput claim is made.

**Smallest correction:** keep the retained recovery source with an existing charged owner through its real lifetime and release/resize that ownership when the shadow drops, including cancellation and cold recovery. Do not add payload copies, a second cache, or blanket normal-ingest SQL. A focused two-hour canonical accounting/cancellation proof should hold shard completion and assert all retained source plus prepared bytes remain covered, then release exactly once.

## Evidence limits

The appended R3 evidence reports passing canonical Bloom warm/restart, native accepted-version, mixed-schema, SQL operation-count/rollback and replay-related lanes. Source inspection supports the Bloom correction, preserved explicit original-version native behavior, no extra fresh accepted-ID SQL, bounded CPU preparation, strict equality before retry ACK, and canonical-coordinate replay. The added journey does not exercise current preparation refusal, changed-version mixed-group rollback, post-publication retry, or multipart retained-source accounting. Existing SQL contradiction coverage remains same-version. No request to require explicitly changed native claims to succeed is made.

Skipped independent runtime/build/DB/fault and benchmark execution as instructed. Throughput remains unmeasured; the failures above are source-backed correctness and resource-ownership gaps, not benchmark conclusions.
