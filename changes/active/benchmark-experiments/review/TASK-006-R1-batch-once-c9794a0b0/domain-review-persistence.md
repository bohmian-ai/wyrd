# Domain review: persistent data / Scribe persistence (Step 4 gathered sort)

- Subject: base `55f87cef1`, candidate `c9794a0b0` (`git diff 55f87cef1...c9794a0b0`), Step 4 commit `f50fdd9fb`.
- Reviewer: fresh `domain-rev` (persistence). Read-only. No cargo/mise run.
- Result: **PASS**

## Reviewed boundary

`crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs`:
`ParquetBatchEncoder::prepare_sorted_candidate` (now lines 400-479), `sort_keys` (replaces
`sort_batch`), and the callers and owners around them: `ParquetBatchEncoder::encode`,
`RollingArtifactWriter::append_charged_batch` / `open_artifact`, `file_candidates`,
`encode_batch` -> `ScribeMemberStager::encode_runs` (`member_stager.rs:181-260`, fsync and
`preflight_run`), `ScribeResources::try_reserve_maintenance` / `scribe_resource_error`
(`resources.rs:1537`, `4336`), `memory::unshared_arrow_bytes`, how the memtable bucket gets its
schema (`memtable.rs:323-338`, `1355`, `1418`), and `ConformPlan` (`execution_lanes.rs:716-800`).
New tests `gathered_sort_matches_concatenated_reference`, `gathered_sort_respects_persistence_admission`,
the rewritten `sort_batch_uses_one_key_for_every_table`, and the extended
`scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path`.

## Authority and source coverage

- AGENTS.md; task section "Approved revision — build batches once" (Step 4, "Persistence sorting"
  fixed decision, P4 block, reuse map); `architecture/bifrost-design.md` `ScribeHotStage` paragraph
  (updated in `cd26d66d3` to say order is computed from sort-key columns and payload gathered once).
- Installed Arrow 60.0.0 source: `arrow-ord/src/sort.rs::lexsort_to_indices` (lines 939-1010) and
  `arrow-select/src/interleave.rs::interleave_record_batch` (917-932), `concat.rs::concat_batches` (613-641).
- `verification.md` (orchestrator rerun): P4 unit selectors 8/8 and both P4 journeys 2/2 pass.

## Obligation checks

| Obligation | Finding |
|---|---|
| Comparator identical | `sort_keys` gives back the same `(index, SortOptions{descending: key.is_descending(), nulls_first: key.nulls_first()})` in declaration order that `sort_batch` built. The missing-column error is kept. |
| Equal-key ordering | Old and new both call `lexsort_to_indices(&columns, None)`. In Arrow 60 that is an *unstable* sort (`sort_unstable_by` / `sort_fixed_column`; with one key it goes to `sort_to_indices`). So neither path was ever stable by source position. The new key columns are `concat` of the same per-batch arrays, in the same order and with the same values, that `concat_batches` produced. The input to the same deterministic function is therefore identical, and the permutation is identical, ties included. Ties keep exactly the old order, which is what the task requires. It is not a "batch then row" stable order, and the old code did not give one either. |
| Position mapping | `starts` holds exclusive prefix sums. `partition_point(start <= row) - 1` is always >= 0 because `starts[0] == 0`, and it skips zero-row source batches correctly. |
| No unsorted payload concat | Only the key columns are concatenated. The payload is gathered once with `interleave_record_batch` over the original source batches. No new dependency. A single source batch still costs one copy, the same as before (`concat` of one array is a slice). |
| Bounds unchanged | `file_candidates` / `FILE_CANDIDATE_TARGET_ROWS`, one artifact sealed per candidate, `target_object_bytes`, row-group/roll logic, and ordinal/object identity (`open_artifact`) are untouched. |
| Working memory charged | One `Persistence` lease covers key bytes, 20 B/row for indices and positions, and an output estimate (`unshared_arrow_bytes(sources)`). It is taken before any key, index or output allocation and held in `_working` until the function returns. `append_charged_batch` then charges the output exactly, as before. Before this change the concatenated batch and the sorted copy were not charged at all, so accounting is strictly tighter. |
| Pressure retryable | `Occupied` maps to `IngestBusy`, the same class `append_charged_batch` already returned on this path. `encode_runs` clears any unrecorded member directory on the next attempt. Determinism: the same frozen batches give the same permutation and the same rolled artifacts. |
| Tenant/table checks | The binding/seal/frozen tenant and table checks still run before any work. |
| Preflight / merge | `encode_runs` fsync + `preflight_run`, the WAL retirement order and claim merge are unchanged. |
| No sort keys / single batch | Empty keys: `lexsort_to_indices` errors the same way the old path did (both map to `Internal`). Single batch is covered by `sort_batch_uses_one_key_for_every_table` and the `3..4` range. |
| `sort_batch` callers | `git grep` at base shows only the encoder and its one test. At candidate there are none. |

## Test assessment

- `gathered_sort_matches_concatenated_reference` gives real proof. The fixture has four batches,
  equal keys repeated within and across batches with distinct Variant/list payloads, nulls in both
  string keys, descending plus nulls-first options, an hour boundary, and five candidate ranges
  that start and end on stored-batch boundaries. The reference copies the old algorithm, so
  whole-`RecordBatch` equality proves the ties match the old path. The explicit expected-key
  vector pins the comparator independently of `sort_keys`.
- `gathered_sort_respects_persistence_admission` leaves exactly the output-estimate bytes free. It
  proves the charge is larger than the output alone, that refusal is `IngestBusy`, that the
  candidate succeeds once pressure is lifted, and that no bytes leak. It cannot tell `let _working`
  apart from an immediately dropped `let _`. The fact that the lease is held for the whole build is
  shown by reading the code, not by the test (see O-2).
- The extended `scribe_persistence_path` test reads the live tail (memtable) only. It is Step 3
  proof and never runs `prepare_sorted_candidate`. Real publication and recovery for Step 4 rest
  on the `cross_shard` and `source_boundary` journeys (2/2 pass in `verification.md`).

## Proposed findings

None material.

## Minor observations (non-blocking)

- **O-1 (output schema source).** `parquet_writer.rs:476`: `interleave_record_batch` takes the
  schema of `sources[0]`, the first batch of the candidate. The old path used `self.frozen.schema`,
  the first batch of the member. `ConformPlan::build` keeps the sender's field Arcs and top-level
  schema metadata (`execution_lanes.rs:757-772`), so a bucket holding batches with different Arrow
  metadata could now produce candidate artifacts with different embedded Arrow metadata within one
  member. Fingerprints are not affected: `from_arrow_schema_exact` hashes name, nullability and type
  only, and those are conformed. The same mix of metadata across artifacts already happens between
  members, so this adds no new failure class, and no consequence was shown. If parity is wanted,
  the fix is a single line: rebuild with
  `RecordBatch::try_new(Arc::clone(&self.frozen.schema), sorted.columns().to_vec())`.
- **O-2 (test strength).** The admission test would also pass if the lease were dropped right after
  it was reserved. This is acceptable because the binding in the code is correct. No change is
  needed unless someone wants the property tested.

## Verification limits

No builds or tests were run. Test pass results come from `verification.md`. Behavior of the Arrow
internals was checked against the vendored 60.0.0 source, not by running it. No allocation or copy
profile was inspected for the Step 4 performance claim (P5 is out of scope for this domain).
