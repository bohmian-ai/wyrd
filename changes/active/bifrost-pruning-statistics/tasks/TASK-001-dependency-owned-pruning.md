---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-bifrost-pruning-statistics
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006]
depends_on: []
---

# Correct pruning in DataFusion and verify Wyrd's integration

## Outcome and Value

Queries use available exact bounds even when neighboring files or batches have
unknown bounds, and empty live-route selections finish quietly. For example,
an exact batch entirely before a requested time window is excluded while an
unknown-bound batch stays eligible. An output partition with no live routes
returns no rows and dispatches no fragment, without conversion warnings.

Both generic corrections belong in DataFusion. Wyrd consumes the corrected
dependency and proves that its source adapters preserve rows, pruning, and
tenant checks. This is one delivery outcome across the existing fork and its
Wyrd consumer, not separate UI or storage-statistics projects.

## What Changes and What Must Stay True

Execute in a **new Wyrd worktree based on main**, independent of UI/TASK-004.
Planning inspected main `d7de12a74c4bdbf93e727cc2383147e34298f689`; its pruning
owners, fork pins, Oracle test modules, and testing authority match the sources
used here. At execution, use the caller-selected main revision and revalidate
those facts. Copy only this change packet into the worktree. Do not branch
from, merge, or cherry-pick the UI work, and do not require its development
server, credentials, seeded data, or harness additions.

Use a separate writable DataFusion fork worktree rooted at Wyrd's compatible
pin `19796a13ed46877cf998013886947b28a6f774b3`, or its isolated correction
`8d6d6bf58c3a64242d2193a0cff0b8ad3a050599`. The latter differs only in the
shared conversion file and its regression test. Do not modify Cargo's cached
checkouts or reuse unrelated uncommitted work in another DataFusion checkout.
Branch names, worktree placement, scheduling, and Git operations remain
caller-owned. This packet does not authorize commits, pushes, or publication.

An immutable corrected fork revision reachable from the declared Git source
is required for the final Wyrd pin. Prepare and verify the correction locally;
if that revision is not yet available, report the exact publication dependency
after completing independent proof. Do not ship a path override, a local-only
Git reference, or an unrelated fork upgrade to bypass it. Existing published
typed-unknown correction alone is insufficient for empty-input behavior.

Preserve these boundaries:

- Unknown or inexact evidence alone cannot exclude a container, prove all its
  rows match, or become an assertion that the stored rows are SQL null.
- Column types retain their parameters and representation; container count and
  order stay aligned. A genuine incompatibility remains conservative and
  diagnostic without disclosing stored values or query parameters.
- DataFusion owns generic pruning, statistics conversion, and empty-input
  semantics. Add no Wyrd guard, pruning wrapper, alternate reader, or copied
  generic algorithm to compensate for the dependency defects.
- Iceberg and Parquet retain their existing file metrics and pruning roles.
  Wyrd retains tenant proofs, pinned cuts, live captures, source routing, and
  resource/admission policy. Preserve footer proof before decoding and prevent
  aggregate shortcuts from bypassing it.
- No public API, SDK behavior contract, schema, data format, migration,
  dependency addition, Cargo feature, UI code, or statistics-collection change.
  Do not restore the broader rolled-back TASK-008 changes or relocate every
  generic Wyrd helper as part of this correction.

Conversion remains linear with no added IO. Record bounded work and returned
statistics-array memory for all-known, mixed, and all-unknown inputs; typed
unknown arrays may cost memory while avoiding unnecessary reads. Empty-input
handling saves conversion/logging work only. Timing is observational, without
a percentage gate or synthetic host load.

## Approach

1. Revalidate main, the writable fork base, package/test owners, and applicable
   repository rules; retain the approved packet independently of UI work.
2. Establish failing dependency regressions for partial evidence, no-evidence
   inputs, real pruning predicates, and scrubbed conversion diagnostics.
3. Correct those behaviors at the existing DataFusion owners, restoring the
   isolated mixed-bound fix where applicable and keeping its compatible API.
4. Consume the corrected immutable fork revision consistently across Wyrd's
   patched packages and lockfile; restore source-owner regression coverage.
5. Prove live-route, sizing/full-match, and real SDK query behavior using
   main's existing fixtures, then complete the bounded adjacent audit and
   focused checks. Record both repositories' revisions and evidence.

## Ordered Implementation Scenarios

All new executable behavior uses RED/GREEN/REFACTOR. Planned names below give
exact selectors in [Verification and Evidence](#verification-and-evidence).
After adding a test, confirm its actual selector and selected count before RED;
a compile/setup failure or an empty selection is not the expected regression.

### Scenario 1 — Usable bounds survive partial evidence

**Behavior.** Minimum and maximum statistics retain their exact entries beside
absent, inexact, and missing-column-entry bounds, irrespective of input order.
Unavailable entries retain the column type without becoming data assertions.
Cover integers, nanosecond timestamps with timezone, string representations in
use, and decimals with precision and scale. An out-of-window exact container
can be excluded while an unknown-bound container remains eligible.

**RED.** Add the planned dependency test
`pruning::tests::partial_statistics_preserve_exact_bounds` using [D1](#d1--partial-statistics).
Against the original pin, the mixed typed/unknown case must fail because its
requested bound array is unavailable. The prior fork fix and test are reuse
evidence, not proof that every matrix case currently passes.

**GREEN.** Restore or extend the existing shared conversion to satisfy the
matrix. Confirm the exact entries, unknown positions, type parameters, and
container order, not only successful array construction.

**REFACTOR.** Keep conversion at its dependency owner and reduce duplicated
handling only while the matrix remains green. Do not broaden types or promote
inexact evidence to obtain a passing test.

### Scenario 2 — Empty inputs and ordinary absence are quiet

**Behavior.** A real DataFusion pruning predicate over zero containers returns
an empty selection without conversion warnings. Entirely unavailable bounds
and unknown columns remain unavailable quietly. Existing exact typed-null
bounds preserve their representation. Wyrd's route caller needs no workaround.

**RED.** Add planned dependency tests
`pruning::tests::empty_statistics_do_not_warn` and
`pruning_predicate::tests::empty_containers_prune_without_warnings`, using
[D2](#d2--ordinary-absence) and [D3](#d3--empty-predicate).
The current pin returns the expected empty/unavailable result but emits the
conversion warning; the diagnostic assertion must fail. Include the four
reported predicate columns, with numeric `kind`, as a representative query.

**GREEN.** Correct ordinary empty/no-evidence behavior inside DataFusion while
keeping partial evidence useful. Rerun scenario 1 and these tests; a logger
filter or a changed Wyrd adapter is not a fix.

**REFACTOR.** Reuse existing dependency ownership and test isolation. Avoid a
new general logging harness or production option solely for this test.

### Scenario 3 — Genuine incompatibilities remain safe and diagnosable

**Behavior.** Actually incompatible exact statistics remain conservative.
Diagnostics distinguish or generically identify bounds without labeling a max
failure as min, and convey a useful failure reason without scalar values or
query parameters. Quiet legitimate absence must not hide genuine defects.

**RED.** Add the planned dependency test
`pruning::tests::incompatible_statistics_have_scrubbed_diagnostics`, using
[D4](#d4--genuine-incompatibility). Exercise incompatible representations or
type parameters and assert unavailable bounds, diagnostic accuracy, and
non-disclosure. The existing hard-coded min-only message must fail the
diagnostic assertion. Preserve the dependency's error API.

**GREEN.** Correct the shared diagnostic behavior without silently coercing
incompatible values, returning fabricated bounds, or logging the raw scalar
conversion error. Rerun scenarios 1–3.

**REFACTOR.** Keep genuinely useful diagnostics in the existing owner. Do not
add a new public error contract, telemetry family, or warning-suppression knob.

### Scenario 4 — Wyrd's existing sources inherit the correction

**Behavior.** Wyrd receives the corrected library semantics without a new
generic production workaround. A globally pruned live-route set and an empty
execution partition stay empty, dispatch nothing, and emit no conversion
warning. Mixed bounds preserve exclusion, conservative sizing, and full-match
proofs. Returned rows match execution without the statistics shortcut.

**RED.** Add or restore the planned Wyrd tests in [W1](#w1--mixed-nanosecond-bounds),
[W2](#w2--empty-live-routes), and [W3](#w3--conservative-memory-semantics).
Before pin integration, the restored mixed-bound test must fail on unavailable
minima, and the empty-route warning assertion must fail on a valid empty
selection. New safety assertions that already pass are verification-only;
do not manufacture failures for unchanged invariants.

**GREEN.** Consume the corrected fork and prove the production source paths
through the existing Wyrd owners. Keep an excludable, matching, and
unknown-bound container distinguishable in the evidence. Record bounded
all-known/mixed/all-unknown array memory and exclusion/work observations.
Rerun scenarios 1–4 against the final dependency revision.

**REFACTOR.** Keep Wyrd tests focused on integration and source authority.
Remove a workaround only if it is made redundant by the dependency contract
and its safety proof remains; do not redesign existing source orchestration.

### Scenario 5 — Real clients retain exact rows across persisted and live data

**Behavior.** A real Rust SDK query against `WyrdTestServer` and repository
Postgres exercises Observe's time, service, numeric kind, and GenAI operation
predicates across persisted and live sources. Matching, nonmatching, and
SQL-null cases return exact expected rows without duplication or cross-tenant
data. Restored pruning and quiet empty execution are independently evidenced.

**RED.** Add the planned journey in [J1](#j1--sdk-observe-predicates), reusing
the existing Oracle published/live fixtures and canonical-signal producers.
Run its focused command on the original pin. Its warning or lost-pruning
assertion must expose the defect; unchanged row/security assertions may
already pass. A successful terminal with an empty protocol `warnings` array
does not prove the server log was quiet. Do not use the running UI dev server
or depend on historical `/tmp` probes.

**GREEN.** Run the same journey with the corrected pin, retaining deterministic
source-state synchronization, reference rows, diagnostic evidence, and
existing tenant/terminal assertions. Demonstrate the failing shapes on a
controlled fixture rather than claiming to reconstruct unavailable historical
route counts. Rerun the [existing source-proof regressions](#j2--existing-source-proof-regressions).

**REFACTOR.** Reuse main's fixture, telemetry, and source-control seams. Any
small missing private test-support seam stays with its existing owner; do not
import UI harness changes, introduce a new public API, or replace real sources
with an engine-only mock.

## Acceptance Criteria

| Spec obligations | Owner and proof |
|---|---|
| REQ-001, REQ-003; INV-001, INV-002; AC-001 | DataFusion matrix plus Wyrd mixed-bound regression retain correct typed positions and exclude only provably disjoint containers. Wrong type/order, discarded usable evidence, or exclusion by an unknown bound fails. |
| REQ-002; AC-002 | Dependency statistics and predicate tests assert ordinary no-evidence results and no conversion warning. Wyrd route tests prove both empty paths with no fragment dispatch. A result-only assertion is insufficient. |
| REQ-004; AC-002 | Dependency incompatibility test retains conservative results, useful bounds diagnostics, and scalar/parameter non-disclosure. Silent genuine failure or misleading min/max wording fails. |
| REQ-005; INV-001, INV-002, INV-005; AC-003 | Existing batch, route, sizing, and full-match consumers are verified against the corrected owner. Fixed all-known/all-unknown fixtures incur no additional conversion IO or container scans; bounded work/memory evidence accompanies the mixed case. |
| REQ-006; INV-001, INV-003; AC-004 | Real SDK/server journey covers all four predicate columns, persisted/live rows, negative/null cases, and tenant/source authority; separate evidence proves quiet logs and restored exclusion. Record build revisions and the limit of original-incident attribution. |
| INV-003, INV-004, INV-005; AC-005 | Consistent immutable pins and lockfile, focused source-proof regressions, format/lint/boundary checks, and diff audit pass. No UI dependency, source-proof bypass, new public surface, migration, or local path dependency ships. |
| AC-006 | Bounded review covers direct statistics adapters, empty inputs, mixed precision, typed nulls, schema alignment, and parameterized types. Each new defect has a reproducer and consequence; unrelated material changes return for separate specification. |

## Implementation Reference

Paths identify existing owners, not a prescribed private layout.

| Existing owner / inspected consumers and tests | Missing behavior and selected extension |
|---|---|
| DataFusion `datafusion/common/src/pruning.rs`: `PrunableStatistics::get_exact_column_statistics`, min/max adapters, existing statistics tests; `scalar/mod.rs::iter_to_array` | Partial statistics introduce untyped nulls; empty conversion warns; max failures say min. Extend the existing adapter and tests. Keep scalar array type checking strict. Fork `8d6d6bf58` supplies the isolated typed-unknown correction and timestamp regression to reuse. |
| DataFusion `datafusion/pruning/src/pruning_predicate.rs`: `PruningPredicate::prune`, existing `tests`; `pruning/src/file_pruner.rs::FilePruner::try_new` | Zero containers are supported but can reach noisy statistics conversion. Correct the generic dependency path and prove it through the actual predicate, preserving single-file and incompatible-statistics behavior. No new Wyrd machinery. |
| Wyrd `scan_sizing.rs::BatchPruning::{keep,fully_matching}`, `ScanSizer`; `oracle/memtable_source.rs::{pruned,tests}` | Consumers need regression evidence that partial bounds retain exclusion without unsafe full-match or sizing claims. Restore the removed timestamp regression and extend source-owner tests; reuse current DataFusion predicates. |
| Wyrd `oracle/live.rs::LiveTableRoutes::selected_by`, `LiveScribeExec::{resolved,handle_child_pushdown_result}`; existing route tests | Empty shares expose the dependency defect. Verify both zero-route cases and warning silence through the unchanged adapter contract. Do not add another empty guard or wrapper. |
| Wyrd `oracle/exec.rs::{published_files,planned_files,hot_parquet_scan,verify_scanned_footer_tenant}`; `oracle::exec::tests` | Verify dependency integration preserves the unified persisted reader and per-file tenant proof. Use existing source-proof tests rather than replacing the scan. |
| `wyrd-testing` Oracle target: `tests/bifrost/oracle/main.rs`, `published.rs` and `support.rs`; `src/bifrost/{canonical_signals,telemetry,cluster}.rs` | Main already has real SDK readers, canonical span producers, source-state synchronization, and production telemetry capture. Add the narrow Observe predicate regression to the existing target. Current telemetry spans/metrics are not automatically a log-warning capture; establish explicit diagnostic proof using the smallest existing-owner test seam if necessary. |
| Workspace `Cargo.toml` DataFusion patch entries, `Cargo.lock`, `check:deps` | Update all existing patched DataFusion packages to one compatible corrected immutable revision and regenerate the lockfile through Cargo. Never hand-edit it or add a path patch. |

Production changes should be in the dependency; Wyrd normally needs its pin,
lockfile, and integration/regression tests. Contract/schema/codegen changes are
not expected. The prior rollback is evidence about a broad failed attempt, not
permission to restore it. Revalidate upstream `AGENTS.md`, contributor checks,
and applicable directory instructions in the writable fork worktree before
editing; the inspected pinned fork has root `AGENTS.md` and no nested one.

## Verification and Evidence

Commands below are **planned execution recipes, not checks run during planning**.
Use Wyrd's existing `mise` toolchain and discovered lanes. Run Cargo-backed work
sequentially per shared target directory. Dependency commands run in the
isolated DataFusion worktree; Wyrd commands run in the new main-based Wyrd
worktree. Confirm every planned selector after adding it. Exact `cargo test`
filters must report the intended selected test; zero selected tests fail the
proof. Wyrd nextest commands explicitly reject empty selection.

### D1 — Partial statistics

Planned dependency test; extends the existing `datafusion-common` library test
module and the isolated prior timestamp regression:

```bash
mise exec -- cargo test --locked -p datafusion-common --lib pruning::tests::partial_statistics_preserve_exact_bounds -- --exact
```

### D2 — Ordinary absence

Planned dependency test, including empty/all-unknown/unknown-column and exact
typed-null cases with diagnostic capture:

```bash
mise exec -- cargo test --locked -p datafusion-common --lib pruning::tests::empty_statistics_do_not_warn -- --exact
```

### D3 — Empty predicate

Planned test in the existing `datafusion-pruning` library's predicate test
module; proves the generic API works for zero containers:

```bash
mise exec -- cargo test --locked -p datafusion-pruning --lib pruning_predicate::tests::empty_containers_prune_without_warnings -- --exact
```

### D4 — Genuine incompatibility

Planned dependency test for conservative, accurate, scrubbed diagnostics:

```bash
mise exec -- cargo test --locked -p datafusion-common --lib pruning::tests::incompatible_statistics_have_scrubbed_diagnostics -- --exact
```

### W1 — Mixed nanosecond bounds

Planned restoration of the regression removed by `07a157cb9`; test source and
expected mask are available in `3b7f9d5af` independently of all UI work:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds)'
```

### W2 — Empty live routes

Planned inline route test, including a fully pruned selection and an empty
execution partition, with explicit warning and dispatch assertions:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=oracle::live::tests::empty_live_route_selections_do_not_warn)'
```

### W3 — Conservative memory semantics

Planned inline memory-source test covering partial bounds, reference rows,
sizing, and full-match conservatism. Reuse existing source-owner seams:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=oracle::memtable_source::tests::partial_statistics_preserve_conservative_filtering)'
```

### J1 — SDK Observe predicates

Planned test in the existing `wyrd-testing` `oracle` target and `published`
module. It uses repository Postgres, normal migrations, the real server, and
SDK; it is selected explicitly even if marked ignored like its neighboring
journeys. This setup mirrors the owning main lane without running that whole
lane:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all --no-tests=fail -E "test(=published::observe_predicates_preserve_rows_across_live_and_persisted_sources)"'
```

### J2 — Existing source-proof regressions

Existing selectors inspected in source. Together they preserve actual batch
exclusion, sizing, full-match/null behavior, and footer tenant refusal:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=oracle::memtable_source::tests::point_lookup_prunes_batches_outside_their_bounds) | test(=oracle::memtable_source::tests::range_filter_skips_evaluation_only_on_proven_batches) | test(=scan_sizing::tests::scan_sizer_counts_covered_columns_of_kept_batches) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row)'
```

Existing SDK/server journey verifies published and unpublished time pruning
through real storage, including objects that must not be opened:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all --no-tests=fail -E "test(=published::time_column_range_prunes_published_files)"'
```

### Owner checks and non-TDD proof

After focused RED/GREEN evidence, run the affected dependency modules (not the
entire fork workspace as an iteration loop):

```bash
mise exec -- cargo test --locked -p datafusion-common --lib pruning::tests::
mise exec -- cargo test --locked -p datafusion-pruning --lib pruning_predicate::tests::
```

At dependency delivery, satisfy the fork's applicable root `AGENTS.md` and
contributor checks, including its required format, lint, extended-test, and
applicable local-benchmark evidence. Those requirements do not authorize
committing or publication. Do not treat Wyrd checks as proof of fork-owned
checks, or treat this focused task list as a waiver of upstream authority.

For the affected Wyrd owners, broaden once after focused proof:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(/^oracle::live::tests::/) | test(/^oracle::memtable_source::tests::/) | test(/^scan_sizing::tests::/)'
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run check:storage:drift
mise exec -- git diff --check
```

Pin/lockfile consistency, base independence, ownership, the bounded adjacent
audit, and absence of production workaround/IO are static or verification-only
proofs. Record inspected direct call sites and any reproduction for an additional
finding. Do not manufacture RED for them. Compare array memory and exclusion
counts on bounded existing fixtures; do not add a benchmark service, dependency,
or synthetic host load. Record any timing with its limitations.

Record dependency base/corrected revision, Wyrd base, actual selectors and test
counts, expected RED failures, GREEN results, all-unknown/all-known comparisons,
scrubbed diagnostics, source-proof results, and final diff audit. Preserve
this packet through review. Complete capability-wide and cross-surface aggregate
verification once on the integrated candidate at change review; do not substitute
that later step for any focused task proof above.

## Material Stop Conditions

- A correct solution requires changing public/persisted contracts, source
  authority, tenant proof, resource policy, Iceberg semantics, or dependency
  ownership rather than correcting the approved generic DataFusion behavior.
- Compatibility requires a new dependency/feature or unrelated fork upgrade.
- Correct integration requires UI/TASK-004 code or a production change that
  relocates broad generic Wyrd logic. Re-evaluate the existing main owner first;
  ordinary private test-support gaps are implementation-owned.
- The final corrected immutable fork revision is unavailable from the declared
  Git source. Finish local correction and independent verification, identify
  the exact external delivery dependency, and do not claim pin integration or
  task completion.
- A new finding changes approved behavior outside partial-statistics or
  empty-input conversion/pruning. Keep the bounded reproducer and route the
  material change through specification authority.

No historical trace reconstruction is required when unavailable. Runtime/test
failures, old private names, and missing repository-managed local setup are
diagnostic work, not reasons to stop while an in-scope correction remains.

## Authority Links

- [Approved revision 3](../spec.md) and [investigation](../investigation.md).
- [Root rules](../../../../AGENTS.md), [Bifrost architecture](../../../../architecture/bifrost-design.md).
- [Specification workflow](../../../spec-driven-development.md), [execution discipline](../../../implementation-execution.md), [testing map](../../../../TESTING.md).
- [Dependency pins](../../../../Cargo.toml), [repository commands](../../../../mise.toml).
- The writable DataFusion fork's own `AGENTS.md` and referenced contributor
  guidance, loaded from that repository rather than copied into Wyrd authority.
