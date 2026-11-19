---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-variant-file-reads
spec_revision: 2
requirements: [REQ-005, REQ-006, AC-004, AC-005]
depends_on: [TASK-001, TASK-002]
---

## Outcome and Value

Use L1 to establish whether the consolidated execution actually improves learned
Variant queries in memory and staging. Compare identical logical work across
memory, staged and published Parquet sources, and compare the unchanged main
baseline with the integrated candidate. Report canonical-versus-learned results
separately so a claim of storage adoption cannot substitute for query benefit.

The final deliverable is measured CPU, latency and reconstruction evidence with
an explicit conclusion for memory and staging. A valid benchmark with unchanged
performance does not complete the spec. Published Parquet remains the shared-
reader and correctness reference; this task adds no Parquet speedup threshold.

## What Changes and What Must Stay True

Own only the necessary L1 workload/declaration/measurement extensions, their
tests, execution of matched comparisons and the comparison report. Tasks 001/002
own production query repairs. Run this task after their correctness evidence
passes. Benchmark harness preparation and baseline runs occur here, even though
the baseline production source predates the repair.

Keep full existing Bifrost L1 writes, hot and Parquet reference workloads and
their target reporting intact. Add explicitly labelled matched Variant diagnostic
cases through the current driver when the references are not directly comparable.
Do not silently compare the existing half-fixture hot query against the full
published fixture or rewrite historical reference workloads to appear comparable.

For each compared query, keep keys, values, rows, selectivity, SQL shape, result
expectation, clients, repetitions and actual execution settings matched. Record
source placement, actual layout coverage, cache/warmup state, concurrency and
binary/source identities. Expose tier-specific IO costs rather than hide them in
Arrow decoded-byte figures. Obtain canonical data through ordinary cold-generation
behavior and learned data through actual claim teaching/new-generation adoption;
add no production source/layout toggle or ingest-policy change.

Preserve the current benchmark contract: correctness/integrity determines
VALID/INVALID, while existing performance targets remain met/missed/unsettled and
do not change exit status. The spec's improvement conclusion is a separate
change-acceptance judgment. Smokes, selected subsets, profiled or dirty runs
retain their declared provenance and do not masquerade as unchanged full reference
records. Do not add a new benchmark engine, record format or calibration system.

The cost is release builds, real isolated Postgres/server runs and repeated
measurements on an otherwise idle host. No synthetic host load, L2 competing-load,
gateway or resource-scaling qualification belongs here. No commits or pushes are
authorized by this task.

## Approach

1. Inspect the current L1 Variant cases and their controls. Extend the existing
   workload owners with the minimum explicitly labelled matched diagnostic matrix
   and exact expected answers, preserving every reference case.
2. Test the measurement preparation, actual tier/layout observations and evidence
   completeness. Reuse Tasks 001/002's owner-level work observations; add only
   bounded in-owner diagnostic support if existing evidence cannot show the work.
3. Establish baseline and candidate production identities. Use identical benchmark
   harness semantics for both; if baseline needs new diagnostic harness support,
   record that patch separately and do not backport the query repair into it.
4. Run full Bifrost L1 reference experiments and matched Variant comparisons on
   both revisions, serially on the same host. Repeat invalid/noisy measurements
   through the existing experiment rules; preserve all results and omissions.
5. Compare within-tier baseline/candidate and canonical/learned observations,
   account for variation, and report whether both memory and staging satisfy the
   performance objective. Route a demonstrated production defect to its owning
   implementation task; do not weaken acceptance to finish the report.

## Ordered Implementation Scenarios

### Scenario 1 — Matched Variant workloads measure the same logical work

**Behavior.** The diagnostic matrix contains covered scalar projection, covered
filter-only COUNT, whole-root output and residual/unshredded fallback controls.
The same logical rows, keys/values, selectivity, query shape and exact answers are
used on all three tiers and for canonical/learned data. Whole-root/fallback cases
are correctness and cost controls; the gain claim concerns covered leaf-focused
queries. Existing L1 reference step IDs, scope and target semantics remain intact.

**RED.** Add planned workload-equivalence test
[V1](#verification-and-evidence). Compare generated logical inputs, SQL/bounds and
expected answers across tiers/layout cases, including missing/null/residual values.
It must fail when hot halves differ from the published workload or a learned case
changes values/selectivity. Run existing reference declaration proof V3 alongside it.

**GREEN.** Extend the current L1 generator/query/declaration owners to supply
matched diagnostics without modifying reference workloads. Rerun V1/V3 and the
existing exact span-answer test V4.

**REFACTOR.** Share the existing deterministic generator and answer logic; remove
duplicate query descriptions without adding a general benchmark framework.

### Scenario 2 — Incomplete or incorrectly placed evidence cannot support a gain

**Behavior.** Preparation proves actual tier and layout before measurements;
the record preserves matched controls, repetition-level CPU/latency, reconstruction
evidence and measurement availability. Wrong counts, failed placement, missing
required evidence or invalid timing invalidate the comparison. Known zero remains
distinct from unknown. Valid but unchanged/noisy performance remains an unmet
spec objective without changing the existing experiment exit semantics.

**RED.** Add planned L1 evidence test [V2](#verification-and-evidence) using the
capacity binary's existing declaration/scripted-workload proof owner. Exercise
wrong placement/unproven layout and missing measurements, alongside a complete
record and an unchanged-performance record. Assert evidence availability and
existing validity/target behavior; missing observations must not become zero or
a performance success. Test any new executable evidence handling through TDD;
the final human-readable interpretation needs no invented classifier.

**GREEN.** Wire the minimum actual source/layout/work observations into existing
preparation and records, preserving refusal/measurement semantics. Rerun V1–V4.

**REFACTOR.** Reuse existing experiment validity and timing records, eliminating
duplicate collection. Keep additional diagnostics bounded and separately labelled.

## Acceptance Criteria

| Obligation | Proof and failure observation |
|---|---|
| AC-004 | V1–V4 and recorded controls prove equal logical inputs, rows, selectivity, SQL and actual settings. Unmatched workloads or unproven tier/layout invalidate the comparison. |
| REQ-005; AC-004 | V2 and Tasks 001/002's work evidence preserve unknown/zero and byte meanings; refused/stale work must not be presented as successful source work. |
| REQ-006; AC-005 | Full repeated matched L1 measurements show reduced CPU per query and query latency outside observed run-to-run variation, separately for covered memory and staged cases against baseline. A valid exit, an average hiding a failed tier, or plan/layout changes alone fails acceptance. |
| AC-004/005 regression controls | Full existing writes/hot/Parquet results retain their workloads, targets, integrity and durability checks. Report write/staging cost and whole-root/fallback results, plus Parquet results without imposing a new Parquet gain target. |

### Proof Strategy for the measurements

TDD covers new executable workload/evidence logic above. Actual performance proof
is repeated real execution, not a unit assertion about synthetic timings.

Use main `5234961d51798581a026834cfffd5efaadc226b4` as the unchanged production
baseline and the integrated Tasks 001/002 candidate as the repair. Establish a
complete identical diagnostic harness for both sources before comparing. Record
the source diff, fork/dependency revision, release binary identities and harness
identity; do not run two revisions from a shared overwritten release binary.
Checkout/worktree mechanics remain caller-owned and require no commits here.

Compare memory and staged separately using repetition-level CPU per completed
query and the same latency percentile in each pair, under the same client count.
Keep the existing L1 three-repetition method and measured precision. Show the
spread and gains; repeat under the existing method when their ranges/precision
cannot distinguish a gain. No chosen percentage target is added. Inconclusive or
unchanged results remain unmet; additional observations cannot erase invalid runs.

The report records invocation/record/artifact paths, exact answers and placement,
layout coverage, per-repetition CPU/latency, reconstructed roots/leaves or equivalent
executed-work observations, compressed IO separately from decoded bytes, actual
settings/cache state, and write/staging cost. Connect improvements to the execution
cause established in Tasks 001/002. Keep diagnostic results separate from reference
history and list any selected-step omissions.

## Implementation Reference

- `crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs::HOT_READS`,
  `PARQUET_READS`, `Read`, `Half`, `hot_fixture` and `read_fixture` own current
  L1 declarations. Hot uses two disjoint half-ranges, while published reads use
  the full fixture; these are not automatically matched cross-tier diagnostics.
  Preserve their current reference semantics while adding the required comparison.
- `query/isolated.rs::Isolated::prepare_hot` publishes a teaching cohort, verifies
  learning and places staged/memory halves with existing benchmark controls.
  `Workload` preparation for published reads loads and settles files. Reuse these
  complete placement workflows and require observations of actual placement/layout.
- `query/telemetry.rs::Telemetry::shredded_filter`, `bounded_filter` and
  `long_tail_filter` generate deterministic exact answers. The hot filter has
  time bounds while the published filter currently does not. Extend this owner
  for matched query shapes rather than comparing their unrelated timing labels.
- `capacity/main.rs::tests` already tests complete L1 declaration coverage,
  latency/CPU accounting and record/exit behavior with scripted workloads.
  `query::tests` and `query::telemetry::tests` own pure fixture/answer checks.
  These tests compile in the confirmed `wyrd-testing` binary target `capacity`.
- `capacity/levels.rs::{assemble,execute}` and
  `wyrd-testing/src/experiment.rs` own source/build/host controls, repetitions,
  validity and publishing. Reuse these records and their existing diagnostic
  storage behavior, without changing the persisted record schema.
- `mise.toml::bench:build` builds the measured server separately with only its
  existing `bench` feature, then the driver. The `bench:l1:bifrost` task runs
  writes-parquet, hot and parquet; its full default coverage is the reference
  suite. No broad Verification/Gateway or L2 run is necessary for this change.

Likely changes stay in the current capacity query/test owners, benchmark method
documentation and bounded evidence support in the existing query owner if needed.
Production repairs return to Tasks 001/002. No new public API, options, engine,
dependency or calibration record is needed.

## Verification and Evidence

V1/V2 are **planned** names in confirmed existing binary/module owners. Confirm
their actual selectors after adding them before RED. Unit proofs need no Postgres.

**V1 — planned exact logical workload equivalence.**
```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=query::tests::variant_comparisons_use_identical_logical_workloads)'
```

**V2 — planned L1 evidence/refusal/availability proof.**
```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::variant_l1_evidence_requires_matched_proven_sources)'
```

**V3 — existing complete reference declaration proof.**
```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::level_one_declares_complete_engine_coverage)'
```

**V4 — existing deterministic span answer proof.**
```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=query::telemetry::tests::span_answers_count_the_generated_spans)'
```

**V5 — owner regression proof and required static checks.**
```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'not test(/^tests::pg_tests::/)'
mise run fmt
mise run lints
```

**V6 — full baseline and candidate Bifrost L1 runs.** Execute the same command
once for each identified source/build on the same otherwise idle host. The task
build dependency creates the release server/driver; the experiment owns isolated
repository Postgres, setup, measurements, stop/drain and exact reconciliation.
```bash
mise run bench:l1:bifrost
```

If the source is uncommitted, use `mise run bench:l1:bifrost --allow-dirty` instead
and retain the exact diff/build identities; its diagnostic location/provenance
must remain explicit. Do not commit to satisfy a benchmark clean-source check.
The matched Variant diagnostic cases added here must be exercised through the
same L1 owners in both builds, with their exact implemented invocation recorded
before measurement. No new caller option is required; private case registration
and presentation remain implementation-owned. A selected-step or smoke run may
diagnose preparation but cannot substitute for the full reference evidence.

Archive the comparison report as `../l1-comparison.md` beside the spec and task
packet, linking raw records/artifacts rather than copying binary output into Git.
Run `git diff --check` and inspect tracked/untracked changes. Integrated change
review owns `mise run verify:bifrost` once for the whole candidate; this task
does not repeat every correctness lane as a benchmark prerequisite checklist.

## Material Stop Conditions

- Matching workloads requires a new public source/layout control, altered learning,
  ingest/durability behavior, record format or replacement benchmark framework.
- The baseline cannot execute the same measurement method without production
  repair backports. Document the conflict; do not label unlike runs comparable.
- Complete valid matched evidence shows no repeatable CPU/latency improvement for
  either required tier. The objective remains unmet; return the evidence for a
  maintainer decision as required by the spec, without weakening the claim.
- Required hardware or external authority has no repository-managed substitute.
  Preserve completed evidence and report the precise unavailable proof.

Noisy/invalid runs and missing local repository-managed setup are recoverable
work, not grounds to claim success or to abandon the applicable recovery path.

## Authority Links

- [Approved specification](../spec.md), revision 2;
  [memory execution](TASK-001-memory-variant-execution.md) and
  [file execution](TASK-002-shared-file-variant-reads.md).
- [L1 method, workload and verdict contract](../../../../benchmarks/README.md).
- [Bifrost measurement meanings](../../../../architecture/bifrost-design.md#measurement-meanings).
- [Execution discipline](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [Rust](../../../../agent_docs/rust.md).
