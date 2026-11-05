# Statistics data-correctness review

Result: **PASS**. No material finding established in the assigned boundary.

## Subject and authority

- Wyrd: `d7de12a74c4bdbf93e727cc2383147e34298f689` to `01fd3116d2501d89317073a2abb547818a23b4a8`.
- DataFusion: `19796a13ed46877cf998013886947b28a6f774b3` to `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.
- Both current HEADs independently checked against those candidates.
- Approved specification revision 3 and original TASK-001; root contributor instructions, task-review skill, spec-driven workflow, maintainer guide, agent-doc index, fork AGENTS, and Bifrost's Oracle source-authority section.
- No CodeGraph directory in either repository. No nested AGENTS found under the reviewed Vala, common, or pruning paths.

This is a first review of the committed cumulative ranges. No prior implementation acceptance or other reviewer's conclusions were used.

## Expanded producer-to-consumer map

1. Scribe `batch_statistics` (`scribe/memtable.rs:2414`) records exact primitive min/max, exact null counts, and byte sizes; uncollected string bounds remain absent. `ReadableSource::project` around line 2220 preserves bounds only for unchanged columns, projects their order, and clears bounds for casts or null-filled fields.
2. Wyrd `set_time_bounds` (`oracle/exec.rs:1369`) creates timestamp bounds in the schema unit and timezone, widening microsecond intervals for nanosecond precision. `LiveTableRoutes::selected_by` (`oracle/live.rs:248`) supplies these interval facts directly to the shared dependency adapter.
3. `PrunableStatistics::exact_column_statistics` (`datafusion/common/src/pruning.rs:424`) combines exact min/max and schema-typed unknown slots. `null_counts` and `row_counts` remain independent exact-only producers.
4. `PruningPredicate::prune`, `build_statistics_record_batch`, and `BoolVecBuilder` (`datafusion/pruning/src/pruning_predicate.rs:661,1289,783`) consume those arrays. Missing arrays become typed null arrays; a null predicate result keeps the container. Independent proven-false conjuncts can still exclude it.
5. Dependency `FilePruner::try_new/should_prune` uses the same adapter, keeping files when pruning evaluation fails. Its one-file input remains unchanged; no Wyrd alternate conversion was added.
6. Wyrd `BatchPruning::keep` (`scan_sizing.rs:593`) supplies per-batch arrays and conservatively keeps all on count mismatch or dependency error. Consumers include `ScanSizer::batch_bytes`, Scribe generation projection, `MemtableSource::pruned`, and hot-file sizing through `ScanSizer::keep` (`oracle/exec.rs:1756`).
7. `BatchPruning::fully_matching` (`scan_sizing.rs:541`) prunes the negated predicate and additionally requires exact zero null counts on every referenced column. `MemtableSource::filtered` uses that proof only to skip row filtering on proven batches; unknown/inexact bounds do not create full-match proofs. Its merged aggregate statistics clear schema-mismatched bounds and become inexact after filtering.
8. `LiveScribeExec::resolved` (`oracle/live.rs:438`) selects only the routes dealt to an output partition. `LivePartitionRead::into_stream` around line 838 builds fragments and opens streams only inside its route loop. Empty resolved routes therefore dispatch nothing without a new special case.

## Acceptance matrix

| Obligation | Source evidence | Proof inspected | Result |
|---|---|---|---|
| REQ-001 / AC-001: exact evidence survives absent, inexact, and missing entries with stable order/count | `exact_column_statistics` maps precisely one slot per container; only `Precision::Exact` supplies values | `partial_statistics_preserve_exact_bounds` checks both orders, values, types, validity, and array lengths | PASS |
| REQ-002 / AC-002: zero/all-unavailable/unknown-column input is quiet and unavailable | Missing schema index and the all-unavailable scan return `Ok(None)` before conversion; warning wrapper logs only `Err` | `empty_statistics_do_not_warn`; actual `PruningPredicate` empty/mixed warning capture in `empty_containers_prune_without_warnings`; Wyrd empty-route proof | PASS |
| REQ-003: parameterized representation retained | Unknown scalar derives from the schema type; existing strict `ScalarValue::iter_to_array` timestamp timezone and decimal parameter checks remain intact | Matrix covers ns timestamp with timezone, all three string representations, integer types, Decimal128 precision/scale; mismatch cases test timezone, decimal, string representation | PASS |
| REQ-004: genuine conversion mismatch is unavailable and diagnostic without scalar contents | Conversion failure returns no bound; diagnostic rebuilds reason from bound name, column name and `ScalarValue::data_type`, discarding the value-bearing conversion error | `incompatible_statistics_have_scrubbed_diagnostics` exercises both orders and min/max, asserts exact diagnostic and secret absence | PASS |
| INV-001 / INV-002: unknown evidence is neither SQL-null data nor an exclusion/full-match proof | Unknown slots are null statistics, while actual data-null knowledge comes from separate exact null counts; `BoolVecBuilder` excludes only `Some(false)`; full match requires exact zero null counts | Wyrd partial-statistics proof compares reference rows and kept batches; existing nullable full-match proof remains; typed-null test retains the exact null scalar's type | PASS |
| REQ-005 / AC-003: sizing, batch exclusion, full match and live/file consumers share correction | All callers above reach the common adapter; only production Wyrd behavior change is removal of its now-unnecessary zero-batch guard | Partial-statistics seam asserts three retained batches, `[false,false,true]` full-match flags, kept-byte sum, zero-batch result, and exact reference rows | PASS |
| AC-004 / INV-003: persisted/live rows, nulls, negatives, and tenant boundary remain intact | Journey writes both source tiers, queries time/service/kind/operation predicates, and checks expected rows; production tenant/source paths are unchanged | `observe_predicates_preserve_rows_across_live_and_persisted_sources` source inspected, including neighbour query and disjoint object-read assertion; existing footer-tenant rejection proof preserved | PASS |
| INV-004 / INV-005 and AC-006: preserve ownership, bounded conversion and adjacent consumers | No new IO or statistics collection; exact-only availability check and array conversion are linear; diagnostic adds only a linear error-path scan; adjacent file and batch consumers remain conservative | Bounded memory/time observations in task evidence; direct producer/caller/consumer inspection above | PASS |

## Verification and limits

The review-time `verification.md` records seven selected Wyrd regressions/source-proof tests passing and all 13 dependency common pruning tests passing. These support the inspected source; this reviewer did not independently launch Cargo commands, avoiding concurrent target-directory builds. The actual dependency pruning warning-capture test and the SDK journey were inspected in source; their historical RED/GREEN results are recorded in the task evidence. The orchestration's broader rule/gate assessment remains separate from this domain verdict.

The checked scenarios distinguish restoration of pruning from unchanged rows: the mixed fixture proves an exact excluded batch stays excluded while unknown and inexact batches remain; a separate row-by-row reference catches false full matches. The dependency tests distinguish unknown-bound null slots from exact typed-null scalars. Diagnostics never format a `ScalarValue` or query literal.

No exhaustive type-space, full workspace test, fresh journey run, or original-incident route reconstruction was performed by this reviewer. The source inspection establishes no additional persistent-state, tenant-proof, or lifecycle change within this diff; it does not claim a repository-wide audit.

## Findings

Empty. No remediation is proposed.
