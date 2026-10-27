# Scoped discovery — review of the original findings

Subject: original `55f87cef1d74886a73cedab8d5c0960e915717a9...c9794a0b0c2182e93dcb8d936d253cb465620faa`; closure `c9794a0b0..cc0d9c86c049ee93cd3ee3f9012fbdd85c5b1b0a`.

This is a review of the claims in `../TASK-006-R1-batch-once-c9794a0b0/`, including its minor observations and verdict rationale. It is not a replacement implementation acceptance audit. No builds, tests, benchmark runs, source edits, or commits were performed. CodeGraph was consulted before code navigation. Immutable Git source/diffs, the approved revision-14 spec, the final approved batch-once task revision, the workflow, maintainer guide, and relevant Bifrost/security owner sections ground this report.

## Claim and acceptance matrix

| Claim or obligation | Independent evidence | Result / proposed disposition |
|---|---|---|
| F-1 / FIND-006R1B-1: separate intervention diagnostics are missing | Task Steps 1–5 GREEN and P5 diagnostic paragraph require allocations/copies/geometry and local-versus-peer retained memory; current evidence records correctness selectors and integrated timing, not those observations | CONFIRMED, open |
| Marking GREEN incomplete is an alternative closure for F-1 | Task acceptance mapping explicitly requires per-intervention diagnostic evidence | REJECTED as a correction; honest incomplete labeling is required but does not supply the proof |
| F-1 diagnostics require a P5 driver-budget decision first | Existing `levels.rs::select` accepts declared `--steps`; `prepass` forwards selection and strips only `--profile`/qualification; verification declares `direct-verify-c1` and `direct-verify-c64` | REJECTED; selected c1 profiling and bounded in-process allocation/memory observations do not require full c64 acceptance |
| F-2 / FIND-006R1B-4: missing removed-test mapping, incorrect capacity wording, wrong commit, zero-match selector | Closure diff corrects map, `<256 B` assertion wording, `b47082554`, and P3 selector; producer code uses common `StagedRow::project` | CONFIRMED historical record errors; closed statically |
| F-3 / FIND-006R1B-3: missing mandatory panic documentation | Original bodies index rows/cells and call `expect`; closure adds `# Panics` to both cited items | CONFIRMED; closed statically |
| O-1 / FIND-006R1B-2 mixed fallback is material DRIFT | Original branch can project one-row batches, but destination/schema grouping and producer variants make mixed staged/batch frames unreachable | REJECTED as a blocker; small deletion was reasonable and is now present |
| F-4: P5 acceptance is not demonstrated | Six clean JSON records for five experiments at `f50fdd9fb` are INVALID; verification records stop at prepass | CONFIRMED as incomplete acceptance; do not describe INVALID full verification as a valid measured latency failure |
| F-4 / verdict: approved work cannot resolve remaining failures, therefore specification revision is required | REQ-021 explicitly requires optimizing these engines until unchanged limits pass; task brings shared Scribe work into scope; no concrete unavoidable contract change is established | REJECTED inference; failures alone do not establish a required specification change |
| Invariant O-1: retained Variant capacity slack is acceptable because sibling weights use lengths | `Cell::owned_bytes` calls `EncodedVariant::encoded_bytes` (length); constructors retain upstream growing `Vec`s; task explicitly requires charging owned memory | REVISED to material accounting issue, see B-2 |
| Remaining implementation correctness, security, persistence conclusions | Narrow affected source checks support the original reasoning; full acceptance discovery and runtime revalidation were not assigned | Carried as prior evidence only; not newly certified |

## B-1 — Missing required diagnostic evidence remains open

Classification: MISSING. Preserve stable `FIND-006R1B-1`.

Authority: task Step 1 GREEN, Step 2 GREEN, Step 3 GREEN, Step 4 GREEN, Step 5 opening paragraph, P5 diagnostic instructions (current task lines 1452–1457), and acceptance mapping (1469–1471). These distinguish correctness tests from cost evidence and explicitly require separate intervention evidence.

The implementation table's Step 1–4 PASS entries cite tests. Those can prove representation/equivalence and the canonical payload seam; they do not measure allocation reductions, encoded geometry, per-route retained memory, or downstream merge work. The closure adds an explanation that profiling stopped at c64 smoke, without supplying the missing evidence. Marking these rows incomplete avoids overstating completion, but cannot close a required proof obligation.

The explanation that all diagnostics must wait for budget approval is unsupported. `crates/wyrd/wyrd-testing/src/bin/capacity/levels.rs:215–232` selects declared steps; `:245–263` forwards `--steps` into the smoke prepass while removing `--profile`. `verification/mod.rs::direct` declares `direct-verify-c1` separately from c64. The task explicitly classifies partial/profiled/dirty runs as diagnostics. Existing profiling can therefore target `mise run bench:l1:verification -- --steps direct-verify-c1 --profile`, subject to its unchanged validity checks. It need not claim c64 latency acceptance. S3 local/peer retained-byte proof also need not depend on public c64 throughput; a bounded instrumented execution of the existing route fixture can observe that seam. The sampler may not expose allocation counts, so use temporary bounded counters or existing allocation tooling only for the missing operands, rather than claim a CPU profile alone proves allocation counts.

Minimum correction: retain source/configuration identity and comparable before/after measurements at the four intervention commits, using selected diagnostics or bounded existing fixtures. Record S1 discarded-copy elimination, S2 allocation/copy/row-frame geometry, S3 local and peer retained memory separately, S4 gathered payload copy and downstream merge-work observations. Remove temporary probes afterward. Do not require one full L1 run per step, introduce permanent metrics, or change the driver guard. Honest incomplete labels are an interim record correction only.

## B-2 — The original Variant-capacity observation violates the explicit charging obligation

Classification: INCORRECT. This revises original `task-review-invariants.md` O-1, rather than discovering an unrelated boundary.

Authority: approved task Step 2: “Charge retained owned values until settlement/loss”; REQ-021: “Staged rows are admitted by the memory they own.” The original dismissal cites sibling length-based estimates instead of reconciling this explicit obligation.

Producer-to-consumer trace:

1. `EncodedVariant::from_serialize` (`variant.rs:218–234`) retains `(metadata, value)` from `VariantBuilder::try_finish` directly. The JSON path likewise retains builder output.
2. Installed parquet-variant 60 `builder.rs::ValueBuilder` owns a growing `Vec<u8>`; `append_slice` uses `extend_from_slice`, `append_u8` uses `push`, and `into_inner` returns that Vec. `VariantBuilder::try_finish` (`:976–984`) returns these buffers without shrinking. Compound values append headers and children incrementally, so capacity can exceed length. `EncodedVariant` intentionally owns this storage for retries.
3. `tables/staged.rs::Cell::owned_bytes` (`:59–65`) charges Variants through `EncodedVariant::encoded_bytes`; `variant.rs:388–389` returns only lengths. `StagedRow::owned_bytes` adds cell-array storage but cannot recover the omitted Variant capacity.
4. `scribe_outbox.rs::ScribeSink::weight` (`:1549–1561`) uses that row charge for audit, execution inputs, and result payloads. No separate charge covers the omitted spare Variant storage while queued/waiting/retrying/in-flight.

Reachable consequence: ordinary compound verification input/output values with grown buffers consume more retained memory than their outbox weight. Repeating them under backlog admits more live heap than the fixed byte budget accounts for. This is not an exotic corrupt input, and the requirement expressly protects this boundary. “About twice” is not a proven universal bound and is unnecessary to establish the violation. Sibling estimators do not waive this changed staging obligation.

Minimum correction: expose owned buffer capacity accounting from the existing `EncodedVariant` owner and use it at the staged-cell memory boundary. Keep `encoded_bytes()` as encoded lengths: sibling field/frame budgets use wire/value size and must not silently change to heap capacity. No shrinking copy, allocation registry, or new accounting owner is needed. Cover a real compound Variant whose retained capacity exceeds length and prove the staged weight includes capacities; retain existing projection/retry and byte-limit behavior. This needs independent source validation before inclusion in the final ledger.

## Performance evidence and verdict rationale

The original review did **not** run fresh benchmarks. Its `verification.md` explicitly says L1 benchmarks were not rerun. The source records inspected here are also historical: all six records identify clean `f50fdd9fb`, not `c9794a0b0` or `cc0d9c86c`. `git diff --stat f50fdd9fb c9794a0b0` shows only the task-evidence document changed, so runtime behavior reasonably carries across that first interval. Closure at cc0d adds an unreachable-branch refusal and rustdoc changes; this does not turn old INVALID records into current acceptance proof.

The two verification normal records contain no full workload measurement after their failed smoke prepass. Smoke p99 around 0.74/0.77 ms is diagnostic evidence at reduced timing; it cannot prove the normal three-repetition median criterion would fail, much less that every compliant improvement is impossible. The driver CPU arithmetic supports the throughput/driver-cost explanation, but cannot establish scheduling as the exclusive tail cause or that changing budget is the only repair. Gateway/Bifrost records retain failures, including criteria inside overall INVALID records; these still prevent acceptance, and historical failure is not an exemption because the approved revision explicitly requires all five experiments to pass.

REQ-021 keeps limits unchanged and explicitly includes Gateway capture and Bifrost point-lookup optimizations. The task's material stop condition applies **if correctness requires** a different boundary/criterion, not whenever a performance measurement fails. No original report establishes such a necessity. Driver efficiency changes that preserve workload, timing/accounting, limits and validity semantics do not inherently revise a specification, although an invalid private task prohibition may need correction through planning. Increasing a declared budget or relaxing an SLO would need appropriate approval; this review does not recommend either.

Proposed disposition: retain P5 as an unmet acceptance obligation. Reject the categorical `SPEC_REVISION_REQUIRED` rationale unless a concrete necessary conflict with approved behavior is independently demonstrated. Continue bounded diagnostics and approved optimization/remediation. Require fresh clean, normal full L1 evidence for the final runtime candidate before acceptance; current review has neither run nor supplied it.

## Disposition of every other original observation

| Original observation | Source/materiality assessment | Proposed status |
|---|---|---|
| Behavior O-1 / invariant O-2: mixed-part fallback | Producers put staged rows only into audit/results/input/output destinations; capture/spans are distinct tables. `Frame::shares` requires same table and schema. No supported mixed-frame scenario shown. cc0d refuses it and preserves homogeneous batch concat | Nonblocking cleanup, closed; no remediation needed |
| Behavior O-2: local split copies | `Chunk::encode` retains full rows only for whole frame; split uses `take_record_batch`, keeps original IPC wire bytes/identity, then discards IPC locally | Consistent with compact ownership charge; reject as defect |
| Behavior O-3: design commit after Step 1 | Chronological order did not meet literal “before executable changes”; final owners are present and Step 1 preserves behavior | Historical process observation, no current implementation correction |
| Behavior O-4: staging weights change frame geometry | `Chunk::split` still enforces actual encoded ceiling; estimate changes do not establish wrong frames | Valid diagnostic motivation within B-1; no separate defect |
| Invariant O-3: audit-only real-WAL shape coverage | Test extends audit shape; changed admission owner is table-generic, other fixtures/public lanes are cited | Coverage limit, no demonstrated material defect; no extra broad suite prescribed |
| Persistence O-1: source-batch vs frozen schema metadata | `interleave_record_batch` takes first source schema; candidate spans can differ in extra Arrow metadata, but fingerprints and conformed field semantics remain equal; report establishes no supported observable consequence | Reject proposed parity rebuild as optional; no correction |
| Persistence O-2: admission test would pass if lease immediately dropped | Actual `_working` lease spans allocation work in `prepare_sorted_candidate`; compiler/source lifetime is credible evidence | No defect and no test extension required |
| Security SEC-OBS-1: ServerSpans canonical | Existing server-owned validated projection; owner text allows server-projected rows; no external trust flag introduced | Consistent; no correction |
| Security SEC-OBS-2: peer Rows panic | Route fixed on sink; encoder produces Rows only for local route and peer receiver creates IPC; panic documented | Invariant guard, no reachable supported-use failure shown |

Original F-3's rustdoc gate remains mandatory despite being called minor; the closure supplies the missing sections. Original F-2/FIND-006R1B-4's bounded documentation corrections are now present. Original FIND-006R1B-2's cleanup was not a sound material blocker merely because it removed unneeded lines.

## Overall scoped result

**FAIL** for closure of the reviewed claims: required diagnostic evidence remains missing and the original capacity observation warrants a material accounting correction, subject to independent validation. P5 acceptance remains unproven. The original specification-revision conclusion is unsupported by the failures alone.

Skipped: comprehensive implementation rediscovery and all runtime execution. Risk: historical INVALID measurements and green correctness tests cannot establish final performance acceptance.
