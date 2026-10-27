# Independent findings validation

**Scoped recommendation: FIX_REQUIRED.** Two material findings remain: `FIND-006R1B-1` and `FIND-006R1B-5`. Required P5 acceptance is also outstanding. The original categorical `SPEC_REVISION_REQUIRED` rationale is not established by its evidence.

Subject: original `55f87cef1d74886a73cedab8d5c0960e915717a9...c9794a0b0c2182e93dcb8d936d253cb465620faa`; closure `c9794a0b0..cc0d9c86c049ee93cd3ee3f9012fbdd85c5b1b0a`. HEAD was the latter candidate during this review. This validates the original findings, challenged dismissals, corrections and verdict reasoning, not complete implementation acceptance.

Method: independent source/diff and record inspection, with CodeGraph before source navigation. Read the task-review skill, workflow, maintainer guide, approved revision-14 spec, final batch-once task, original reports/verification/verdict, current scoped discovery, applicable Bifrost/security owner paragraphs, and installed parquet-variant 60.0.0 builder sources. No builds, tests, benchmark runs or production-source edits. Original test results are attributed evidence only. No blocker originated solely with this validator; B-2 came from scoped behavior discovery revisiting original invariant O-1.

## Final ledger

| Claim | Decision at original/current subject | Correction and proof |
|---|---|---|
| `FIND-006R1B-1` / behavior F-1 / new B-1, required intervention diagnostics | **CONFIRMED, OPEN — MISSING** | Supply the bounded per-intervention allocation/copy/geometry and separate local/peer retained-memory evidence explicitly required by Steps 1–5 and P5. Correct incomplete GREEN claims while proof is pending. Merely relabeling them incomplete does not close the obligation. |
| `FIND-006R1B-2`, mixed-frame fallback as material DRIFT | **REJECTED as blocker; cleanup CLOSED** | Original behavior/invariant reviewers correctly established no supported mixed destination. cc0d replaces the unreachable single-row projection arm with an error and corrects its docs. No further work. |
| `FIND-006R1B-3` / behavior F-3, mandatory panic docs | **CONFIRMED historically; CLOSED statically** | cc0d adds `# Panics` to `StagedRow::column` and the results test projection helper, naming the indexing/failed-projection conditions. Mandatory AGENTS gate applies even though runtime behavior is unchanged. |
| `FIND-006R1B-4` / behavior F-2 plus evidence inaccuracies | **CONFIRMED historically; CLOSED statically** | Closure adds the missing audit-test replacement, describes the actual `<256 B` assertion, corrects Step 1 to `b47082554`, and removes the extra `tests::` component from the P3 selector. Producer projection uses the shared staged builder. |
| Invariant O-1 / new B-2, Variant allocated capacity omitted | **CONFIRMED, OPEN — INCORRECT; assign `FIND-006R1B-5`** | Account retained capacities at the existing `EncodedVariant` owner and staged cell memory boundary, preserving encoded-length semantics for field/frame budgets. Focused proof below. |
| Behavior F-4, required P5 not demonstrated | **CONFIRMED acceptance gap** | Historical INVALID records cannot qualify the candidate. Keep P5 incomplete and use its existing full normal commands for final acceptance. No new finding ID for the already recorded failed acceptance requirement. |
| F-4 correction/verdict inference that no approved fix exists | **REJECTED** | No source-backed necessity to change an approved boundary or criterion was demonstrated. Failures do not establish impossibility of compliant optimization. Do not prescribe blanket budget/criteria changes. |

## FIND-006R1B-1: missing diagnostic evidence

Authority: task Step 1 GREEN requires profile/allocation proof of eliminated discard copies; Step 2 GREEN requires actual allocations/copying and encoded geometry; Step 3 GREEN separately checks local and peer retained memory; Step 4 GREEN requires reduced copies/allocations without increased downstream merge work; Step 5 requires separate intervention evidence. P5 and the acceptance map at task lines 1452–1471 require source/configuration, geometry and CPU/work operands. AC-014 also requires the diagnostic run locating each cost.

The implementation evidence at lines 1502–1510 supplies correctness selectors and integrated historical timing. It contains none of the distinct required intervention measurements. Closure lines 1564–1571 acknowledge this but say the c64 prepass requires a maintainer decision first. That dependence is not established: `capacity/levels.rs:221–231` accepts declared selections; `:249–263` forwards `--steps` into the prepass and strips profiling/qualification flags; verification declares both c1 and c64 steps. P5 expressly permits partial/profiled runs as diagnostics. Thus `mise run bench:l1:verification -- --steps direct-verify-c1 --profile` is an existing bounded diagnostic route, subject to unchanged validity checks, not a promise it will pass or prove every operand. The separate in-process route fixture can supply retained-memory observations without public c64 throughput acceptance. A CPU profile alone cannot substitute for allocation or retained-memory measurements.

Smallest complete correction: append comparable source/configuration-identified observations around the existing intervention commits (`b47082554`, `c5d0ed5ad`, `f50fdd9fb`, with predecessor/control identities). Measure only missing operands: S1 eliminated owned validation copies; S2 producer allocations/copied bytes and rows/frame; S3 local versus peer retained bytes; S4 payload copies and downstream merge work. Reuse current profiling and fixtures, adding temporary bounded probes only where needed and removing them afterward. No permanent metrics, new harness, full L1 per intervention, budget increase or new product decision is needed. This is an explicit proof requirement, so labeling it incomplete is honest interim reporting rather than acceptance.

## FIND-006R1B-5: retained Variant capacity is undercharged

Authority: REQ-021 says staged rows are admitted by memory they own; task Step 2 requires retained owned values charged until settlement/loss. The earlier approved task accounting rule at line 780 explicitly charges retained allocated capacity while queued/waiting/retrying/in-flight. The final representation change preserves pressure behavior and does not waive this rule.

Source trace:

1. `wyrd-queue/src/variant.rs:218–234` directly retains `VariantBuilder::try_finish()` buffers; JSON construction at `:260–274` does likewise. The builder is installed parquet-variant **60.0.0**, matching Cargo.lock.
2. Installed `builder.rs:88,130–145` wraps `Vec<u8>`, appends with `push`/`extend_from_slice`, and returns the underlying Vec. `try_finish` at `:976–984` returns these buffers without shrink. Object finish (`builder/object.rs:261–328`) and list finish (`builder/list.rs:154–192`) splice headers into that same grown value buffer. Metadata finish (`builder/metadata.rs:210–255`) reserves and returns its buffer. Finish does not convert to an exact-size boxed slice. Ordinary compound values therefore retain spare allocation; a universal twofold bound is neither necessary nor proved here.
3. `tables/staged.rs:61–65` calls `EncodedVariant::encoded_bytes`, whose `variant.rs:388–389` adds **lengths**. `StagedRow::owned_bytes` at `:132–134` adds cells' inline storage but cannot recover either Vec's spare capacity. `StagedRow::new` moves the cells into an Arc; later row clones share them, so they do not shrink these Vecs.
4. Verification `results.rs:540–545` constructs Variant cells through this typed encoder for inputs and outputs. `ResultRow::owned_bytes:127–128` delegates to staged rows. `ScribeSink::weight:1549–1557` uses it for results, inputs and audit. `Part::bytes:259–264` also uses the same row estimate for frame construction.
5. `wyrd-runtime/src/outbox.rs:320–348` computes weight and admits bytes before moving the item through `enqueue:460–468`; there is no intervening clone/shrink or additional Variant-capacity charge. Retained bytes remain attached to the queued item through its existing settlement/loss lifecycle.

Reachable consequence: repeated ordinary compound input/output Variants under backlog consume more live payload heap than the fixed outbox byte budget charges. Sibling capture/span length estimates do not discharge this explicit staging obligation. This is material admission-accounting failure, not optional allocator precision.

Smallest correction: provide capacity accounting from the existing EncodedVariant owner and use it for staged-cell owned memory. Keep `encoded_bytes()` returning lengths: log size limits, metrics/trace payload estimates and Variant column sizing use actual encoding lengths. Changing that accessor globally would change unrelated size semantics. No shrink/copy, allocation registry, per-producer guard, shared outbox redesign or new configuration is justified. The common owner fixes audit and all verification cells together, and updates frame estimates through the existing delegation.

Focused proof must use an actual typed/JSON compound Variant with capacity greater than encoded length and independently assert capacities, not simply compare weight to the same faulty estimator. Extend existing local tests where practical: owner capacity versus private buffers; staged row charge includes that capacity; server weight retains it for normal staged inputs/results/audit. Preserve encoded-length limits, projection/retry behavior and unchanged admission limits. The current server test only checks weight equals row estimates plus a 64-KiB payload lower bound; it cannot detect spare capacity omission.

Exact existing focused selectors suitable for that correction (not run here):

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::typed_values_encode_as_their_json_text)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::staged::tests::staged_rows_project_into_one_dense_batch)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=scribe_outbox::tests::staged_writes_charge_their_owned_memory)'
```

These are existing extension locations, not a requirement to couple new assertions to unrelated behavior. Any separately named test must carry its own exact selector. No broad suite is required solely for this small shared accounting correction.

## Challenged observations and closure paths

| Original observation | Independent decision |
|---|---|
| Behavior O-1 / invariant O-2, mixed fallback | `frames` distinguishes captures, other writers project their own tables, and `Frame::shares:1067` requires equal table/schema. Staged audit/verification destinations never overlap batch calls/spans. Original blocker was not material. Homogeneous rows still use one projection; homogeneous batches keep concat in cc0d. |
| Behavior O-2, local split copies | `Chunk::encode:1230–1256` derives all frame identity from IPC first; whole frames share rows, split frames use compact `take_record_batch`, then replace IPC with rows carrying wire length. Copy supports existing ownership boundaries; no defect shown. |
| Behavior O-3, design commit chronology | Step 1 predates design commit; original literal process ordering was missed. Final owner text and behavior exist; historical order cannot be repaired by implementation churn. No current blocker. |
| Behavior O-4, changed geometry | Estimate changes can alter rows/frame, but `Chunk::split` still applies actual encoded budget. Needed measurements belong to FIND-006R1B-1; no separate geometry failure shown. |
| Invariant O-3, audit-only WAL fixture | A coverage limitation, not a source-proven failure; do not prescribe a new broad suite. Prior real-server results remain attributed evidence. |
| Persistence O-1, schema metadata | Gather uses first candidate source schema rather than frozen schema. No supported logical/type/fingerprint consequence was established; optional parity rebuild remains rejected. |
| Persistence O-2, admission test cannot detect early drop | `_working` in `prepare_sorted_candidate:437` is a named lease held across key/index/output construction. Source establishes lifetime; no new test solely to mirror correct binding. |
| Security SEC-OBS-1, canonical ServerSpans | Existing server projection is the relevant trust owner; canonical eligibility is not restricted to staged cells. No new external trust flag was introduced. |
| Security SEC-OBS-2, peer Rows panic | `ScribePeers::submit:760–763` guards a fixed-route invariant; `Chunk::encode` emits Rows only for in-process delivery, and peer receive constructs IPC. No supported reachable failure shown; documented guard is sufficient. |

No additional unresolved material conflict needs follow-up. The changed code is only the mixed fallback and two doc additions; affected homogeneous row/batch projection, frame grouping and route/identity paths remain source-consistent. This statement is scoped closure validation, not fresh trust/persistence certification.

## Benchmark provenance and verdict sufficiency

The original review **ran no benchmarks**. Its `verification.md` explicitly says L1 was not rerun; focused correctness reruns and formatting were reported at c9794a0b0. This review also ran none.

All six cited JSON records are clean `f50fdd9fb5c1ba1c31defd8134d0e7c75294eff8`, across five experiments: verification `215213Z` and `215339Z`, Gateway `215452Z`, Bifrost writes `220142Z`, hot `220746Z`, parquet `221256Z`. All report INVALID. `git diff --name-only f50fdd9fb c9794a0b0` contains only the task Markdown, so runtime source matches the original reviewed candidate. cc0d adds only the unreachable mixed-frame error and panic docs to runtime files; historical records still are not fresh acceptance evidence at cc0d.

Both verification normal records have prepass exit 2 and no measured full repetition workload. Their smoke p99 cannot establish a normal three-repetition median failure, nor that compliant optimization is impossible. Gateway/Bifrost records have measured criterion failures inside overall INVALID records; their historical preexistence does not waive REQ-021. CPU arithmetic may motivate driver-cost investigation; it does not prove scheduling as the exclusive tail cause or a budget change as the sole repair.

REQ-021 explicitly optimizes all these engines until unchanged limits pass. The final task stop condition requires a necessary change to correctness/boundary/criteria before specification escalation. No such necessity is proved. A driver efficiency change preserving workload, accounting, validity and limits is not inherently a specification change; a conflicting private task restriction can be corrected through planning. A budget increase or criterion relaxation would require approval, but neither is prescribed here.

The original reports adequately disclose selected test results and P5 failure. Their final ledger omits the original F-4 acceptance gap as a ledger row, dismisses capacity against the explicit obligation, and offers relabeling as an alternative diagnostic correction. Their verdict turns missing/invalid proof into an unsupported impossibility claim. These defects prevent relying on that verdict as written.

Bounded remediation should close the two validated findings and accurately retain P5 as outstanding. Final acceptance still needs clean normal full P5 records under the unchanged five existing commands; this report authorizes no benchmark execution or automatic source tuning. No specification revision is justified solely by current failures.

Skipped: comprehensive implementation rediscovery and all runtime execution. Risk: existing green correctness tests and historical INVALID records cannot establish final memory or performance acceptance.
