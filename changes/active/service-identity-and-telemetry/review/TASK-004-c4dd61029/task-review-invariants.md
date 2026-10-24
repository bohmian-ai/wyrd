# TASK-004 independent invariant review

Result: **FAIL**.

Subject: repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`; cumulative base `3e7c59db7` to candidate `c4dd61029a54c44017c63bd4ff469472c406bc13`. HEAD was checked against the candidate. Reviewed source was not changed. No other review report was read.

Authority: original TASK-004; approved specification revision 12 (especially REQ-036–038 and AC-012–013), storage supplement sections “One layout contract and tenant configuration”, “Supported layout types and schema edits”, “Shared writer and maintenance policy”, and “Durable lifecycle and evolution”; root/nested repository rules; spec-driven development; maintainer and agent guides; Bifrost table identity, storage, Scribe, Oracle, Forge and failure invariants; security peer and Postgres tenant binding. Public alteration orchestration and cache propagation remain TASK-005's scope. The task explicitly requires the retained physical capability underneath those edits and permits direct catalog-seam evolution proofs here.

This is a static audit. Recorded green verification is supplied evidence, not an independently reproduced result. No Cargo command was run. CodeGraph is absent.

## Expanded ownership and state map

| Producer / authority | Carried state and consumers inspected | Assessment |
| --- | --- | --- |
| `PhysicalLayout::resolve`, registration and physical metadata | Wire declarations → one bounded validator → Iceberg schema/spec/order; Rust/Python/TS layout consumer changes | Existing owner extended; no parallel registration engine. Defaults and supported partition/Bloom types are explicit. |
| `PartitionScheme`, `PhysicalPartition`, `SealKey` | Iceberg transforms → typed null-aware tuple → binary/wire codecs, WAL v7, memtable buckets, staged keys, file-list rows and promotion record | Core tuple propagation is coherent; complete-request fanout and retry selection have the gaps below. |
| `ScribeImpl::resolve_logical_frame` | Registration fingerprint + committed Iceberg revision → stamped Arrow → partition planning → logical slice digest → committed-batch shortcut / durable SQL fence | All ingress uses current metadata before resolving an existing accepted batch. This is the producer of INV-R1, rather than a defect in the fence comparison. |
| `NativeSliceProducer` / `prepare_rows` | One native frame's multiple IPC batches → per-batch partition plans → accumulated slices → shard WAL | No request-wide distinct-tuple union; INV-R2. |
| `BifrostCatalog::{committed_revision,retained_layout}` and `resolve_write_recipe` | Retained schema/spec/order IDs + current Bloom property → persistence writer; staged restore recomputes assembly key | Reuse is appropriate, but current-only Bloom state contradicts the stored fingerprint on restart; INV-R3. |
| Staged source and publication ownership | `member_stager`, `hot_stage`, `assembly`, claim assembly/merge/publication, `staging_runtime`, `hot_source`, `file_list_writer`, `staging`, shard replay/publication lookup | Existing generation/fence/lease owners are preserved. Complete dimensions now reach the recovery lookup, not only output metadata. Generation ordinals are allocated across a memtable owner, so omission of revision in the capture's `(partition, generation)` comparison is not itself a demonstrated collision. |
| Oracle and field-ID projection | `memtable::{ReadableSource::project,projection_indices}`, `tail_rpc`, follower required fields, published scan schema selection, DataFusion fork `1eab549` | Current logical names resolve old source fields by stable IDs; nullable additions produce nulls. Authenticated source/capture ownership remains established before reads. |
| Forge promotion and compaction | Promotion retained spec/order lookup, footer schema matching, Iceberg append `486273c`; managed policy → pinned core file grouping and executor | Promotion now accepts retained specs and writes separate manifests; core grouping still erases spec identity (INV-R4). |
| Writer policy | `ParquetBatchEncoder`, `sort_batch`, shared writer properties; system table default declarations | Existing encoder and sort/merge owners are reused. Bloom columns have no hidden mandatory floor. No new compression, retention, TTL or tuning mechanism found. |
| Verification | V1–V9 source assertions and implementation evidence; revised mismatch mutations and recorded diagnoses | Tests establish useful healthy paths, but V4 is single-IPC-batch, V6 is sort-only, V7/V8 keep time transform and use noncolliding spec shapes, and no Bloom-change staged-recovery proof closes the identified paths. |

External sources were read at the pinned checkout commits under `/home/thorrester/.cargo/git/checkouts/`: iceberg-rust `486273c`, iceberg-compaction `efb39ce`, and DataFusion `1eab549`. The compaction bump from `8cc7bc5` to `efb39ce` changes dependency resolution only; it does not alter the grouping code cited below. I checked the append validation/manifest change and field-ID adapter change rather than treating version pins as proof.

## Acceptance matrix

PASS below means source plus the supplied verification supports the bounded obligation; it does not claim independent execution.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-036/047–048; bounded hour/day + at most two eligible dimensions, scalar sort and complete Bloom sets | `catalog/layout.rs::{resolve,canonical_time_column,canonical_partition_by,canonical_sort_keys,canonical_bloom_columns}`; public `partition_by` shape | V1 matrix; V2 create/describe/refusal | PASS |
| Same declaration idempotent; changed declaration does not implicitly alter registration | Existing `BifrostCatalog` registration/physical validation extended over all partition fields | V2 | PASS |
| REQ-037; null/type/spec distinct tuple survives codecs, WAL and stage | `catalog/physical_partition.rs`; `SealKey`; v7 slice payload; full dimensions in file-list insertion/replay matching and shard publication recovery lookup | V3; V5 | PASS |
| REQ-036; 32 complete tuples succeeds, 33 refuses entire native request before WAL/fence | `NativeSliceProducer::next_slice` and `prepare_rows` only bound each decoded IPC batch | V4 has one RecordBatch and misses multiple-batch bypass; INV-R2 | FAIL |
| REQ-024–025/035/037; active/staged/published/peer rows, origin, tenant separation | Existing stamping retained; complete partition key and field-ID projection in live/historical reads; peer/source authority unchanged | V5 lifecycle and V7 foreign-tenant refusal | PASS |
| REQ-038; new admission uses selected committed schema/spec/sort IDs; accepted rows seal under retained IDs | `committed_revision`, `PartitionScheme::with_revision`, WAL revision, staged key revision, `resolve_write_recipe` | V6 sort-order metadata checks; V7 old/new physical specs | PASS |
| REQ-037–038; retry preserves original acceptance across partition/schema alteration, restart/publication | Current revision is resolved before logical retry comparison; fence identity binds the newly generated slice set | V6 sort-only case does not falsify this; INV-R1 | FAIL |
| AC-013; stable IDs on rename, nullable additions read null, historical order claims remain truthful | Retained schema/order lookup; Arrow field-ID live projection; DataFusion field-ID adapter; promotion retained footer schemas | V7, V8, production pin test | PASS |
| REQ-038; legitimate Bloom-only metadata change cannot invalidate staged accepted work at restart | Current property is read for retained recipes, but assembly fingerprints include the old complete Bloom list | No corresponding restart case; INV-R3 | FAIL |
| AC-013; pins protect retained snapshot/schema/files | Existing pin owner and retained Iceberg scan planning preserved | Production pin across schema/spec revisions; supplied snapshot/fence regressions | PASS |
| REQ-035/038; Forge groups by complete physical partition including spec | `ForgeTaskPlanning` delegates to core `PartitionKey(Struct)` without spec ID | V8 changes tuple arity and does not cover equal-valued tuples under different specs; INV-R4 | FAIL |
| REQ-038; promotion accepts retained specs and files keep honest spec/order | `forge/scribe_promotion.rs::validate_promoted_object`; promoted footer retained-schema matching; Iceberg per-spec manifest append | V8 and promotion mismatch regressions | PASS |
| REQ-042/044; sorting, exact complete/partial/empty Bloom sets, missing filters remain readable | Existing encoder sorted candidates and declared per-column writer policy; no floor inserted | V9 actual Parquet order/Bloom assertions; mixed-spec exact query cases | PASS |
| Existing encoding/group/page/compression/offset/compaction targets remain | Existing writer property owner retained; V9 asserts writer recipe | V9 plus V8 rows/plan observations | PASS |
| Physical cost evidence requested by AC-008/task scenarios | Evidence records plan counts and row geometry; no numeric latency threshold added | No complete independent size/pruning/overhead measurement was run in this review | PASS for preserving policy; supplied measurement evidence remains limited |
| INV-001–002/006–008; tenant/auth separation, server-owned durable decisions, origin/typed data | Tenant-qualified bindings, existing authenticated peer/cut path, managed field stamping and existing SQL fence owner retained | V5/V7 and supplied tenant/fence/recovery suites | PASS apart from the separately identified lifecycle failures |
| No automatic historical rewrite, new row TTL, second engine/transport/cache/ledger or tuning | Extensions use existing catalog, WAL, staged owner, Oracle, Forge and encoder; no competing orchestration introduced | Cumulative source comparison | PASS |
| TASK-005 owns PATCH/CAS/cache invalidation; TASK-003 owns system inventory; later SDK task owns full language journeys | No public edit endpoint added; catalog-seam helper commits explicit revisions for physical tests | Task boundary and test sources | PASS |

## Proposed findings

### INV-R1 — INCORRECT: accepted-batch retry is interpreted under the new partition/schema before deduplication

Obligation: REQ-037–038, AC-013, task scenario 3: “resolve accepted batches before current-write rules”; original acceptance/binding must persist across alteration, restart and publication.

Locations: `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:225–235,404,590–599`; `scribe/preprocess.rs:149–169,698–710,897–910`; `scribe/shards.rs:1129–1142`. Sink: `crates/vala/vala-sql/src/queries/scribe_batch_commits.rs::ExistingScribeBatchCommit::matches_logical` compares `slice_set_digest` and `slice_count`.

Reachable trace:

1. Accept one batch containing two customers in one hour under time-only partitioning: one logical slice is fenced.
2. Commit a supported spec adding customer identity (through the same catalog seam already used for runtime evolution) and invalidate the current cache.
3. Resend the identical payload and batch ID. Ingress unconditionally obtains the current scheme, stamps using the current schema, and preprocesses the payload before asking the committed-batch shortcut.
4. The resend becomes two logical slices. `PreparedAppend::logical_identity` folds the new count/ordinals and per-slice schema/digest. It cannot match the original one-slice logical identity; the shortcut misses and the durable fence refuses the honest retry as contradictory. A schema rename/addition can similarly fail during current contract stamping before deduplication.

This is not a request to weaken the conflict check. Its producer now receives current-layout slices for a previously accepted identity. Siblings checked: native and projected preprocessing, in-memory committed index, shard fence/reconciliation, and WAL replay. None supplies the original accepted layout to ingress. The durable SQL fence holds logical slice identity and WAL coordinates, not an ingress-resolvable retained schema/spec/order binding. V6 changes only sort order, preserving slice shape and user schema, so it cannot expose this failure.

Observable consequence: an acknowledged batch cannot be safely retried after a supported partition/schema alteration, including after uncertain responses or restart; the advertised idempotency contract breaks despite unchanged rows.

Required testable correction: resolve a known accepted batch under its original retained contract before applying current schema/partition admission. Extend the existing accepted-batch/fence/catalog ownership as needed; keep changed-payload refusal and preserve WAL/fence identity checks. Fresh batches still use current committed metadata. Add a focused retry case where the same rows split into a different number of current partitions, plus rename/addition retry and restart coverage. Do not add another acceptance ledger or downstream bypass.

### INV-R2 — INCORRECT: a native frame bypasses the 32-tuple limit by carrying multiple IPC batches

Obligation: REQ-036, AC-012/021; architecture Scribe requires at most 32 distinct complete tuples per request, refused before WAL/fence.

Locations: `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:340–349,698–710`; `scribe/seal_key.rs::plan_partitions`; sibling `scribe/material_plan.rs:150–163` only checks native wire bytes.

Reachable trace: encode 17 distinct customer identities in IPC RecordBatch A and 16 different identities in RecordBatch B, all at one hour, in one native Arrow stream and one append/batch ID. Each call to `plan_partitions` creates a new local tuple set and passes its own limit. `prepare_rows` accumulates the resulting 33 slices, assigns ordinals and returns them for WAL; it never unions distinct tuples across the request. Sending one batch with 33 fails, but splitting the same request into two IPC batches succeeds. Repeated tuples must count once across IPC boundaries; simply limiting total slice count would incorrectly reject valid repeated partitions.

Siblings checked: projected path has one already projected RecordBatch; native transport material planning does not inspect tuple counts; shard processing does not establish the request union before append. V4's helper calls `encode_ipc` for exactly one RecordBatch, leaving this route untested.

Observable consequence: public native requests exceed the approved fanout bound and become durable/visible; callers control the bypass solely through legal Arrow stream framing.

Required testable correction: enforce one bounded distinct complete-tuple set for the entire admitted request in the existing preprocessing owner before returning a prepared append to WAL. Reuse `PhysicalPartition` equality, the existing 32 constant and existing refusal. Prove 17+16 disjoint tuples refuses with no fence/WAL/rows, 16+16 passes, and more than 32 slices repeating at most 32 tuples passes. No extra parse pass, transport or validator is required.

### INV-R3 — REGRESSION: a Bloom change makes valid staged work unrecoverable

Obligation: REQ-038, AC-013/016; metadata changes preserve accepted work and restart recovery; Bloom-only edits are supported and must not invalidate retained files/layouts.

Locations: `crates/vala/vala-bifrost-redux/src/catalog/layout.rs:1007–1014`; `scribe/assembly.rs:275–279`; `scribe/staging_runtime.rs:825–846`. Shared caller owner: `scribe/write_recipe.rs:64–73` → `BifrostCatalog::retained_layout`.

Reachable trace:

1. Accept and stage a member under Bloom set `[customer]`, below publication target. The durable staged record stores an assembly key whose `layout_fingerprint` includes that complete list.
2. Commit a legitimate Bloom-only metadata change to `[]` through the catalog/property seam. No old object rewrite is required or requested.
3. Restart from the retained data root. `restore_context` loads the run schema and calls the retained-layout resolver with the stored schema/order/spec IDs.
4. `PhysicalLayout::of_iceberg` reads the table's **current** Bloom property, yielding `[]`. Rebuilding `ScribeAssemblyKey` now yields another fingerprint, and recovery returns “the registered write recipe no longer reproduces a recovered staged key”. The stored run was valid and its field IDs/checksum did not change.

Reordering the Bloom list also changes the fingerprint even though membership semantics are order-independent; an ID-preserving rename of a Bloom source can drop the historical name when the current property is narrowed to the old schema. These share the same producer: current-only configuration substituted for retained recipe state.

Siblings checked: live persistence and staged restore both use `resolve_write_recipe`; committed revision currently carries schema/order only, spec is carried separately; `StagedHotSourceRecordV1` carries revision IDs and a fingerprint, not a reconstructible retained Bloom recipe. The task's V6 changes sort order and V7 uses empty Bloom lists, so neither exercises this. This finding is about the required physical revision/recovery capability; it does not ask TASK-004 to implement the public PATCH endpoint or distributed edit-cache orchestration.

Observable consequence: valid acknowledged rows with local staged authority prevent successful recovery after an authorized metadata change; accepted work cannot resume without reverting configuration or manually changing durable evidence.

Required testable correction: make the retained recipe and durable staged identity agree across supported Bloom changes, using the existing catalog/staged ownership. Preserve original accepted layout as required rather than reconstructing it from current property state. Keep corruption/fingerprint validation and do not paper over the mismatch at startup. Prove a staged member survives clear/replace/reorder and a Bloom-source rename, restarts, publishes once, and remains exactly readable. No new lifecycle or acceptance ledger is justified.

### INV-R4 — INCORRECT: pinned compaction grouping erases the partition-spec component

Obligation: REQ-037–038, task scenario 4 and architecture Forge: complete physical partition includes spec ID; compaction must not cross physical partitions to meet a target.

Location in changed dependency contract: root `Cargo.toml:260` pins `efb39ce4d03c79c27e005b193bd235c9403af7e3`; actual owner `/home/thorrester/.cargo/git/checkouts/iceberg-compaction-3b4eb9269abd88f3/efb39ce/core/src/file_selection/strategy.rs:1176–1200` defines `PartitionKey(iceberg::spec::Struct)` and groups only on the tuple. Production callers: `crates/vala/vala-bifrost-redux/src/forge/managed/policy.rs:441–444,480–490`; core `GroupingStrategyEnum::group_files` and `PlanStrategy` partition scope.

Reachable trace: retain a spec using `hour(event_time), bucket(4, nullable_customer)`, then commit one using `bucket(8, nullable_customer)`. Old/new null customer partitions have equal typed tuple values but different spec IDs. The scan retains each task's source spec; `group_files_by_partition` discards it and uses only `file.partition`. These tasks can therefore enter one group/rewrite plan under SmallFiles/Full partition-scoped compaction. The same issue applies whenever different specs produce equal transformed values. There is no upstream per-spec split in `FileSelector::scan_data_files` or `ForgeTaskPlanning::plan`.

The current mixed-spec Forge test drops the shard dimension, producing different tuple arities; it does not establish separation for colliding tuples under distinct specs. Pinning a newer dependency does not close the gap: the compaction revision range changes Cargo dependency resolution only.

Observable consequence: distinct approved physical partitions become one compaction input group, violating the full-identity boundary and allowing cross-spec consolidation to satisfy group/file targets. This finding does not assert lost row data; the partition isolation contract itself is explicit.

Required testable correction: extend the existing compaction-core partition grouping identity to include each task's retained spec ID alongside its typed tuple, then pin that tested correction. Reuse the core owner; do not add Wyrd-side grouping orchestration. Preserve table-scoped delete/rewrite semantics where explicitly selected. Prove equal-valued/null tuples under two retained specs yield separate partition-scoped groups and that normal within-spec compaction still proceeds.

## Reuse, diagnosis and verification limits

The added `PhysicalPartition` and `PartitionScheme` have coherent ownership and reuse Iceberg transforms rather than inventing bucket/truncate behavior. `LayoutRevision` extends existing WAL/staging/catalog flows and does not create a second storage engine. The actionable gaps are at those shared producer/owner boundaries, not a need for more leaf guards.

The recorded V8 first failure is consistent with the inspected Iceberg default-only validation and the per-spec append correction. The V8 test-geometry change is explained by the two-file minimum. V9 Bloom reader setup and DataFusion fingerprint pin changes are consistent with their owning source. The promotion corruption mutations now target the nested `partition.time.start_utc` and `partition.spec_id`; that diagnosis does not indicate a waived production check. No unsupported timeout/skip/assertion weakening is proposed here.

Available evidence claims V1–V9 and 919 redux / 35 scribe journeys pass, plus format/lints/dependency/tenant/codegen checks. Those results do not exercise the four paths above. No prior task-review/remediation report exists in this checkout, so there are no prior IDs to close.

No implementation, merge, commit or push was performed. No independent runtime or measurement checks were run; the principal remaining risk is that the identified lifecycle/fanout paths are not covered by the supplied green tests.
