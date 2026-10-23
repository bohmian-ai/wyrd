---
id: TASK-004-R2
kind: remediation
status: proposed
spec: SPEC-service-identity-and-telemetry
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7]
requirements: [REQ-035, REQ-036, REQ-037, REQ-038, REQ-042, REQ-044, AC-008, AC-012, AC-013, AC-016, AC-018, AC-021, AC-022, AC-023]
depends_on: []
---

# Bind every Bifrost tier to one retained table version

Replace the incomplete composite layout identity and copied accepted-layout JSON with one positive integer, called `table_version` throughout this proposal. Approved specification revision 13 fixes the canonical public spelling and exact representation below. A version identifies the complete immutable definition under which Bifrost accepts, writes, recovers and publishes a batch. Resolve that definition from a retained native Iceberg metadata file. For example, clearing a table's Bloom list creates a new version; already staged data still resolves the old version and publishes under its original policy. Retrying an accepted batch after a partition change returns the original acceptance rather than comparing geometry produced under the new partition spec.

This is the single active remediation task for TASK-004 and supersedes [R1](TASK-004-R1-retained-storage-authority.md). Findings 2–6 are already implemented and remain closed as recorded below. Finding 1 uses the version model instead of R1's recipe transport; finding 7 remains open. The owner accepted the version direction, not a revised specification or completed implementation.

**Authority: approved specification revision 13.** Revision 12 remains the historical review baseline; revision 13 records the new public/persisted contract. This task stays proposed until readiness review, and TASK-004's older composite/recipe text must be aligned through `$wyrd-plan` before execution. There is no dependency on superseded R1 and no dependency on TASK-005 that would create a cycle. TASK-004 supplies the substrate; TASK-005 exposes PATCH and committed-edit refresh using it.

The owner requires preserving throughput and **not adding PostgreSQL to the hot path**. The existing pre-ACK batch fence remains. Versioning adds no normal-ingest SQL call or transaction; the fence optimization reduces statements inside the existing transactions. Replacing PostgreSQL fencing is out of scope. Throughput remains unmeasured until the benchmark run described below; a regression beyond 5% fails the task.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`, branch `task/service-identity-and-telemetry/TASK-004`.
- Original reviewed base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`; reviewed candidate: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- Source citations in this task were checked against current HEAD `3ab558f129a1e6959de520595c2b18aabb3cfbaa`. Dependency citations explicitly name their pinned revision. Historical RED/GREEN results below are carried evidence, not executions performed while writing R2. No `.codegraph/` directory exists at this HEAD.
- Packet authority: [approved spec revision 13](../../spec.md), [table-storage supplement](../../supplements/table-storage-and-correlation.md), [TASK-004](../../tasks/TASK-004-shared-table-storage.md), [TASK-005](../../tasks/TASK-005-tenant-table-configuration.md), [verdict](verdict.md), [validated findings](findings-validation.md) and [follow-up diagnosis](followup-review.md). Immutable review reports remain historical evidence; R2 replaces their selected remedy where it conflicts with the version decision.
- Stronger authority: root/applicable `AGENTS.md`, `architecture/README.md`, Wyrd doctrine/client model, Bifrost identity/Scribe/Oracle/Forge and recovery sections, security tenant/Postgres/peer rules, and `agent_docs/{rust,errors,maintainer-style,agent-surfaces}.md`. Commands run through mise; Cargo-backed verification runs sequentially.

## Diagnosis and selected corrections

### 1. Replace accepted recipes and composite revisions with one table version

**FIND-TASK-004-1; REQ-035/037/038/042, AC-013/016.** Schema, partition-spec and sort-order IDs do not identify a complete definition. `catalog/layout.rs:75` defines the two-field `LayoutRevision`; `catalog/layout.rs:627` declares `wyrd.bifrost.bloom-columns`; `catalog/layout.rs:1025` reconstructs a historical layout while reading current Bloom properties. A Bloom-only edit is invisible to the composite revision and can strand staged data. Current-definition preprocessing also changes accepted retry geometry: dropping a partition dimension merges the original slices, and stable-ID renames/additions can fail current-schema preparation before the accepted fence is consulted.

Commit `86cb0d3b8` carries `AcceptedRecipe` through seal/WAL/staging but not the SQL fence. The unmerged local branch `wip/f1-phase-b-binding` extends the fence with a recipe and retained-source comparison. That branch **must not be merged**. Its required retry/replay behavior is specified here using versions; its recipe binding and duplicate identity fields are superseded.

#### Definition, scope and representation

Use one positive integer `table_version`, starting at 1 for a newly created physical table and monotonically incremented once per successful effective definition edit. It is scoped to the existing immutable table identity, including authenticated tenant binding; a same-named replacement table cannot reuse the old table's authority. Do not assemble the version from schema/spec/order IDs or hashes. Do not introduce an independent configuration revision, writer revision, recipe version or accepted-recipe blob.

The definition includes columns, names, types, nullability, stable field IDs, time-column identity, partition transforms/spec, scalar sort direction/null order, Bloom columns and other settings that determine writer/layout behavior. Retained metadata must encode every such fact: native Iceberg fields where available, and Wyrd table properties for facts without a native representation. Do not reconstruct any historical part from a mutable SQL layout projection or current table property. The immutable time-column rule remains: inclusion in a version does not authorize changing it after creation.

Use `wyrd.bifrost.table-version` on the Iceberg table. Its value and all definition edits commit together in the same native metadata commit. Ordinary snapshot appends, visibility progress, statistics and housekeeping do not bump it. Reject invalid/ineffective edits without consuming a committed version; never wrap or reuse a committed version. Revision 13 fixes the domain as 1..=9223372036854775807: Rust i64, SQL BIGINT and protobuf int64, with eight-byte persisted fields. JSON versions are canonical unsigned decimal strings even for "1"; TypeScript SDK values are bigint with direct string conversion, and Python uses exact int. Reject numeric JSON, zero, signs, leading zeros, whitespace, fractions, exponents and overflow. Exhaustion refuses without wrapping or mutation. No JavaScript-number intermediate is allowed.

Revision 13 uses this same version in responses as `table_version` and in PATCH as `expected_table_version`; no configuration counter, old-name alias or second independently meaningful field remains.

Native Iceberg schema/spec/order IDs remain necessary descriptors inside retained definitions and `DataFile`/partition structures. They are not additional Bifrost versions. Iceberg snapshot IDs and Oracle publication revisions still describe visibility, a different fact; a table version must never select the snapshot to read or append to.

#### Atomic publication and retained native metadata references

Retain `(existing immutable table identity, table_version) -> native metadata location`. This is an address reference to one authoritative native metadata object, not a second representation of its schema/layout. Native metadata contains schemas, spec/order histories and custom properties; Iceberg has no native complete-definition-only epoch or WAL-aware retention of those metadata files. Snapshot refs and bounded `metadata-log` are insufficient substitutes.

**Exact existing ownership:** `BifrostCatalog` in `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs` owns `Arc<SqlCatalog>` (`:246`), constructs it through `iceberg_sql::build_catalog` (`:802`) and exposes its `iceberg_catalog` capability (`:838`). Extend `BifrostCatalog::create_table` (`:856`), `ensure_builtin` (`:956`), shared `create_table_locked` (`:1026`) and `create_physical_table` (`:1139`, dependency creation at `:1179`) rather than adding a configuration service. `BifrostCatalog::committed_revision` (`:1365`) is the present cache-fill boundary to replace with one version-bound descriptor.

The actual metadata-pointer CAS is **not currently implemented in Wyrd**. `Cargo.toml:250` pins Iceberg and its SQL catalog to `486273c9e0597611ed2f69ce8ab088f60cc88192`. At that dependency revision, `crates/catalog/sql/src/catalog.rs:1041`, `SqlCatalog::update_table`, loads current metadata, applies the commit, writes the new metadata object and performs `UPDATE ... WHERE metadata_location = old_location`. At `:1056` it calls `execute` with no supplied transaction. `SqlCatalog::execute` (`:453`) then opens and commits a transaction for that single statement (`:470`); it currently discards the COMMIT result (`:472`). There is no existing multi-statement pointer/reference publication transaction or Wyrd callback. `crates/iceberg/src/catalog/mod.rs:406`, `TableCommit::apply`, applies updates and then allocates the resulting metadata location.

Extend that existing catalog publication boundary in the pinned Iceberg fork, then pin the immutable tested correction in Wyrd. Wyrd's `catalog/iceberg_sql.rs` construction boundary and `BifrostCatalog` consume it. The extension must:

1. Receive/assert the editor's expected table version and expected base metadata pointer before applying the validated complete post-edit definition. A stale Bloom-only edit must fail even when schema/spec/order IDs are unchanged. Ordinary native requirements and generic refresh/rebase are not sufficient property-CAS protection.
2. Build native metadata with version `r + 1` and the complete edits, write it to the allocated immutable metadata location, then publish the pointer CAS and retained version-location reference atomically in **one catalog SQL transaction**. Creation publishes version 1 and its reference at the same boundary. The reference relation/index belongs to this catalog capability; do not commit it afterward through a second `TenantConn`, database, outbox or eventual projection.
3. On zero-row pointer CAS, roll back the reference and return conflict. Losing/unpublished metadata objects are orphans, never committed definitions. Do not overwrite an existing version's reference. If a competing data-only commit moved the pointer, an internal retry may reload only after proving the expected definition version still matches; revalidate every edit assumption. A concurrent definition edit must return stale-version conflict, not silently merge or overwrite it.
4. Reconcile an uncertain catalog COMMIT by reading the authoritative pointer/reference and verifying the committed version/location before success. Cache invalidation follows proven commit, never a staged object write or failed transaction.
5. Preserve existing catalog tenant/namespace/table identity checks and named catalog-pool capability. The reference must be tenant-bound and unavailable to arbitrary client-supplied metadata URIs. Do not expose raw pools or widen tenant query contracts to reach the dependency's transaction.

This is cold DDL/catalog publication work. No catalog SQL reference allocation, version increment or independent version lookup runs per admitted batch. Extend the existing registration/current-descriptor cache fill to obtain the version and metadata reference together. Historical exceptional reads return the reference with the accepted fence or through the existing recovery/catalog acquisition owner, not an added normal-ingest query.

#### Retention and existing resolution owners

Reuse `BifrostCatalog`'s immutable metadata-by-location cache (`bifrost_catalog.rs:258`), `BifrostCatalog::materialize_cut` (`:511`), and Scribe's existing `resolve_write_recipe` boundary (`scribe/write_recipe.rs:51`) to turn a version into its native definition. Replace recipe resolution there; do not duplicate Scribe/Forge writer orchestration. Cache one immutable descriptor per retained table/version; extend existing bounded ownership/accounting rather than introducing another unbounded cache.

The live current table version is always a retention root, even when data-only commits have advanced the current metadata pointer and no batch/query references its original definition file. WAL, staged data, accepted retry fences, pending promotions and active query pins are additional roots. Register/protect the definition before acknowledging dependent work, and remove a root only after its durable owner no longer references it. Recover roots before Forge cleanup starts; a crash or a cold descriptor cache cannot make accepted work collectible. Protect both the retained reference and native metadata object. Keep snapshot/data-file roots separately: retaining definition metadata does not promise historical snapshot queryability or pin every file mentioned by that old metadata document.

Derive work roots from the existing version-bearing WAL/stage/fence/promotion evidence and query-pin owners, not a per-batch SQL root-registration round trip. The catalog version-reference allocation occurs once at definition publication; durable batch references use the existing fence transaction. Retain conservatively while an unfenced WAL attempt or unavailable role's retained storage cannot be proved settled. An absent in-memory cache entry is not proof that a root is gone.

Extend Forge's current `load_catalog_protection` (`forge/orphan_gc.rs:1274`) and `expiry_cleanup_candidates` (`forge/expire.rs:1575`) to honor these roots. Current protection covers current/native metadata-log locations; native history rollover or expiry must not delete a rooted definition. Do not enable native delete-after-commit behavior that bypasses these roots; metadata deletion remains with the existing root-aware cleanup owner. Coordinate root removal and deletion using the existing cleanup/pin/fence owners so a new durable reference cannot race with collection. Do not invent a parallel history/GC service.

Recommended default: prune only a noncurrent version after no durable WAL/stage/promotion reference or active pin remains **and** the existing accepted-batch retry-retention window has expired. A fence surviving WAL retirement keeps its original definition resolvable. If a retention horizon is not yet defined, retain rather than guess a TTL. Missing, malformed or contradictory referenced metadata fails closed; do not fall back to current layout. Historical schema names and stable IDs come from the referenced definition.

#### Client alignment and admission

Correct the earlier claim precisely. Clients already fetch schema/layout/UID/fingerprint through `TableConfig::describe_with`: `crates/shared/wyrd-client/src/bifrost/table.rs:177` and `:209`. Registration can also resolve a table. However, writer construction in `bifrost/facade.rs:399` retains FQN/schema and drops resolved identity (`:413`; `table.rs:429`). The outgoing `InsertBatchRequest` at `bifrost/grpc.rs:230` sends **only table, Arrow IPC, batch ID and producer trace contexts**. Native Gate sets `expected_schema_fingerprint: None` at `crates/vala/vala-bifrost-redux/src/gate/mod.rs:1034`; canonical ingestion does the same at `:957`. The internal optional fingerprint is not a public version handshake.

Describe/register must return the same table version with the schema, resolved layout and immutable table identity. Every writer retains that binding and sends it on each frame, including retries and buffered chunks. Implement transport and writer ownership once in `wyrd-client`; Rust, Python and TypeScript consume it through their existing facades. Update the owning wire contracts/protobuf, generated projections, exports and authored stubs as applicable. No SDK may allocate a version or silently describe-and-rebind an already queued batch. Ensure table recreation cannot redirect a writer that held the old identity.

Gate resolves one versioned server-owned projection for canonical OTLP/built-in records and carries the resulting version to Scribe. Canonical clients need no synthetic Arrow-schema describe handshake. Native Arrow ingestion, including writes to built-in tables where supported, supplies its retained version; it cannot bypass server-managed-column rules. Retrying canonical input must resolve the accepted projection/definition rather than reinterpreting the accepted rows under current layout.

New batches require the committed current version. Reject stale/unknown versions with a typed version conflict, no WAL/fence/row residue and remediation to refresh describe and create a new batch. TASK-005 owns committed-edit cache refresh across replicas; R2 must provide a coherent current version/descriptor so that task cannot mix one version's schema with another version's layout. Version rejection is not authorization and must not reveal another tenant's table.

Accepted retries are the exception to stale-new-write refusal. Use the existing bounded committed-batch mirror before preparation on a hit. Bind comparison to the fence's original version, acceptance instant, origin, digest/count and canonical WAL coordinates. An accepted ID alone never proves equality. Changed source schema, payload, row order or count still conflicts. Preserve all authentication, permission, frame-size and managed-column checks on every attempt.

On a cold mirror miss, do not add a preliminary accepted-ID SQL query. Preparation/WAL work is provisional. The existing fence conflict read must return the original version/reference and acceptance facts; compare the retained source under that version using the existing bounded CPU owner outside the SQL transaction. If current-contract preparation or stale-version validation fails before the fence, use the R1 exceptional tenant-scoped accepted-ID recovery path only for that failure. An unknown ID preserves the original refusal. That exceptional path is not a new query on successful fresh ingest. Retain source ownership under current admission accounting without an additional whole-payload copy/hash pass on all requests.

Preserve tenant-group rollback: if provisional digest/count/version comparison contradicts an accepted row, roll back every still-uncommitted fresh fence in that tenant group before retained-binding comparison. Only a proven identical retry permits resuming the same group's fencing; a genuine contradiction commits none of its fresh fences. Cancellation cannot transfer durable completion from the shard to a response waiter.

#### Carriers, codecs and durable fence

Replace Bifrost composite definition identity with the same `table_version` in every carrier below. Native physical partition/spec fields, payload checksums and exact `DataFile` evidence remain where their formats require them.

| Existing boundary | Required version behavior |
| --- | --- |
| `scribe/ingress.rs:221`, admission/preprocess and duplicate mirror | Carry immutable table identity/version plus one shared descriptor. Partition and stamp field IDs from it. Mirror hits retain acceptance version/time, not a copied recipe. |
| WAL SLICE and COMMIT, `scribe/wal.rs:775` | Encode an eight-byte version and bind terminal commit evidence to the same version. Validate all slices/commit for that attempt consistently. Remove recipe length/JSON and redundant definition fingerprint fields after proving their replacement; retain ordinal/count, CRC and payload/WAL digest checks. Update codec discriminator and reject unsupported/malformed formats explicitly. |
| `SealKey`, `scribe/seal_key.rs:105` | Full key includes table version and complete physical partition. Preserve full-key capture from closed finding 5; never collapse to partition/generation. |
| `ScribeAssemblyKey`, `scribe/assembly.rs:240` | Use version for definition compatibility instead of schema/layout fingerprint + revision + recipe digest. Preserve tenant/table/partition and actual merge-compatibility checks. |
| Staged record/context, `scribe/hot_stage.rs:312` | Serialize version and retained-reference dependency, not layout JSON; restore through the existing resolver. Contradictory record/key/version fails closed. |
| `vala.scribe_batch_commits`, existing SQL query owner | Add version/table-identity binding through an additive migration. Keep original ingestion instant, logical digest/count and canonical WAL coordinates. Extend projections, mirror, conflict/resolve/replay reads together. Fence remains acceptance authority after WAL retirement. |
| Promotion record, `scribe/promotion.rs:210` | Add version; retain exact writer-derived `DataFile`, complete typed partition tuple/spec, sort-order declaration, checksums and tenant/footer evidence. The version does not replace native physical descriptors. |
| Forge promotion/compaction | Resolve writer layout from retained version through existing catalog/writer owners. Append original files to the current publication head with truthful native descriptors. Compaction uses its selected output definition/version; it does not relabel old input files. Preserve closed findings 3/4. |
| Oracle cuts/pins | Carry version with the existing exact metadata/snapshot/hot cut. Resolve each source by its accepted fields/spec/version while current query projection uses stable IDs. Preserve visibility revision, live-tail bounds and snapshot/file leases; version is not a snapshot selector. |
| Describe/register and SDK writer | Return/retain/send one version with the matching definition and immutable table identity. No separate fingerprint-as-version contract. |

Design the additive fence migration and catalog reference relation with existing tenant isolation, composite identity constraints and catalog capabilities. Do not populate unknown historical versions from current Blooms. This in-baseline change has no authorized release migration/backfill/reset: if existing persisted legacy evidence cannot be mapped unambiguously, stop for the spec's compatibility decision rather than silently inventing a version or maintaining two permanent authority models. Document the chosen transition before changing format readers.

#### Cold retries and provisional WAL attempts

A cold retry may have written a complete provisional SLICE/COMMIT attempt under a different current definition before fence conflict reveals the original acceptance. There is no abort marker. Reuse the fence's immutable canonical WAL coordinates to select the accepted attempt. Preserve each attempt's internal CRC, digest, ordinal/count and terminal consistency; move cross-attempt duplicate resolution to that authority boundary rather than rejecting different provisional geometry in `ReplayAccumulator` first.

Only the canonical attempt for an already accepted identity may restore rows. A complete noncanonical provisional attempt for that spent ID owns no accepted rows. Contradictions at canonical coordinates still fail closed. Unavailable fence authority never permits speculative suppression or promotion of the provisional attempt. Preserve recovery of genuinely unfenced complete commits using their recorded version and native definition, including interruption after WAL sync but before fencing. Root their definition until recovery settles them.

Do not add an abort record, truncate WAL to conceal the provisional attempt, create a second acceptance ledger or add a replay service. Preserve exact row order/source schema/payload checks, ingestion time and origin. An original retry succeeds after restart, publication and WAL retirement without rewriting accepted rows.

#### Checks retained, work removed and throughput accounting

At `scribe/execution_lanes.rs:547`, each decoded batch currently checks fields, conforms, fingerprints source schema, validates dynamic values and stamps managed/field-ID metadata. Plans are already cached at `:755`. At `catalog/bifrost_catalog.rs:1365`, recipe creation is already once per committed-revision cache fill. Do not claim all conformance planning or recipe serialization happens on every request.

Keep structural/type/nullability checks, duplicate/reserved-name refusal, Variant/value validation, authorization, native/canonical trust boundaries, managed stamping and required supported casts/name ordering. A client-supplied version does not establish that its bytes conform. Key the existing conformance cache by the immutable version/descriptor and actual source shape; valid noncanonical input still needs its actual normalization. An exact conforming shape may take the existing validated fast path. Remove source-schema fingerprint computation/comparison **as redundant definition identity**, not source-shape or payload conflict proof that the accepted-batch protocol still needs.

Delete `AcceptedRecipe`, canonical layout JSON transport/decode/digest, recipe bytes in WAL/staged records, the two-field `LayoutRevision` as Bifrost authority, and redundant composite schema/layout fingerprint comparisons in seal/assembly/replay. Reuse the existing layout wire type for declarations/descriptions; do not delete public authoring validation. Keep payload digests and required native schema/spec/order fields. Replace the recipe portions of `86cb0d3b8`; do not blanket-revert closed fixes or `3ab558f12` formatting. Do not merge `wip/f1-phase-b-binding`; port only its required behaviors through the version model.

Current per-SLICE identity includes 8 revision bytes + 4 recipe-length bytes + `N` recipe bytes + 32 source-schema-fingerprint bytes (`scribe/wal.rs:775`, test encoder capacity accounting at `:868`). Replacing those fields with one 8-byte version removes **`N + 36` bytes per slice**, provided the source fingerprint is removed from this encoding after preserving logical conflict proof. The unchanged native partition/spec and payload fields are excluded from this comparison. Report actual new COMMIT/manifest/promotion overhead separately; no version is added per row. Do not claim an exact total byte saving until encoded fixtures establish it.

One descriptor lookup replaces separate registration/current-revision acquisition and repeated layout identity work. Remove replay JSON parsing/canonicalization/hashing and assembly's repeated layout traversal (`scribe/assembly.rs:267`). Share cached descriptors by version; cache fill and historical recovery may perform existing catalog/object IO, but successful warm ingress adds no PostgreSQL query or remote metadata fetch. Fold needed retained-reference fields into existing acquisitions/conflict reads, not extra normal-path round trips. Keep bounded cache/source ownership and no new global lock. These structural savings motivate the change; they are not measured throughput proof.

**Write boundary.** Existing catalog/dependency publication owners and immutable pin, layout/descriptor resolution, public contracts/shared transport and three SDK projections, additive SQL fence/reference persistence, Scribe admission/preprocess/mirror/WAL/replay/seal/assembly/stage/promotion and direct consumers, Oracle pins and Forge cleanup/resolution. TASK-005 still owns public PATCH orchestration and committed-edit refresh; update its contract/handoff rather than duplicating it here.

**Proof.** Carry R1 finding 1's proof in full, replacing saved recipes with retained versions:

- Accept rows under an old version; drop a partition dimension so grouping changes; resend original bytes/ID. Original acknowledgment/time/origin/version must survive warm mirror, cold miss, retained-root restart, publication and WAL retirement. Include a provisional differing-geometry WAL attempt and a subsequent restart. Changed payload/schema/order/count still refuses without accepted-row residue.
- Exercise stable-ID rename and nullable addition causing current-contract preparation failure, plus a stale-version admission failure. Existing accepted IDs resolve to their original definition; unknown IDs preserve validation refusal. Exercise native and canonical paths under real authorization.
- Stage nonempty Blooms, then clear, replace, reorder or rename a Bloomed field. Each effective definition change creates a new version; old accepted output retains its original policy, new batches use the new version, restart/publish returns exact rows/origin. Retained definitions remain resolvable after native metadata-log rollover and Forge expiry/orphan passes; release/prune only after all roots and retry retention permit it.
- Add focused version/reference/fence/WAL/stage/promotion round trips; reject contradictory version/reference/schema/partition evidence. Include unfenced complete WAL recovery, internally malformed provisional attempts, canonical-coordinate contradictions and unavailable fence authority.
- Add catalog publication seam proof: competing Bloom-only edits from one expected version yield exactly one winner; a losing CAS publishes no retained reference; data-only head movement never loses an edit; uncertain COMMIT resolves without inventing a second version. Creation/re-registration and failed/no-op edits preserve version semantics. Coordinate with TASK-005's real PATCH/multi-instance journeys instead of claiming a helper-only edit proves that API.
- Real SDK/server journeys prove describe/register binding, Rust/Python/TypeScript writers sending the retained version, stale-new-write refusal and accepted-retry behavior. Built-in canonical projection and native dynamic writes share the same Scribe authority. Tenant table-name collisions/recreation do not redirect an old writer.
- Trace/test successful fresh admission to establish no added SQL acquisition/statement/transaction from versioning. Use existing diagnostics, not new production instrumentation. Run matched throughput comparison only when separately authorized as described below.

### 2. Batch fence statements inside the existing tenant transaction

**Throughput correction supporting finding 1; no new acceptance authority.** `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:406` sets `MAX_GROUP_ITEMS = 64`. `pop_group` (`:596`) drains a bounded fair group and stops before a repeated `(tenant, table, batch_id)` identity. `process_group` (`:3072`) writes/syncs/fences before active insertion. `append_and_sync_batch_commits` (`:3583`) appends terminal records, calls one `sync_group` for touched segments (`:3630`), then groups fences by tenant (`:3637`).

`commit_batch_control_fences` (`shards.rs:3757`) opens one tenant transaction, awaits `record()` once per batch (`:3769`), then commits once (`:3776`). `crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:280` implements each call as `INSERT ... ON CONFLICT DO NOTHING RETURNING`, followed on conflict by `load` (`:318`, SELECT at `:375`). A group therefore amortizes fsync and transaction commit but still pays up to one INSERT round trip per batch per tenant, plus conflict reads. Earlier line hints such as `process_group:1381` are not current HEAD locations; use these verified owners.

**Selected correction.** Extend the existing SQL query owner with one multi-row insert per tenant per drained group, using parameterized `VALUES` or `INSERT ... SELECT FROM unnest(...)`. Include every version/fence fact in aligned rows. Use `ON CONFLICT DO NOTHING RETURNING` with enough identity to distinguish tables as well as batch IDs. Classify non-returned keys through **one batched SELECT** in that same transaction. All-fresh groups need no conflict SELECT. Do not correlate `RETURNING` by row order: PostgreSQL does not promise input order, and a batch UUID can be used on different tables.

Preserve these semantics:

- Match every offered row to its full durable key and classify it in original group order: exact canonical attempt is `Committed`; equal accepted logical content from a different attempt is `AlreadyCommitted`; contradiction is an error. Include retained version and original acceptance facts in duplicate results. Key existence alone is never success.
- Preserve `pop_group`'s duplicate-identity guard and fairness. Do not deduplicate a contradictory request away to make multi-row insertion work.
- Keep every fresh insertion rollbackable until **all** tenant-group conflicts validate. A contradiction rolls back fresh rows from that tenant group; earlier committed tenant groups retain their existing behavior. Retained-version exceptional comparison follows section 1 outside the transaction, then resumes only after equality is proved. Do not partially commit the fresh subset first.
- Preserve one existing transaction/COMMIT per tenant per group, existing tenant iteration/order, WAL-before-fence-before-insert/ACK ordering, spent-ID suppression and request cancellation ownership. The optimization changes statement count, not transaction count or group size.
- Preserve the ambiguous-COMMIT loop at `shards.rs:3781`: use a fresh tenant transaction and a **batched `resolve`** read for all offered keys. An exact/logical match receives its existing classification; any absent result retries the identical fence set; unavailable or contradictory authority poisons the shared governor and retains `GroupWalState`. Never acknowledge ambiguous acceptance or discard its admitted resources.
- Use existing `TenantConn`, transaction-local RLS and tenant-isolation checks. Caller owns commit/rollback; query helpers do not open nested transactions or widen to a bare pool. Keep the migration's forced RLS policies (`20260910000021_scribe_batch_commits.sql:22`). Run `check:tenant-isolation`; do not add manual tenant predicates on `TenantConn` paths or weaken the boundary checker.

**Write boundary/proof.** Existing `vala-sql` fence types/query owner, Scribe tenant-group fence loop/reconciliation and focused tests. Add a `vala-sql` Postgres test with fresh, exact duplicate, logically equal/noncanonical and contradictory rows in one offered set; contradiction must roll back the fresh subset. Prove results are correctly mapped despite returned-row ordering and same UUID on different tables, plus tenant refusal. Add a shard test showing N fresh batches in one tenant group execute one insert statement, and mixed tenants execute one per tenant with unchanged transaction count; the conflict case adds one batched read. Exercise ambiguous commit with all-present, absent, contradictory and unavailable outcomes and preserve poisoning/rollback behavior. Use existing seams/test observation; no production counter or new test runtime.

**Migration comment disposition.** `crates/vala/vala-sql/migrations/20260910000021_scribe_batch_commits.sql:1` incorrectly says the fence commits together with its canonical audit event. `queries/scribe_batch_commits.rs:3` and `architecture/wyrd-security-posture.md:744` correctly describe lineage only: no permission decision and no audit. `crates/wyrd/wyrd-sql/AGENTS.md:54` forbids editing applied, checksum-verified migrations. Leave that historical migration unchanged; record this discrepancy for the SQL documentation owner, and describe lineage correctly in new migration/query documentation. Do not introduce an audit write to make the old comment true.

Statement reduction is source-backed; throughput impact remains unmeasured until the matched benchmark below. Keep existing retry/transaction behavior even if a simpler multi-row sketch would be faster.

### 3. Preserve closed R1 findings 2–6 and committed evidence

These are implemented/committed on this branch, not new work to re-specify. Git ancestry and implementation subjects were checked at HEAD; the following RED/GREEN results are carried owner evidence and were not rerun for R2. Preserve these fixes while replacing recipe identity; a regression reopens the affected finding.

| Finding | Closed implementation and carried evidence |
| --- | --- |
| FIND-TASK-004-6 — rustdoc | `085c0bade` documents the validated changed items. Closure is useful intent/error/real-panic documentation, not blanket exemptions. New version/fence changes must meet the same rustdoc rules. |
| FIND-TASK-004-2 — native request-wide fanout | `1fad7df00`: `seal_key.rs::PartitionPlan::union_into` and `preprocess.rs::NativeSliceProducer.partitions` enforce the complete request union. RED: `ingest_bounds.rs:411`, disjoint 17+16 stream accepted with union disabled. GREEN: 1/1 `ingest_bounds::composite_partition_fanout_is_atomic`. Preserve union 32 success/33 refusal, repeated-tuple counting and refused-ID reuse. |
| FIND-TASK-004-4 — promotion tuple authority | `ab8befbc2`: compare complete durable SQL partition tuple against promotion evidence on worker revalidation. Diagnosis found a planning-time liveness gap; do not move this refusal back into planning or silently drop contradictory demand. GREEN: 1/1 corruption selector covering 11 original mutations + 4 tuple mutations; intact composite input promotes. |
| FIND-TASK-004-5 — full seal-key capture | `9ac8a3110`: source capture retains full seal key. GREEN: 36/36 `scribe::memtable::tests::` / `scribe::hot_source::tests::`, including `revisions_sharing_an_ordinal_are_captured_by_full_key`. Version conversion must retain this full-key distinction. |
| FIND-TASK-004-3 — compaction core spec grouping | Native fork commit `a561b990b5013563072368eb2fcfea99263f6dfd`, branch `wyrd-production`, separates partition-scoped groups by retained spec plus typed tuple. RED at fork `core/src/file_selection/strategy.rs:3491`; GREEN 154/154 library tests and clean clippy. Wyrd pin `63b8ea5f1`; `scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth` was RED on old pin and GREEN 1/1 in 15.3s. Preserve table-scoped delete behavior and native execution ownership. The fork line is historical fork evidence, not a Wyrd HEAD source location. |

`3ab558f12` is formatting only. `86cb0d3b8` is not finding-1 closure under this task: its accepted-recipe representation is superseded and must be replaced, while preserving required behavior and the unrelated commits above.

### 4. Supply the required physical and query evidence

**FIND-TASK-004-7 remains open; scenario 5, AC-008/016/018.** Carry R1's physical/query proof obligation unchanged in substance. Existing evidence does not establish the complete policy matrix, compaction Bloom application, representative file/group/page/byte geometry, actual pruning and planning overhead together. The six-row writer test at `scribe/parquet_writer.rs:2082` covers configured Blooms with unfiltered reads/direct probes; it is not a policy-equivalent Oracle query measurement. Current mixed-spec journeys already exercise predicates, so do not repeat R1's blanket statement that every mixed fixture has empty filters. No speedup or numerical SLO is alleged or required.

**Selected correction.** Extend existing writer, mixed-source and Forge fixtures with a bounded deterministic mixed-Service/environment dataset producing multiple ordinary groups/pages under unchanged writer constants. For identical rows/sort, exercise complete, partial, empty and reordered Bloom lists and historical files missing newly selected filters. Each effective Bloom-list change now commits a new table version, including a reorder when the declared definition changes; membership/query results remain equivalent. Old files keep their original version and metadata.

Run same-Service selective and cross-Service/environment equality, finite-IN and null predicates through Oracle. Compare exact authorized row identities across policies and before/after compaction; inspect complete/empty output Bloom metadata after compaction. Retain local/hot/published/peer and missing-historical-filter exact fallback. Do not treat a row-count match as proof of row identity or use direct Bloom probes as a substitute for query execution.

**Existing fixture/telemetry owners.** Start from `iceberg_filtering_mechanisms_cover_all_table_kinds` in `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:2526`. Public PATCH is not implemented at this HEAD; test Bloom changes currently use `WyrdTestServer::commit_table_revision` in `crates/wyrd/wyrd-testing/src/bifrost/revisions.rs:165`, committing at `:195` and evicting the local revision cache at `:200`. Update that test seam to the real versioned catalog commit; do not add direct test-only property writes that bypass atomic publication. TASK-005 later proves the public mutation path.

Reuse telemetry families in `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:341`, `parquet_pruning` in `oracle/exec.rs:107`, optional physical-byte and file/partition counters in `oracle/exec.rs:272`, distributed scan metrics, and `oracle/profile.rs:42` steps for cut read/materialization, Scribe listing, provider setup, physical planning, scan sizing and admission. Use Parquet footer/offset metadata for encoded geometry. Do not create a second measurement framework.

Record fixture distribution, file count/bytes, group/page counts, candidate/scanned/pruned units, physical scanned bytes and observed planning/discovery/admission time in the original task evidence. State unavailable counters explicitly; `physical_bytes` is optional, not a fabricated zero. Do not substitute transport bytes. Observations and exact-result equivalence close the promised proof even when Blooms do not help a predicate. No production tuning, cache, threshold or synthetic host load is authorized.

## Required authority/spec revisions and owner decisions

Revision 13 is approved and its spec, supplement and `architecture/bifrost-design.md` edits are made. Keep these owners consistent and complete remaining task alignment before execution:

| Authority/artifact | Required revision |
| --- | --- |
| `architecture/bifrost-design.md:102` revision/identity section and affected Scribe/Oracle/Forge durability/retention prose | Replace `LayoutRevision { schema_id, sort_order_id }` + separate spec as Bifrost definition identity with one table version. Describe native retained metadata reference, current-version admission/accepted retries and atomic publication. Keep SQL pre-ACK fence and native physical fields. |
| `changes/active/service-identity-and-telemetry/spec.md:312` | Revise REQ-035 resolved describe/version, REQ-037 cross-tier version propagation and REQ-038 compatible evolution/retained definition semantics. Update affected acceptance mapping without weakening exact-result, origin, tenant or performance constraints. |
| `supplements/table-storage-and-correlation.md:315` revision/PATCH section and lifecycle/retention obligations | Make public configuration revision the same table version; canonical field spelling, creation 1, one effective-edit increment, atomic expected-version conflict, retry exception and no data-commit bump. Keep immutable time axis and safe-edit matrix. |
| `tasks/TASK-004-shared-table-storage.md` | Replace composite/recipe carrier design with this shared substrate, codecs/retention/reference publication and proof. Preserve findings already closed. |
| `tasks/TASK-005-tenant-table-configuration.md` | Expose the same expected-version mutation; no second allocator or properties-only CAS. Refresh one coherent descriptor across replicas after proven commit; preserve old accepted versions. Retain TASK-004 dependency and public API ownership. |
| This task and later implementation evidence | Update `spec_revision` to the approved revision before ready/execution; record immutable dependency fix, exact selectors/results and remaining measured limits. R1 remains historical/superseded. |

Decisions approved in revision 13:

| Owner decision | Approved default and consequence |
| --- | --- |
| Version coverage | Every effective complete-definition/configuration edit affecting schema/layout/writer output increments once, including Bloom-only changes; no-op refuses and snapshot/housekeeping commits preserve the version. Reuse one public table-version integer; no parallel config counter. |
| Stale writers | Reject stale versions for new IDs; accepted IDs retain their original version and strict equality checks. Do not auto-refresh/repartition an already queued batch. |
| Retention | Always retain the current version; keep older definition roots until all durable/active references and accepted-retry retention expire. Without an approved finite horizon, retain; do not invent a TTL. Keep data snapshot retention separately owned. |

Revision 13 fixes integer/wire representation above and specifies an in-baseline transition with no release migration/backfill/alias: legacy evidence without an unambiguous exact retained-version mapping is refused, never guessed. These decisions are approved and need no further implementation interpretation. If the dependency cannot atomically publish the reference at the existing catalog boundary, stop for the boundary decision; do not emulate atomicity with a second database write.

## Preserved behavior and excluded work

Retain authenticated tenant/table scope, current permissions, original origin/ingestion instant, exact payload conflicts, complete typed null/spec identity, stable field IDs, SQL/RLS capabilities, database-anchored clocks, checksums, query/source pins, publication/lease fences, uncertain-result reconciliation and bounded ownership. Recovery failures stay scoped to the affected role/request; no shared-server crash policy is introduced. Native compaction remains grouping/execution owner; Rust remains durable-behavior owner.

Do not replace the existing pre-ACK SQL fence, add a second acceptance ledger, add a history service, implement TASK-005's PATCH/refresh workflow twice, change TASK-003 inventory/schema ownership, perform release backfill/reset, automatically rewrite historical data, tune writer constants or invent a performance SLO. CONC-03 remains rejected as a finding on the original immutable range; committed-edit refresh is still TASK-005's obligation. R1's claim that no spec amendment is needed does not apply to R2's public/persisted version decision.

## Throughput constraint and benchmark gate

Successful fresh ingest must add no PostgreSQL call, preliminary accepted-ID read, per-batch transaction, extra full-payload copy/hash pass, per-row metadata or global lock. Preserve grouped fence scheduling, resource accounting and concurrency. Version/reference persistence is cold catalog-publication work; historical reads use existing acquisition/conflict/recovery boundaries. The existing fence still commits before ACK.

The multi-row change reduces a tenant group's INSERTs from N to 1, with at most one batched conflict SELECT and batched uncertain-result resolution when needed. Transaction count stays unchanged. Preserve the existing bounded mirror, not a second cache/ledger. Additional promotion tuple fields remain in the existing query, and full-key capture/native grouping retain their current synchronization owners.

Performance is not established by these structural constraints. Before declaring throughput preservation, compare the reviewed candidate and remediated candidate using the existing benchmark tasks on matching host/configuration, batch sizes, storage and concurrency. Preserve current benchmark gates; passing an existing minimum is insufficient if the candidate shows a reproducible regression against the matched baseline. Record exact baseline/candidate commits and dependency pins, observed run variation, and repeat only to resolve noise. **Tolerance: 5%.** Any throughput metric (rows/s, MB/s, MB/s per core) more than 5% below the baseline, or any query latency more than 5% above it, is a regression and fails the task. Do not change defaults or compensate with stronger hardware.

Ingest comparisons cover native, telemetry and logs; affected query and Forge paths use existing capacity tasks where supported. Compare rows/s, bytes/s, stage times, CPU and fsync behavior using existing reports/profiles. Existing `scribe/profile.rs` group timings are charged to each request, so summing them is not physical group elapsed time; report them consistently without multiplying a group operation into a false saving. No new benchmark framework or production metrics are required.

**When to run.** Run these only when no other worktree is running a build, test, server or benchmark (check processes for other `wyrd-worktrees/*` or main-checkout paths first); otherwise skip and record the limit. No benchmark was run for this recommendation; throughput stays unmeasured until a run completes.

```bash
mise run bench:bifrost:write-capacity
mise run bench:bifrost:write-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity -- --fixture logs
mise run bench:bifrost:query-capacity
```

The capacity benchmarks are the native write and query sweeps; the tracing benchmark is the telemetry fixture. For Forge, use `bench:bifrost:forge-capacity` only when separately asked, on its supported benchmark environment after confirming the owning task/prerequisites. If unavailable, record the limit; do not improvise host controls or claim Forge throughput is verified.

## Acceptance criteria and focused execution

| Finding/obligation | Required closure |
| --- | --- |
| FIND-TASK-004-1 | One version identifies the complete accepted definition through every carrier. Atomic publication/root retention survives concurrent Bloom edits, restart and cleanup. Original retries succeed across grouping/schema changes with warm/cold paths, provisional WAL attempts and WAL retirement; changed requests refuse, origin/time persist and staged old-Bloom output publishes honestly. No normal-ingest SQL is added. |
| Fence optimization | One multi-row INSERT per tenant/group, one batched conflict read when needed, batched ambiguous-COMMIT resolution, unchanged transactions/order/RLS/poisoning and rollback of fresh rows on contradiction. |
| FIND-TASK-004-2/3/4/5/6 | Preserve the closed commits/evidence above; version conversion does not regress request-union fanout, native spec grouping, worker tuple refusal, full-key capture or useful rustdoc. |
| FIND-TASK-004-7 | Real policy-equivalent Oracle queries and post-compaction Bloom metadata, exact authorized rows and observed geometry/pruning/planning evidence recorded without tuning or invented thresholds. |
| Spec/throughput gate | Revised public/persisted meaning has human-approved spec authority before implementation; benchmarks, when run under the no-other-worktree rule, show no regression beyond 5%; otherwise throughput is recorded as unmeasured. |

Apply Red–Green–Refactor one behavioral scenario at a time: first expose missing version binding/publication/retention, then retry/replay cases, then grouped SQL semantics, then missing physical/query proof. Confirm the RED asserts the promised failure before fixing the shared owner; GREEN reruns preceding affected cases; refactor only while preserving them. Documentation preparation itself needs static verification, not a manufactured failing test. Use tracing to diagnose failures and follow repository independent-diagnosis rules before modifying proof to clear a failure.

Extend the existing focused selectors below. These are implementation instructions, not verification already performed for R2. Record selected counts and exact RED cause/GREEN result. New version codec/catalog CAS/retention/fence batch tests need actual nonempty selectors recorded after adding them; derive setup/package/target from manifests and `mise tasks ls --hidden`, not invented test names. Postgres cases live in the owning `pg_tests` modules and use repository-managed Postgres. Native dependency corrections use that fork's repository instructions and immutable pin evidence.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::accepted_batch_revision_survives_alteration_and_restart) | test(=ingest_bounds::composite_partition_fanout_is_atomic) | test(=write_read::mixed_runtime_specifications_preserve_rows_and_pins)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::promotion::scribe_promotion_integration_rejects_physical_datafile_mismatch_before_catalog_io)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::iceberg_filtering_mechanisms_cover_all_table_kinds)"'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(scribe::memtable::tests::) | test(scribe::hot_source::tests::) | test(=scribe::parquet_writer::tests::resolved_sort_and_blooms_preserve_exact_reads) | test(=scribe::seal_key::tests::composite_partition_identity_and_wal)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::revisions_sharing_an_ordinal_are_captured_by_full_key)'

mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
```

Add exact per-language version-handshake journey commands, SQL `pg_tests` selectors, shard batching/reconciliation tests and catalog-fork selectors to implementation evidence after confirming their current owning runners. Regenerate changed contracts from their source and run `mise run codegen:check` plus narrow affected Rust/Python/TypeScript/MCP/HTTP binding checks; public version transport is not covered by private Rust tests alone. Do not hand-edit generated declarations; Python stubs remain authored.

Do not require `gate`, broad redux aggregates or full language/journey suites solely for this task. Full integrated suites run once at change review. Record exact commands/results, closed-finding regression checks, immutable dependency commits, physical/query measurements and remaining limits. Later `$wyrd-task-review` audits the original base-to-remediated-candidate range, original verdict, this replacement task and all seven finding dispositions. No code, migration, commit or benchmark is changed by writing this task.
