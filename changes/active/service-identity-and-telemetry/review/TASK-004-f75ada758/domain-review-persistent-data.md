# Persistent-data and durability review

Overall result: **FAIL**.

Subject: immutable cumulative `3e7c59db7a0f5e9506273eab826875cc8b59f226...f75ada758827266ac0b25a05d6a7b530f86d845a`; remediation locator `c4dd61029...f75ada758`. Scope is definition identity and retention from catalog publication through WAL/staging, Scribe publication, Forge promotion/rewrites and cleanup. The approved verifier-response-data integration is included; its unrelated completion is not an acceptance requirement here. No source, dependency, benchmark, or test was modified or executed.

## Authority and coverage

| Authority | Applied boundary |
| --- | --- |
| Root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/index.md`, `agent_docs/maintainer-style.md` | Existing owners, actual source evidence, immutable cumulative acceptance and persistent-contract obligations. No nested AGENTS exists under the reviewed Vala/testing source directories. |
| Approved spec revision 13, REQ-035–038; single table version contract at `spec.md:436`; AC-013/016/021/022 | One complete immutable definition, exact representation, atomic native publication, original-version retries and conservative retention. |
| `supplements/table-storage-and-correlation.md:40–113,650–737` | Definition contents/default resolution, expected-version/base-pointer publication, required version carriers, WAL/fence/stage/promotion/query roots and separate snapshot retention. |
| Original TASK-004, supplemented by R2's replacement for superseded recipe mechanics | Supported internal catalog evolution; no public PATCH requirement; complete tuple/fanout/retained source/physical output requirements. |
| `architecture/bifrost-design.md:107–164,639–722,1250–1302,1362–1408,1441` | Catalog definition ownership, staged and promotion evidence, retained-definition promotion, output version identity, revalidation and cleanup. |
| Prior verdict, validated FIND-TASK-004-1 through -7, R1 and R2 | Closure checked against current owners, preserving approved revision-13 changes rather than requiring copied recipes. |

| Source boundary inspected | Assessment |
| --- | --- |
| `wyrd-spec/src/vala/{ids,api}.rs`, catalog `layout.rs`, `physical_partition.rs`, `bifrost_catalog.rs`, `iceberg_sql.rs` | Positive signed-64-bit version and canonical string representation; complete typed spec-qualified partitions; coherent schema/layout/version descriptors; historical resolution uses retained native metadata rather than current Blooms. Settings/default completeness fails DATA-R2-2 below. |
| Actual pinned Iceberg SQL catalog `a34f3885c`, `crates/catalog/sql/src/catalog.rs` and catalog capability extension | Creation writes pointer plus version-1 reference in one SQL transaction. Updates check requested next version, apply native requirements, CAS the base pointer and insert the reference within that transaction. Losing CAS rolls back; uncertain COMMIT recognizes its exact published pointer before success. References include metadata table UUID and tenant-bound catalog namespace/table. No Wyrd-side second allocator or per-batch version-reference SQL exists. Current definition reference survives data-only pointer movement. |
| `scribe/{ingress,preprocess,committed_batches,seal_key,wal,replay,shards}.rs`; `vala-sql` migration and batch-fence query owner | Descriptor/version selection reaches slices and seals; WAL-v7 encodes eight-byte positive versions and authenticates slice sets; SQL fences preserve the accepting version and canonical attempt coordinates. Stale failures use exceptional accepted-ID resolution, warm mirror hits precede current preparation, and replay suppresses noncanonical spent-ID attempts. Multi-row tenant fencing retains rollback and uncertain-result reconciliation. |
| `scribe/{assembly,hot_stage,member_stager,staging_runtime,staging,claim_assembly,claim_merge,claim_publication,write_recipe,persistence}.rs` | Assembly keys carry table version; staged restoration reconstructs through retained definitions; matching schema/spec is checked. Noncreation versions without a catalog owner refuse. No current-Bloom reconstruction of a historical recipe remains. |
| `scribe/{file_list_writer,promotion}.rs`, `forge/scribe_promotion.rs`, `parquet/promoted_object.rs` | Full durable SQL versus JSON partition comparison closes the prior pruning corruption. However, the version is dropped when constructing promotion evidence; see DATA-R2-1. |
| `scribe/{memtable,hot_source,tail_rpc}.rs`, `oracle/{exec,follower}.rs`, catalog pinned reads and retained-schema scan seam | Complete SealKey plus generation survives served/superseded capture. Stable field IDs govern projection; historical missing columns become null. Time pruning remains conservative across complete physical identities. |
| `forge/{compact,expire,orphan_gc,publication,worker}.rs`, `forge/managed/{policy,executor,handoff,fingerprint}.rs` | Every version-reference location is protected, including metadata-log rollover; failure to acquire references refuses cleanup. Retained definitions do not recursively retain all data snapshots. Managed rewrite definition/output propagation is incomplete (DATA-R2-1); writer defaults remain deployment-dependent (DATA-R2-2). |
| Actual pinned compaction core `1ade704`, `core/src/file_selection/strategy.rs` plus execution/request consumers | `PartitionKey(Option<i32>, Struct)` includes native spec identity and typed tuple. Partition grouping no longer aliases equal tuples across specs; explicitly table-scoped delete work remains unchanged. |
| Source proof inspected | Multi-RecordBatch fanout journey, accepted-batch alteration/restart journey, mixed-spec promotion/compaction journey, durable-partition corruption case, full-key capture unit cases, WAL version codec, catalog fork tests, retained-definition expiry test and `distributed::bloom_policy_versions_answer_exactly_through_compaction`. Supplied results are execution evidence, not fresh reruns. |

## Prior finding dispositions

| Prior finding | Disposition from actual source |
| --- | --- |
| FIND-TASK-004-1 | **Partially closed; remains open.** Complete historical schema/spec/sort/Blooms resolve through the retained native reference for admission/sealing/staged restoration. Promotion and rewrite ownership still drop the version, and omitted writer defaults remain mutable; findings below. |
| FIND-TASK-004-2 | Closed in this domain: `NativeSliceProducer::partitions` persists across decoded RecordBatches and `PartitionPlan::union_into` runs before complete preparation reaches WAL. Repeated tuples count once. |
| FIND-TASK-004-3 | Closed: actual pinned core groups by spec plus typed tuple (`strategy.rs:1182–1205`), rather than tuple alone. |
| FIND-TASK-004-4 | Closed: worker revalidation calls `validate_durable_partition` before catalog append; SQL time/dimension bytes are decoded and compared with the complete JSON partition. Object/footer/checksum checks remain. |
| FIND-TASK-004-5 | Closed: memory/staged membership uses `(SealKey, GenerationOrdinal)` throughout served/superseded capture; table version is part of SealKey. |
| FIND-TASK-004-6 | Rustdoc-wide closure belongs to the standards reviewer. No separate documentation finding is proposed by this domain review. |
| FIND-TASK-004-7 | Domain proof gap closed: the new Oracle journey checks derived exact row identities across complete/partial/empty/reordered Bloom policies and hot/promoted/rewritten cuts, and inspects physical filters/geometry. Recorded measurements include query pruning and physical bytes. Throughput remains explicitly unmeasured as allowed by current authority. |

## Proposed findings

### DATA-R2-1 — MISSING: the accepted version stops at Scribe publication and is absent from Forge rewrite authority

**Prior issue:** incomplete closure of FIND-TASK-004-1; no new stable FIND ID assigned here.

**Violated obligation:** REQ-037; R2 carrier table requires version-bearing promotion evidence and a selected rewrite output version. Architecture `bifrost-design.md:150–155,696–698,1254–1259,1296–1301,1362–1389,1406` explicitly requires these persistent owners to preserve and resolve that identity.

**Locations and producer-to-consumer evidence:**

- `scribe/file_list_writer.rs:231` (`build_claim_inserts`) receives the `ScribeAssemblyKey`, which has `table_version()`, but `PublishedArtifactFacts` and `PublishedHotFileIdentity` omit it (`file_list_writer.rs:254–282,363–376`; `promotion.rs:174–195`).
- The persisted `ScribePublishedHotFileV1` (`promotion.rs:211–243`) has record-format version, schema fingerprint, sort ID and physical tuple/spec, but no table version. A Bloom-only edit leaves all those definition identifiers unchanged. `Forge::read_promotion_demand` and `validate_promoted_object` (`forge/scribe_promotion.rs:329–423,488–555`) consequently cannot resolve the accepted version: they inspect schemas/specs/orders retained by the current metadata and pick a matching schema rather than acquiring the original definition.
- The same downstream gap persists in `ForgeTablePolicy` (`forge/managed/policy.rs:105–124,184–195`), `RewriteHandoff` (`managed/handoff.rs:24–39`) and `RewritePublication` (`worker.rs:89,6476`). They retain native schema/spec/sort and/or a constant writer recipe, never the selected table version. `catalog/layout.rs:544–565` still generates `/data/forge/v2`, despite the version-bearing output contract.
- On definite catalog conflict, `worker.rs:7138–7177` reloads metadata and calls `rewrite_authority`; `worker.rs:7234–7240` only checks planning snapshot retention and schema ID. `publication.rs:790–806` expressly excludes spec/sort from this check; there is no complete-definition/version revalidation or version recorded with the resulting output/snapshot evidence.

**Observable consequence:** Publish real files accepted under version 1, commit a Bloom-only version 2, then promote or rewrite them: the persistent promotion/output evidence cannot say which complete definition produced each file, since native schema/spec/sort are identical and every rewrite path says `v2`. This is loss of required durable lineage on an existing production path, not a request for TASK-005 PATCH. A rewrite interrupted by a definite conflict can reload after such an edit and proceed on schema equality without preserving or checking its selected output definition. Retaining all metadata prevents immediate collection, but does not reconstruct an unrecorded version or satisfy the exact-definition contradiction boundary.

**Smallest correction boundary:** Complete the existing version propagation at `build_claim_inserts` rather than infer it later. Persist the assembly key's version alongside the current exact DataFile evidence; resolve it in the existing promotion validator through the native retained-definition owner, preserving the durable-partition/footer/checksum checks. Carry the selected output version through existing Forge policy/handoff/publication/lineage and the native core's existing output identity mechanism. Replace constant recipe-as-definition identity with the approved version segment, and revalidate the selected definition at existing initial/retry publication boundaries. Keep grouping/execution in the native core and preserve lease, delete, snapshot visibility, retry and cleanup behavior. No new history service or ingest SQL is needed.

**Focused closure proof:** Use the supported internal catalog seam to produce two Bloom-only versions with identical native schema/spec/sort IDs; stage and promote the older file and assert its durable promotion evidence still names/resolves version 1. Rewrite at a selected version and assert output paths and committed lineage name exactly that version. Introduce a definition edit between execution and a definite-conflict retry and prove that output assumptions are retained/revalidated rather than accepted solely on unchanged schema. Contradictory version versus native spec/order/definition must refuse before append; intact historical files still publish unchanged.

### DATA-R2-2 — INCORRECT: an omitted writer default is not part of the immutable definition

**Prior issue:** incomplete closure of FIND-TASK-004-1's complete-definition invariant.

**Violated obligation:** spec single-version contract (`spec.md:440–458`), supplement `:84–88`, and architecture `bifrost-design.md:1287–1290`: all writer/layout settings and resolved defaults belong to the committed definition; a deployment default cannot reinterpret an existing version.

**Exact source and reachability:** `BifrostCatalog::create_physical_table` (`catalog/bifrost_catalog.rs:1185–1238`) explicitly writes nothing for omitted compaction settings. `CompactionRegistration::write_properties` (`:2017–2031`) inserts `write.target-file-size-bytes` only for `Some`. `ForgeTablePolicy::extract` (`forge/managed/policy.rs:153–158`) substitutes `config.default_target_file_size_bytes` when the native property is absent. That value is deployment configuration (`wyrd-server/src/boot/mod.rs:288–290`), not the retained definition. Default table creation is a live supported registration path, not an invalid native metadata mutation.

**Observable consequence:** Create a default table under deployment target 1 GiB, then restart Forge with target 2 GiB. The same metadata, table version and retained definition now select different file geometry and small-file thresholds. Two workers with different deployment defaults can likewise interpret the same version differently. The source therefore falsifies the promised immutable complete definition even when every versioned edit is correctly committed.

**Smallest correction boundary:** Resolve the authorized default at the existing creation owner and persist it with the version-1 native definition; later effective edits use the same versioned metadata publication owner. Forge reads the committed value rather than applying its current deployment default to a registered version. Keep public omission/default behavior and the existing geometry bounds; do not add a second configuration revision or per-batch lookup. Apply this rule to the existing writer/layout settings the version claims to own, without adding settings.

**Focused closure proof:** Create an omitted-target table, assert the resolved property is in the native version-1 metadata, and extract Forge policy under two different deployment defaults. Both must yield the original committed target and thresholds. A newly created table after an authorized default change may resolve a different target; the prior table must remain unchanged.

## Verification limits and calibration

Static source review only. No Cargo, Postgres, SDK, benchmark or external service command ran. Fork source was inspected from local Cargo checkouts at actual immutable pins `a34f3885c` and `1ade704`; their recorded tests were not rerun. No performance regression is alleged. Public edit orchestration, cross-replica cache refresh, full language-surface acceptance and broad rustdoc coverage remain with their owning task/review lanes. No additional finding is proposed merely because arbitrary native property calls could omit a version: supported versioned edit callers were traced, and speculative misuse is excluded.

Skipped broad verification reruns and benchmarks. Material remaining risk: published/rewritten file evidence loses complete-definition identity, and a deployment setting can change writer behavior within one supposedly immutable version.
