---
id: TASK-004-R1
kind: remediation
status: superseded
superseded_by: TASK-004-R2
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7]
requirements: [REQ-035, REQ-036, REQ-037, REQ-038, REQ-042, REQ-044, AC-008, AC-012, AC-013, AC-016, AC-018, AC-021]
depends_on: []
---

# Preserve accepted table authority through retries, recovery and maintenance

Implement this task through `$wyrd-implement`. Restore the approved physical-storage behavior at the existing owners, and supply the missing measured proof. No specification revision is required. This task selects the correction boundaries; private function structure and fixture mechanics remain implementation-owned.

The caller requires preserving current throughput and forbids an added PostgreSQL call on the normal ingest path. The earlier recommendation to look up every incoming batch before preparation is superseded below. Keep corrections limited to demonstrated reachable failures and existing owners; do not add speculative guards or another retry subsystem. No benchmarks are authorized to run during this packet revision; performance verification below is deferred work.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`.
- Original reviewed base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`.
- Reviewed candidate: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- Approved spec: [revision 12](../../spec.md), with [table-storage supplement](../../supplements/table-storage-and-correlation.md).
- Original task: [TASK-004-shared-table-storage.md](../../tasks/TASK-004-shared-table-storage.md).
- Review authority/evidence: [verdict.md](verdict.md) and [findings-validation.md](findings-validation.md). Their immutable range is the starting subject, not permission to discard implementation or prior verification.
- Stronger authority: root and applicable nested `AGENTS.md`; `agent_docs/{rust,errors,maintainer-style}.md`; Bifrost table identity, Scribe, Oracle, Forge and resource/failure sections; security tenant/Postgres/peer rules; operations recovery contract. All commands run through mise; Cargo-backed commands run sequentially.

## Diagnosis and selected corrections

### 1. Retain the complete accepted recipe at the batch authority

**FIND-TASK-004-1; REQ-037/038/042, AC-013/016.** In `scribe/ingress.rs:219–235,547–599`, ingress selects current metadata and conforms/partitions data before checking an accepted ID. `preprocess.rs::LogicalBatchDigest` binds the resulting slice count/order/schema/data. The existing SQL fence in `vala-sql/src/queries/scribe_batch_commits.rs` has no ingress-resolvable original recipe. Accept two same-hour rows distinguished by a shard dimension, remove that dimension, and resend identical bytes/ID: two original slices become one and the fence refuses the honest retry. Rename/addition can fail even earlier in current-schema conformance. V6 only changes sort, leaving this case unproved.

The same missing immutable recipe causes staged recovery failure. `catalog/layout.rs:935–1025` rebuilds retained layouts from current Bloom properties; `scribe/assembly.rs:244–282` hashes the original complete Bloom list; `staging_runtime.rs:788–855` reconstructs and compares keys. Clearing/replacing/reordering Blooms or renaming a Bloomed field can strand valid staged authority. Schema/spec/sort IDs alone cannot identify a Bloom-only change. Deleting Bloom fingerprint checks would conceal this required accepted-layout distinction.

**Selected outcome and recommendation.** Extend the existing tenant/table/batch acceptance authority with one immutable binding. Reuse canonical `PhysicalLayoutWire` for the bounded accepted recipe, alongside the retained schema/spec/sort IDs, accepted source-schema fingerprint and original ingestion instant. Persist it atomically on the existing `scribe_batch_commits` fence through an additive migration; that row remains authority after WAL retirement. Extend the existing bounded committed index to retain this binding and use a hit before preparation. A cache miss follows current preparation, WAL sync and grouped fence execution, with no preliminary SQL lookup. Current preparation on a miss is provisional; it never authorizes replacing an already accepted binding.

The existing fence INSERT already reads the accepted row on an ID conflict. Return its original binding, logical digest/count and acceptance time from that existing conflict read. Never insert the provisional attempt's rows when the fence reports an existing identity. Revalidate the original request under the returned retained binding using the existing bounded CPU owner, outside the SQL transaction; acknowledge the original acceptance only when its original digest/count match. Otherwise return the existing identity conflict. Retain source ownership under existing admission accounting for this exceptional comparison; do not copy or hash the entire payload an additional time on every fresh write. Keep the existing grouped transaction, concurrency, WAL sync and acknowledgement ordering.

Preserve tenant-group rollback behavior: an offered digest/count mismatch ends and rolls back the still-uncommitted group before retained-binding comparison. Only a proven identical retry permits resuming the same grouped fence work; a genuine conflict still commits none of that group's fresh fences. An already exact logical match can return binding/time directly. Keep cancellation and durable completion owned by the shard rather than the caller's response waiter. These extra steps apply only to the exceptional mismatch path.

If current-contract preparation fails before reaching the fence, a tenant-scoped accepted-ID lookup is permitted only on that exceptional failure path. A matching accepted row supplies its original binding for the same strict comparison; an absent row preserves the original validation refusal. Keep current authentication, authorization, request limits and bounded ownership. This does not add SQL to successful fresh requests, and a cache miss alone never triggers a read. Native and canonical paths must reach the same accepted-identity behavior.

**Necessary replay handling.** A cold retry can have a complete provisional SLICE/COMMIT attempt in WAL before the existing fence conflict returns the accepted binding. There is no existing abort marker to rely on. Reuse the SQL fence's existing immutable canonical WAL coordinates: retain existing per-attempt CRC, WAL digest, ordinal/count and terminal checks, then restore only the exact canonical attempt for an already accepted identity. This adds no new payload-hashing protocol. Other complete noncanonical attempts for that spent ID never owned accepted rows and must not become replay sources. Move cross-attempt identity resolution to that existing canonical-location boundary instead of rejecting differing provisional geometry in `ReplayAccumulator` before SQL authority is consulted. Contradictions at the canonical coordinates still fail closed; no available fence authority means no speculative suppression. Keep existing recovery for genuinely unfenced complete commits. Do not introduce an abort record, WAL truncation rule, second ledger or new replay service. This handles the demonstrated cache-cold retry, not a hypothetical failure mode.

Carry that same binding through existing WAL/replay, seal/generation and staged-record/context owners, including an unfenced complete WAL commit recovered after interruption. Keep Bloom-only recipes distinct at existing generation/assembly compatibility boundaries even when schema/spec/order IDs match. Rebuild saved layout against its retained schema, preserving historical names/stable field IDs. Member staging, merge and publication consume this context, not mutable current Blooms. Preserve complete key/fingerprint equality, checksums and malformed-evidence refusal. No second acceptance ledger, configuration revision allocator, history service or public API is needed.

Keep permission/scope and tenant checks on every attempt; retain existing native/canonical managed-column trust validation and stamping owners. An accepted ID alone never proves payload equality. Preserve original row order/schema/payload conflict checks, original origin/ingestion time, and exactly-once rows. Do not rewrite accepted rows or weaken digest/count checks. This closes both failures at the common producer rather than adding restore exceptions and retry bypasses.

**Write boundary.** Existing SQL batch-commit queries/types and additive migration; catalog retained recipe resolution; ingress/admission/preprocessing and bounded duplicate mirror; WAL/replay/seal/generation/staged/context carriers and their direct consumers. Keep new facts private unless existing public contracts actually require a change.

**Proof.** Extend the existing accepted-revision journey: accept under one recipe, change a spec so partition grouping changes, resend the original payload/ID and require successful original acknowledgement before restart, after retained-root restart, and after publication/WAL retirement. Exercise a mirror hit and a cold mirror miss, including a provisional differing-geometry WAL attempt followed by another restart. Include stable-ID rename and nullable addition that cause current-contract preparation failure; an unknown ID must preserve the original refusal. In each phase modified payload/schema/order still refuses without residue, and canonical accepted evidence remains unchanged. Stage nonempty Bloom recipes, clear/replace/reorder or rename a Bloomed field, restart and publish: old accepted output retains its saved recipe, new batches use the new one, and exact rows/origin agree. Keep contradictory retained metadata/key cases failing. Add focused saved-binding codec/fence/replay round trips, including unfenced complete WAL recovery, internally malformed provisional attempts and unavailable fence authority. Trace or test the existing fresh-insert path to demonstrate unchanged SQL operation/transaction counts without adding production instrumentation.

### 2. Enforce fanout over the whole native request

**FIND-TASK-004-2; REQ-036, AC-012/021.** `scribe/preprocess.rs:278–348,651–715` resets `plan_partitions` for each decoded IPC RecordBatch and collects slices without a request-wide union. One legal stream with 17 distinct tuples followed by 16 different tuples becomes durable, although the same rows in one batch are refused. V4 uses a single RecordBatch.

**Selected correction.** At existing native preparation, bound the union of complete `PhysicalPartition` values across all slices before dispatch. Reuse equality, `MAX_PHYSICAL_PARTITIONS` and the existing invalid-frame error. On the 33rd distinct tuple, drop local preparation before WAL/fence. Keep decoding, memory ownership, slice ordinals and the per-source planner. A slice-count limit is wrong because repeated tuples count once. No transport validator, second parse or durable ledger.

**Write boundary/proof.** Native `preprocess.rs` and the existing fanout journey fixture. One stream with union 32 succeeds; disjoint 17+16 refuses with no WAL/fence/rows; more than 32 slices repeating at most 32 tuples succeeds. Corrected input can reuse the refused ID.

### 3. Keep source spec identity in native compaction groups

**FIND-TASK-004-3; REQ-037/038 and scenario 4.** The pinned compaction core `efb39ce4d03c79c27e005b193bd235c9403af7e3`, `core/src/file_selection/strategy.rs:1176–1203`, keys partition groups by `Struct` alone. Forge's `managed/policy.rs:281–305,419–501` delegates partition-scoped planning there. Equal same-hour null/bucket tuples under bucket(4) and bucket(8) specs become one input group and can satisfy the two-file threshold together. V8 changes tuple arity, avoiding this collision.

**Selected correction.** Extend the existing core `PartitionKey` with each task's retained spec identity plus typed tuple; keep absent spec distinct rather than assuming a default. Fix and test that native owner, then update Wyrd to the immutable corrected fork commit and lockfile. Do not pregroup in Wyrd or add a planner. Preserve explicitly table-scoped copy-on-write/delete grouping, output selection, sorting, target sizes, delete application and fenced commit semantics.

**Write boundary/proof.** External core grouping owner/tests and Wyrd pins/lock plus the existing mixed-spec Forge seam. Equal-valued and null tuples under different specs form separate partition-scoped input groups; normal same-spec files still compact. Assert input group spec membership, not output truth alone. Use the external repository's own instructions/mise task for its focused grouping tests, record its exact immutable commit and command, and then run the focused Wyrd Forge selector below. Prepare the tested correction in an ordinary local fork checkout, not a Cargo cache. Commit or push only when asked. If the corrected immutable commit is not yet available or publication is necessary, prepare the concrete tested correction and identify the remaining Git action; do not substitute a mutable branch/path dependency or leave the correction unspecified.

### 4. Compare promotion evidence against complete durable partition authority

**FIND-TASK-004-4; REQ-037, AC-013.** Scribe writes full SQL partition dimensions and JSON promotion partition atomically, but `vala-sql/src/queries/file_list.rs:37–49,153–199` omits the SQL tuple from promotable rows. `forge/scribe_promotion.rs:335–521` checks identity/checksum/footer and only `.time()` of the JSON partition. Changing JSON customer alpha to valid beta leaves physical metrics intact and reaches append with a false manifest tuple, allowing equality pruning to hide alpha rows. This is the existing contradictory-durable-evidence boundary, not a new public attacker model.

**Selected correction.** Extend `HotFileCatalog::list_promotable` and its row projection with durable granularity/start/dimension bytes. At the existing demand reader, decode with `TimePartition::from_durable_columns` and `PhysicalPartition::from_dimension_bytes`; require full equality with decoded promotion evidence before creating demand/catalog effects. Malformed or contradictory projections fail with the existing invariant error. This guard belongs at the durable-evidence join. Keep retained spec/order, tenant, checksum, footer and time checks; no row scan, current-metadata reinterpretation or new authority.

**Write boundary/proof.** SQL projection/types and direct fixtures/consumers, promotion demand reader and existing corruption integration test. Mutate a valid same-type dimension, null versus literal and time-position/full-tuple metadata only in JSON: refuse before append, preserve snapshot/settlement and queryable hot rows. Intact data under retained specs still promotes.

### 5. Preserve full seal identity in coherent capture

**FIND-TASK-004-5; AC-013 and scenario 3.** `scribe/memtable.rs:1746–1764,1912–1943` and `hot_source.rs:889–975` exchange `(partition,generation)` after discarding revision. A Ready staged old-revision generation (lane 0, ordinal 1) can restore without WAL generation reconstruction. The fresh process counter starts at 1; a new revision's exact-key high-water lookup is empty, so its first frozen memory generation reuses that ordinal. Capture marks the pair served and omits old staged rows from a successful result. See the full controlled recovery schedule in [followup-review.md](followup-review.md).

**Selected correction.** Keep full existing `SealKey` in readable-source identity and served/superseded membership, comparing `(SealKey, GenerationOrdinal)`. Update bucket/frozen constructors and direct capture consumers consistently. Derive time filtering from the retained partition. Preserve registry locking, publication barrier, memtable guards, staged leases and resource ownership. No global allocator, durable ordinal ledger or forced startup publication.

**Write boundary/proof.** `memtable.rs`, `hot_source.rs` and owning inline tests. Restore old R staged ordinal 1; use a fresh registry-attached memtable and freeze same-partition R2 ordinal 1; assert reuse occurred and both exact source row sets are selected once. Publication suppresses only the matching full key. Reuse existing registry/capture APIs; no new runtime/server harness.

### 6. Complete the changed-item rustdoc gate in the same pass

**FIND-TASK-004-6; root AGENTS and Rust guide.** Validated omissions are `seal_key.rs:568,595,619` (composite helpers/test), `physical_partition.rs:357,384,530,574` (fallible codec helper, conversion method, index accessors), `wal.rs:801,818,996` (composite/revision test encoders and metadata decoder), and `layout.rs:2282,2324` (rewritten canonical-layout tests).

Document those existing items in place: workflow intent, actual errors, real index/fixture panic preconditions and durable retry/cancellation semantics where applicable. Include newly changed remediation items as written and consolidate other omissions in this bounded write set. Do not invent a reachable panic for exact-width conversions protected by a matching read; do not add helpers/tests or suppress checks to satisfy prose requirements. Source inspection and format/owning clippy verification close this documentation portion.

### 7. Supply the required physical and query evidence

**FIND-TASK-004-7; scenario 5, AC-008/016/018.** The evidence table records plans/rows only. V9 (`scribe/parquet_writer.rs:2082–2255`) uses six rows, unfiltered reads and direct Bloom probes. V7 and Forge fixtures use empty filters. They do not prove requested complete/partial/empty/reordered-policy query equivalence, compaction Bloom application, representative file/group/page/byte geometry, actual pruning or planning overhead. No speedup or numerical SLO is alleged or required.

**Selected correction.** Extend existing writer, mixed-source and Forge fixtures with a bounded deterministic mixed-Service/environment dataset producing multiple ordinary groups/pages under unchanged writer constants. For identical rows/sort, exercise complete, partial, empty and reordered Bloom lists and historical files missing newly selected filters. Run same-Service selective and cross-Service/environment equality, finite-IN and null predicates through Oracle; compare exact authorized row identities across policies and before/after compaction. Inspect complete/empty output Bloom metadata after compaction.

Use existing Oracle scan telemetry (`parquet_pruning`, `ScanStats`, distributed scan metrics), query timing telemetry, and Parquet footer/offset metadata. Record fixture distribution, file count/bytes, group/page counts, candidate/scanned/pruned units, scanned bytes and observed planning/discovery/admission time in the original task evidence. State unavailable counters explicitly; do not substitute transport bytes or fabricated zeroes. No new benchmark framework, production tuning, cache or threshold. Observations and exact-result equivalence close the promised proof, regardless of whether Bloom pruning helps every predicate.

## Preserved behavior and excluded work

Retain authenticated tenant/table scope, current permission checks, origin, exact payload conflicts, complete typed null/spec identity, stable field IDs, SQL/RLS capabilities, clocks, checksums, reader pins, publication/lease fences, uncertain-result reconciliation and bounded ownership. Recovery failures stay scoped to the affected role/request; no shared-server crash policy is introduced. Native compaction remains grouping/execution owner and Rust remains durable-behavior owner.

Do not implement public PATCH/CAS, committed-edit cache refresh/broadcasting (TASK-005), system inventory/schema changes (TASK-003), final language-surface/docsite closure, automatic history rewrites, release migration/backfill, retention/TTL, tuning or a second engine/ledger. CONC-03 was rejected and is not a remediation requirement. The accepted-layout correction uses already approved behavior; no spec amendment or approval is needed for that recommendation.

## Throughput constraint and deferred verification

Successful fresh ingest must retain the current SQL operation and transaction counts, grouped fence scheduling and concurrency. No preliminary accepted-ID read, new round trip, per-batch transaction, additional full-payload copy/hash pass, per-row layout metadata or new global lock is permitted to implement this correction. Extend the existing bounded mirror and existing conflict read; no second cache or ledger. Promotion's extra tuple fields use its existing query, and capture/grouping corrections retain their existing synchronization and execution owners. Any additional guard must enforce a demonstrated reachable invariant in the diagnoses above.

Performance is not established by these structural constraints alone. Before declaring throughput preservation, compare the reviewed candidate and remediated candidate using the existing benchmark tasks on matching host/configuration, batch sizes, storage and concurrency. Preserve current benchmark gates; passing an existing minimum is insufficient if the candidate shows a reproducible regression against the matched baseline. Report observed run variation and repeat only when needed to resolve noise; do not invent a percentage tolerance, change defaults or compensate with stronger hardware. Ingest comparisons cover native, telemetry and logs; affected query and Forge paths use their existing capacity tasks where supported. Compare rows/s, bytes/s, stage times, CPU and fsync behavior using existing reports/profiles. No new benchmark framework or production metrics are required.

These are deferred verification instructions, not authorization to run benchmarks during this review-packet revision. No benchmark has been run for the revised recommendation. Retain an explicit unmeasured status until that comparison is performed.

```bash
mise run bench:bifrost:write-capacity
mise run bench:bifrost:write-capacity -- --fixture telemetry
mise run bench:bifrost:write-capacity -- --fixture logs
mise run bench:bifrost:query-capacity
```

For Forge, select the existing `bench:bifrost:forge-capacity` task only on its supported benchmark environment after confirming its owning task and prerequisites; do not improvise host controls or generate synthetic host load. If that task is unavailable on the host, record the limit rather than claiming Forge throughput is verified.

## Acceptance criteria and focused execution

| Finding | Required closure |
| --- | --- |
| FIND-TASK-004-1 | Identical original retries succeed under grouping/schema changes through restart/publication with warm/cold mirror paths and safe canonical WAL replay; changed requests still refuse; accepted time/origin/recipe persist; staged Bloom changes recover; fresh SQL operation/transaction counts stay unchanged; throughput verification remains explicitly unmeasured until the deferred matched comparison. |
| FIND-TASK-004-2 | Request-wide union 32 accepted/33 refused atomically; repeated tuples count once; refused ID reusable. |
| FIND-TASK-004-3 | Corrected immutable native core produces spec-separated partition input groups, preserves same-spec compaction and table-scoped delete work; Wyrd uses that pin. |
| FIND-TASK-004-4 | Valid contradictory composite evidence is refused before append without losing hot rows or changing settlement; intact retained-spec promotion succeeds. |
| FIND-TASK-004-5 | Restored/new full keys sharing ordinal select both sources exactly once; suppression affects only the exact full key. |
| FIND-TASK-004-6 | Validated changed items and remediation items satisfy useful rustdoc/error/real-panic contracts without exemptions. |
| FIND-TASK-004-7 | Real query/output policy equivalence plus requested observed geometry/pruning/planning measurements recorded, without tuning or invented thresholds. |

Apply the repository Red–Green–Refactor discipline one behavioral scenario at a time. Extend the existing selectors below; confirm selected counts and record exact RED cause and GREEN result. Newly added codec/fence/capture/native grouping tests must receive exact nonempty selectors and repository-native setup in implementation evidence. Use diagnostics/tracing for failures and obtain the required independent diagnosis before changing proof to clear a failure.

The following commands name existing tests to extend, not completed verification. They are scoped closure checks, not full journey suites. Run sequentially:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::accepted_batch_revision_survives_alteration_and_restart) | test(=ingest_bounds::composite_partition_fanout_is_atomic) | test(=write_read::mixed_runtime_specifications_preserve_rows_and_pins)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test forge -P journey --run-ignored=all -E "test(=scribe_promotion::mixed_specifications_preserve_partition_and_sort_truth)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all -E 'test(=forge::promotion::scribe_promotion_integration_rejects_physical_datafile_mismatch_before_catalog_io)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(scribe::memtable::tests::) | test(scribe::hot_source::tests::) | test(=scribe::parquet_writer::tests::resolved_sort_and_blooms_preserve_exact_reads) | test(=scribe::seal_key::tests::composite_partition_identity_and_wal)'

mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
```

Also run the exact focused new/extended binding codec and SQL-fence tests, with Postgres wrapper where needed, and native fork grouping proof. Select the owning commands from `mise tasks ls --hidden` and manifests before execution; do not invent test/lint lane names. If public/generated declarations actually change, regenerate from their owner and run `mise run codegen:check` plus the narrow affected binding check. Private accepted-binding persistence does not itself require public SDK expansion.

Do not require `gate`, full language journeys, full Scribe/Forge suites or broad redux aggregates solely for this remediation. Full integrated suites run once at change review. Record concrete commands/results, selected counts, dependency commit, physical/query measurements and remaining limits. A later `$wyrd-task-review` audits the entire original base-to-remediated-candidate range, this verdict and all seven finding closures.
