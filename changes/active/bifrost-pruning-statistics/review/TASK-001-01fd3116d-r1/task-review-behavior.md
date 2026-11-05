# TASK-001 behavior review

Overall: **FAIL**. Proposed finding: **BEH-001**, incomplete mandatory dependency verification. No source-proven material runtime defect or unearned production machinery found.

## Subject and coverage

First acceptance audit, with no prior implementation acceptance carried forward. Wyrd base `d7de12a74c4bdbf93e727cc2383147e34298f689`, candidate `01fd3116d2501d89317073a2abb547818a23b4a8`; DataFusion cumulative base `19796a13ed46877cf998013886947b28a6f774b3`, candidate `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.

Read actual cumulative Git diffs, approved revision-3 spec, original task, scope map, Wyrd root rules, agent_docs/index.md, rust.md and maintainer-style.md, changes/spec-driven-development.md, DataFusion AGENTS.md and contributor index, and applicable Oracle/source-authority/resource sections of architecture/bifrost-design.md. Wyrd has no CodeGraph directory. No nested AGENTS.md applies to the changed source. Both committed subjects remain unchanged; untracked Wyrd review reports are outside the subject.

Expanded the supplied map through:

- DataFusion PrunableStatistics min/max, unchanged row/null counts, existing all-known/empty/schema tests, ScalarValue::iter_to_array strict primitive/timezone/decimal conversion.
- PruningPredicate::build_statistics_record_batch, which substitutes null arrays for unavailable bounds and casts to the required statistics type; FilePruner::try_new, which uses the same adapter for per-file pruning; PartitionPruningStatistics empty handling.
- Wyrd scribe/memtable.rs::batch_statistics and capture-statistics downgrading: primitive bounds use existing accumulators, unsupported bounds stay absent, filtered captures become inexact.
- BatchPruning::{keep,fully_matching}, ScanSizer and MemtableSource::{pruned,Merged::try_new,partition_statistics}: same dependency owner drives exclusion and negated full-match; exact zero null counts remain required for full-match; filtered output statistics remain inexact.
- LiveTableRoutes::selected_by, LiveScribeExec::resolved and LiveFragmentRead::into_stream: route interval producer, partition assignment, re-pruning, and actual fragment construction/dispatch loop.
- oracle/exec.rs existing per-file tenant proof and merged-statistics downgrading; only private bound-producer visibility changed.
- published.rs SDK journey, canonical span/cluster fixtures and production telemetry capture; manifest, lockfile and codec pin changes.

## Acceptance matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, AC-001: exact min/max survive absent, inexact, missing-entry neighbors in either order | DF exact_column_statistics maps unavailable entries to column-typed nulls and preserves container positions | partial_statistics_preserve_exact_bounds tests exact positions/types/values in both orders; task D1 RED/GREEN; review-time common pruning module 13 PASS | PASS |
| REQ-002, AC-002: empty/all-unknown/unknown-column quietly unavailable | Lookup returns Ok(None); no-exact preflight skips conversion; wrapper logs only Err | empty_statistics_do_not_warn, empty_containers_prune_without_warnings and W2; common module and W2 review-time PASS | PASS |
| REQ-003, AC-001: type parameters and string representation survive | Exact values remain untouched; existing strict iter_to_array receives schema-typed unknowns | Matrix covers integers, zoned ns timestamp, Utf8/Utf8View/LargeUtf8, Decimal128(20,4), both orders | PASS |
| REQ-004, AC-002: genuine conversion errors remain conservative and scrubbed | Error returns unavailable; reason reconstructed from types, column and bound; raw scalar error discarded | incompatible_statistics_have_scrubbed_diagnostics checks both bounds/orders and primitive, timezone, decimal and string incompatibilities; review-time PASS | PASS |
| REQ-005, AC-003: all existing direct consumers inherit shared fix | BatchPruning, route selection and FilePruner still use PrunableStatistics; no parallel algorithm | W1/W2/W3 and existing source-proof regressions; seven selected Wyrd review-time tests PASS | PASS |
| INV-001/002: preserve matching rows, duplicates, null semantics, no fabricated evidence | Unknown/inexact stays unavailable; full-match retains exact-zero-null requirement; uncertain batches retain filter | W3 reference-filter equality, kept batch/matched masks and sizing; SDK exact expected rows including SQL-null and negative combinations | PASS |
| REQ-002, AC-003: both empty-route paths finish without warning or dispatch | W2 resolves globally removed routes and empty partition shares; into_stream constructs and dispatches only inside returned-route loop | W2 route-count/warning assertions and span event-budget guard; review-time PASS; journey empty window opens no object | PASS |
| REQ-006, AC-004: real SDK Observe journey and incident shape | Real three-node server cluster with persisted/live spans; time, service, numeric kind and operation predicates | Task J1 original-pin warning RED and corrected GREEN; exact ordered expected rows, foreign tenant, negative/null cases; controlled reproduction and historical route-count limit stated | PASS |
| INV-003: preserve source/tenant authority and aggregate proof | Source selection/projection/captures and footer validation unchanged; merged persisted statistics remain inexact | Existing footer tenant refusal review-time PASS; task existing time_column_range_prunes_published_files journey PASS; J1 tenant separation | PASS |
| INV-004, AC-005: preserve ownership/contracts and compatible immutable pin | All eleven patches, lockfile and codec move together; no API/schema/data-format/feature/dependency addition | Static diff and pin inspection; task fingerprint/check:deps evidence | PASS |
| INV-005, AC-003: bounded work/memory and no added IO | Pure scalar conversion plus linear preflight, no IO; no-evidence skips array construction | Task 1024-container memory/time table; existing all-known bounds tests, mixed keep-mask proofs and conservative all-unknown source path | PASS |
| AC-005 and task owner checks: required dependency checks pass | DF AGENTS requires full format, all-target/all-feature workspace clippy and extended suite | Task reports only two-crate clippy, unresolved fmt drift, and full extended suite not run | FAIL: BEH-001 |
| AC-006: bounded adjacent review | Traced producer precision, typed nulls, missing entries, schema/array alignment, parameterized conversion, predicate/file/full-match consumers | Existing owner tests plus new matrix; no independently confirmed additional runtime defect | PASS |
| Non-goals: no Wyrd workaround, extra IO/collection, UI, migration or broad rollback | Actual cumulative source changes are the dependency correction, compatible pin, guard removal, private visibility and tests | Full two-repository diff inspection | PASS |

## Failure paths, ownership and maintainability

Ordinary absence now exits before conversion. Genuine array conversion failures remain local unavailable-statistics results with warnings; predicate evaluation failure and Wyrd statistics-count mismatch retain conservative fallback. No process lifecycle, persistence, grant, cancellation, source authorization or fragment transport behavior changed. Conversion remains linear; diagnostic rescanning occurs only on failure.

The existing dependency capability owns the correction. Its private exact_column_statistics helper separates a testable scrubbed outcome from the existing logging wrapper; it does not duplicate pruning or add production configuration. Wyrd reuses its existing bound producer, fixtures, telemetry and source owners. No second reader, generic Wyrd workaround, unnecessary abstraction, dependency or configuration knob was found.

The W2 route-count assertion was checked against the actual dispatcher loop rather than requiring a new mock: fragment construction and dispatch cannot run for zero returned routes. Row-result success is not treated as restored-pruning proof: W1/W3 separately establish exclusion and conservative full match.

Rejected hypothesis: a homogeneous array of exact scalar values that differs from schema does not automatically establish a new bug in this patch. That behavior predates this change; PrunableStatistics callers promise a schema applicable to their containers, and the downstream predicate deliberately casts statistics arrays to required type, including normal Parquet representation differences. No changed supported producer establishes incompatible row bounds on that path. Requiring unconditional schema equality would strengthen behavior without a source-proven supported-use regression. Actual internally incompatible mixed bounds are covered and fail conservatively with scrubbed diagnostics.

## Proposed finding BEH-001

Classification: **VIOLATION**, mandatory dependency verification incomplete.

Obligation: AC-005, task Owner checks and non-TDD proof at line 358 onward; DataFusion AGENTS.md lines 14–27 and 46–52. Wyrd forbids waiving red gates on the basis that failures predate the task.

Exact evidence: task TASK-001-dependency-owned-pruning.md:471 reports clippy for common/pruning only, says schema_rewriter.rs format drift was left untouched; line 473 explicitly records that the full fork extended suite was not run. DF AGENTS requires cargo fmt --all, workspace cargo clippy --all-targets --all-features -- -D warnings, and its stated extended workspace test command. No available evidence establishes these gates passed for the immutable dependency delivered through every patched package.

Consequence/materiality: this candidate has an explicitly incomplete delivery gate. This is not a claim that untested code contains a runtime defect. Focused common/pruning and Parquet checks, however useful, cannot replace separately mandatory fork checks.

Smallest correction: run and record the existing required fork format, workspace clippy and extended-test commands on its toolchain. Fix failures at their owners without weakening checks. If fixes change the dependency revision, update Wyrd patches/lockfile/codec consistently and repeat only affected integration proof. No new benchmark framework is justified: no suitable PrunableStatistics local benchmark was identified, and bounded work/memory observations already address the approved cost question; honor applicable existing benchmark requirements if one is found.

Burden: existing required execution plus minimum necessary source correction; no API, new framework or invented broad test requirement. Neither deletion of the correction nor a review waiver satisfies the task. Closure requires command results tied to the final immutable dependency, consistent Wyrd consumption, and relevant regression checks when source changes.

## Verification limits

No builds or tests run by this reviewer, per orchestration. Read review-time verification.md: seven selected Wyrd regressions/source proofs and thirteen dependency common-owner tests PASS. Historical RED/GREEN and real SDK journeys are available as task execution evidence; they were checked against source rather than independently rerun here. Full mandatory fork gates remain the outstanding risk. No source or packet changes made.
