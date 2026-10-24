# TASK-004 behavior acceptance review

Overall result: **FAIL**.

Subject: base `3e7c59db7`, candidate `c4dd61029a54c44017c63bd4ff469472c406bc13`. HEAD still equals the candidate. This is the original cumulative task review, with no previous findings to close. Source was not edited. No other discovery report was read.

Authority: approved specification revision 12, original `tasks/TASK-004-shared-table-storage.md`, normative `supplements/table-storage-and-correlation.md`, root and touched SDK/docs AGENTS rules, `changes/spec-driven-development.md`, agent Rust/error/surface/maintainer guides, and the applicable Bifrost identity/storage/durability/Scribe/Oracle/Forge and security tenant/peer sections. CodeGraph is absent. Review is static; supplied verification results are evidence of executions reported in the task, not executions independently repeated here.

## Coverage and independently expanded map

The cumulative range, rather than the latest implementation commit alone, is the subject. Task behavior coverage is complete: creation, both row preparation branches, identity producers and persistence consumers, local/peer projection, Forge output, public layout consumers, tests, and explicit non-goals were assessed. This is not a claim that every possible runtime combination was executed.

| Boundary | Producer, caller, and consumer coverage |
| --- | --- |
| Declaration and creation | `PhysicalLayoutWire`, `PartitionFieldWire`, `PartitionTransformWire`; `PhysicalLayout::{resolve,from_stored_wire,of_iceberg,iceberg_partition_spec}` and canonicalizers; Bifrost catalog creation/registration/physical validation; Rust `TableConfig`, server service tests, Python layout assembly/exports/stubs and TS layout serialization/generated types; V1/V2 and existing time-column consumers |
| Admission and retries | Gate/native IPC door → `ScribeImpl::{resolve_logical_frame,admit_transport_frame,prepare_and_dispatch}` → `execution_lanes::decode_rows`/registered schema conformity → both `prepare_rows` branches → `PartitionScheme`/`plan_partitions` → prepared slices → `committed_duplicate_rows` → shard WAL/commit fence; also `committed_batches` and `vala_sql::queries::scribe_batch_commits` |
| Physical lifecycle | `PhysicalPartition` typed binary/wire/struct codecs; `SealKey` and `LayoutRevision`; WAL-v7 preparation/decode, replay grouping/digests, shard published replay suppression; memtable freeze and live capture; assembly keys, stage records, member staging, persistence, stage restore, claims, file-list insertion, publication records and migration |
| Reads and authority | Catalog retained metadata/pins, Oracle published scan schema selection, follower closure fields, tail request and memtable projection by field ID, DataFusion field-ID adapter, source sizing and existing tenant/footer/peer boundaries; V5/V7 and catalog production pin test |
| Maintenance and physical truth | Forge promotion validation and retained schema/footer projection; retained-spec manifest append; managed compaction dependency and shared recipe; V8 physical tuple/order assertions and V9 actual Parquet/Bloom assertions; unchanged fence/pin/retention ownership |
| Integration and excluded scope | New `wyrd-testing::bifrost::revisions` uses the real catalog seam; registered scribe/forge test targets and their support changes; architecture and generated docs changes; packet additions are context/authority. Public PATCH/CAS/cache broadcasting, system schema/default replacement and final docsite closure remain allocated to the named later tasks. |

Pinned dependency source was inspected locally: DataFusion `1eab549` vs `b8ec4b1` changes physical expression column resolution to field IDs; Iceberg `486273c` vs `bc01647` supplies retained-schema scans, guarded metadata updates and per-spec append manifests; compaction `efb39ce` vs `8cc7bc5` changes dependency pins rather than introducing another rewrite engine. These are extensions of installed owners. `CommittedRevisions` replaces the former schema cache; it is not an additional independent acceptance ledger. `PhysicalPartition` and `PartitionScheme` supply one typed identity/transform owner instead of distinct implementations in each tier. No new tuning, transport, query engine or automatic history rewrite was found.

## Acceptance matrix

PASS below means the implementation and available source/tests support the bounded obligation, with the verification limits stated above. It does not waive the failing sibling paths.

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| One system/dynamic layout validator; exactly one required UTC hour/day axis | `catalog/layout.rs` canonical time/partition validation; creation passes resolved layout into Iceberg | V1 type/count/transform matrix and V2 real registration/description/refusal | PASS |
| At most two eligible dimensions; transform parameters, duplicates and unsupported types refused | `canonical_partition_by`, `PartitionTransform::from_wire`, canonical sort/Bloom validation | V1 enumerates eligible/ineligible pairings, bounds and precise errors | PASS |
| Omitted dynamic creation layout remains hourly, time DESC/nulls-last and empty Bloom set | `PhysicalLayout::resolve`; `DomainTable`/table declarations preserve existing defaults explicitly when removing floor insertion | V1, V2 and builtin fixed-point tests | PASS |
| Same registration idempotent, changed declaration does not silently alter table, refusal leaves no registration | Existing catalog workflow and physical-table validation updated for full partition list | V2 re-register, mismatch, describe unchanged and refused-name creation | PASS |
| Canonical `partition_by` replaces public configuration `partition_granularity` | Rust serde denies replaced/unknown layout keys; Python and TS assembly and exported declarations updated | Wire spelling refusal and changed SDK fixtures/type contracts; supplied codegen check | PASS |
| Complete typed tuple distinguishes null/literal/type/spec and time-field position | `PhysicalPartition::{encode,decode,to_wire,iceberg_struct}`, `PartitionScheme::rows`, Iceberg transform application | V3 null/string/int/spec/codec and actual transform cases | PASS |
| Maximum 32 distinct tuples for the complete accepted batch; 33 refuses before WAL/fence | Limit is currently per decoded native RecordBatch, not request-wide (`preprocess.rs:338–343,698–710`) | V4 sends only one RecordBatch; does not cover a multi-batch IPC stream | **FAIL — BEH-002** |
| Single-source excessive fanout refuses atomically and corrected batch ID can succeed | Preparation completes before shard dispatch/WAL | V4 refuses 33, observes zero rows/objects, resends 32 with same ID | PASS |
| Active/frozen/staged/published/WAL/restart and peer paths preserve composite rows and accepted origin | Seal/WAL/stage/publication identities carry full tuple and schema/sort revision; existing stamping retained | V3/V5, existing persistence/replay regressions; public local and two-pod reads | PASS for tested unchanged-layout lifecycle |
| Accepted retry binds original schema/layout before current-write rules | Ingress always resolves current schema/layout and preprocesses before duplicate/fence resolution | V6 exercises a sort-order-only edit; V7 changes partition/schema without retry | **FAIL — BEH-001** |
| New writes bind committed schema/spec/sort; failed edits cannot reinterpret already retained generations | `committed_revision`, revision-bearing `SealKey`, retained `resolve_write_recipe` | V6 checks distinct old/new sort IDs; catalog seam guarded updates | PASS apart from retry/recovery findings |
| Rename preserves IDs/values; old sources read nullable additions as null | Memtable projection by field IDs; DataFusion adapter; Oracle explicit scan schema; retained schema/footer projection | V7 published/staged/active/restored equality/null/IN/time filters | PASS |
| Missing optional historical Bloom artifacts do not change exact reads | Optional Bloom recipe; existing exact filters remain; mixed source field-ID projection | V7 uses empty sets; V9 reads encoded rows and probes held membership | PASS for available exact-result cases; broader proof gap below |
| A valid Bloom/layout policy change must not strand previously accepted staged work on restart | Current Bloom property is rebuilt into an immutable old assembly fingerprint | No staged-backlog Bloom edit/restart case | **FAIL — BEH-003** |
| Tenant and source authority remain enforced; no broader credential masks a refusal | Authenticated binding, full tuple plus tenant in seals, footer identity, existing peer closure/fence owners retained | V7 foreign-tenant refusal; existing security/promotion mismatch tests; no credential broadening in diff | PASS |
| Pins retain snapshot metadata/files across compatible revision | Existing pinned-provider owner and retained Iceberg metadata, no cleanup ownership replacement | Production catalog pin test checks historical schema and planning after revision; existing pin/retention tests supplied | PASS |
| Forge accepts retained specs/orders, preserves boundaries and makes truthful output claims | Promotion resolves retained spec/order/schema; Iceberg manifests grouped by spec; existing compaction owner | V8 actual object hour/customer tuple and recorded sort prefix, public row equality before/after compaction | PASS |
| Scalar sort direction/null order and resolved complete/partial/empty/reordered Blooms applied physically | Existing shared Parquet encoder/merge recipe retained | V9 physical row order and exact filter presence/held-value checks | PASS |
| Representative file counts/sizes, groups/pages/bytes and scan/pruning/planning overhead reported, without new thresholds | Available metadata helpers expose some geometry but new proofs do not report the requested measurements | Task evidence explicitly records plan count/row geometry only; V9 scans all rows without a reader predicate | **FAIL — required proof gap** |
| No historical migration/backfill, row TTL, default retuning, second engine or automatic history rewrite | One new additive file-list dimension migration; existing storage/Oracle/Forge owners and constants retained | Cumulative source/manifests; supplied dependency/tenant/codegen checks | PASS |
| Physical work does not claim TASK-003 system schema/default, TASK-005 PATCH/CAS, or later language-journey closure | Shared retained metadata seam plus necessary layout consumer updates only | Task boundaries and tests explicitly use internal catalog edits | PASS |

## Proposed findings

### BEH-001 — INCORRECT: accepted retries are repartitioned under current metadata

**Obligation:** TASK-004 scenario 3 and REQ-037/038, AC-013: resolve an already accepted batch under its original schema/layout before applying current-write rules. Changed payload/schema/order must still conflict.

**Location:** `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:225–235,592–593`; supporting producers/consumers in `preprocess.rs:141–165,698–710,1021–1035`, `shards.rs:1128–1141,3561–3678`, and `vala-sql/src/queries/scribe_batch_commits.rs::ExistingScribeBatchCommit::matches_logical`.

**Reachable path:** accept native batch X containing rows in two customer/shard tuples; change the partition spec to remove the shard bucket (the supported `commit_spec_revision` seam already does this); resend identical IPC/schema and batch UUID. Ingress chooses the new scheme. Preprocessing merges the old per-shard slices into fewer slices before asking whether X is known. Both the ordered per-slice digest and count can now differ. The memory duplicate lookup misses, and the durable fence compares the changed slice-set digest/count and refuses the honest retry. After a stable-ID rename, current-schema conformity can refuse the original field spelling even earlier. After a nullable addition, the current conformed schema may change the digest. This is one acceptance-resolution cause across these callers, not a separate defect per edit.

The existing cache and durable batch fence are the sibling identity authorities; neither retains/selects the original revision before current preprocessing. WAL replay preserves its stored revision correctly, but a client resend is not WAL replay. The present V6 test changes only sort order, which does not change the slice payload/grouping, so it cannot falsify this defect.

**Consequence:** an acknowledged batch cannot be safely retried through a supported runtime partition/schema revision, even though its bytes are unchanged. Availability of idempotent acknowledgement depends on the current layout.

**Required correction:** resolve accepted identity at the existing tenant/table/batch acceptance authority before selecting the preparation recipe; retain enough original binding there to validate/reproduce the original schema and partitioning across restart and publication. Reuse the existing fence/cache/WAL ownership rather than a new ledger, and retain exact schema/row-order/payload conflict checks. New IDs must continue to select the committed revision. Do not fix this by ignoring slice count/digest conflicts.

**Focused proof:** extend the existing accepted-revision journey with a spec edit that changes grouping, then resend the identical batch before and after restart/publication and require acknowledgement plus unchanged exact rows and original published identity. Include a stable-ID rename/nullable addition retry and a changed-payload refusal. No public PATCH API is required for this task's proof.

### BEH-002 — INCORRECT: native stream fanout bound resets for each RecordBatch

**Obligation:** REQ-036, AC-012/021 and supplement's `Per-write fanout`: maximum 32 distinct complete partitions per accepted batch, refused before durable acceptance.

**Location:** `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:338–343,698–710`; bound owner `scribe/seal_key.rs:337–370`.

**Reachable path:** one authenticated native Arrow IPC request/one batch UUID contains two RecordBatch messages with the same registered schema. Put 16 customer identity values in the first and 17 different values in the second, all in one hour and far below material ceilings. `decode_next` permits successive batches. Each call to `plan_partitions` creates a fresh local distinct-tuple vector and passes. `prepare_rows` collects both sets, assigns 33 ordinals, and returns the request for normal WAL/COMMIT/fence processing. No later distinct-union check exists. Repeated tuples in separate messages also cannot be accounted for by a simple sum of source plan lengths.

Both canonical and native preparation share the transform/partition owner, but only native preserves multiple source batches; canonical preparation plans its assembled rows once. The public `RawIngest` door accepts IPC bytes, so this is a reachable public path. V4 serializes a single RecordBatch and cannot catch it.

**Consequence:** a caller bypasses the approved per-write fanout limit by choosing IPC record-batch boundaries; a 33-tuple logical batch receives durable acceptance.

**Required correction:** enforce the union of complete `PhysicalPartition` values across the complete native preparation, bounded at the existing limit before dispatch/WAL. Extend existing preparation/partition accounting, preserving source decoding, exact row/slice identities and memory limits; do not reject more than 32 slices when repeated partitions across source batches still represent at most 32 tuples.

**Focused proof:** submit a two-message native IPC stream with 32 distinct tuples successfully, 33 refused with unchanged WAL/fence/visible rows, and repeated tuples across messages counted once. Correct and reuse the refused batch ID to demonstrate no durable reservation residue.

### BEH-003 — INCORRECT: current optional Bloom policy invalidates old staged identity

**Obligation:** TASK-004 scenario 3 and REQ-038/042: accepted work survives valid layout changes/restart; replacing or clearing optional Blooms affects future output without making historical sources unreadable. Bloom order has no membership semantics.

**Location:** `crates/vala/vala-bifrost-redux/src/catalog/layout.rs:1007–1013`, `scribe/assembly.rs:275–279`, `scribe/staging_runtime.rs:825–847`; retained recipe caller `scribe/write_recipe.rs:65–77`.

**Reachable path:** accept and freeze a batch into durable staged runs while Bloom policy is `[customer]`; commit a supported catalog property edit to `[]` (or a replacement/reordering) while those runs remain unpublished; restart. The existing staged record retains the original assembly-key fingerprint. `restore` calls `restore_context`, which asks `retained_layout` for the old schema/spec/sort IDs. Despite those IDs, `PhysicalLayout::of_iceberg` reads the table's **current** Bloom property. `ScribeAssemblyKey::new` hashes this changed Bloom list, and the equality check refuses the recovered key. A stable-ID rename of a Bloomed custom column has the same source: the new property name is absent from the retained schema and is dropped, changing the fingerprint.

Sibling writers are live member staging, recovered claim-context reconstruction, and publication from those contexts. Runtime retained schema/spec/sort support does not retain Bloom property history, while assembly identity treats that property and its order as immutable. The two choices conflict at their existing shared owner.

**Consequence:** a valid optional Bloom edit can prevent staged accepted rows from being recovered/published; the Scribe startup path refuses restoration before admission rather than recovering the acknowledged backlog.

**Required correction:** make the existing retained-recipe/staged-context authority consistent about mutable optional Bloom policy. Preserve immutable accepted schema/spec/sort and corruption checks, but prevent a current Bloom replacement/clear/reorder or stable-ID rename from invalidating an otherwise valid durable member. Reuse the existing staged record/recipe owner; no second history ledger or weaker key-wide validation. The validator should select the smallest correction consistent with accepted-layout preservation, rather than adding a consumer exception to the equality check.

**Focused proof:** stage without publishing, alter Bloom policy through the real metadata seam, restart on the same durable root, query exact rows and publish successfully. Exercise empty/replacement/reorder and a Bloomed-field rename; keep an actually contradictory schema/spec/sort/key case failing closed.

## Required proof gap and execution limits

The original task explicitly requires representative file/group/page/byte and scan/pruning/planning evidence. The available V8 evidence records 15 admitted plans and 48 rows; it does not supply file sizes, group/page counts, bytes scanned or pruned, or planning overhead for representative same-Service and cross-Service/environment predicates. V9's six rows are read with an unfiltered `ParquetRecordBatchReaderBuilder`; held-value Bloom probes and equal full decoded rows prove filter construction and physical sort, not actual reader pruning or complete/empty-filter predicate equivalence through Oracle. V7 supplies exact predicates on a dataset with empty Blooms, not the requested comparison across complete/partial/empty policies. The Forge fixture also writes empty Bloom sets, so compaction Bloom policy is not directly exercised there.

Close the promised evidence using the existing query telemetry/object-read and physical metadata helpers with a representative bounded fixture, reporting observed values without inventing latency/pruning thresholds or changing defaults. This is an acceptance/proof deficiency, not a claim that pruning is currently incorrect or that a particular speedup is required.

Supplied final results include redux 919/919, scribe 35/35, V8 1/1, format/lints/dependency/tenant/codegen passes at the reported implementation commit. These results do not exercise the three failing paths above. No Cargo or database commands were rerun. Source review did not change authorization, publication fences, retention, constants, or tests.
