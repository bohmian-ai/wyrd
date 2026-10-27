# TASK-004 second remediation discovery

Result: **FAIL**. Two proposed findings, independently validated by the next reviewer before inclusion in the verdict. Source was not changed.

## Subject, authority and scope

Original base `0b500d677531ba501924b1c6d412fe14ff9aa5cd`; prior candidate `c9d72729b85187f2399725e0ed9496c4c4a39298`; candidate `0bf0cb8662a221233a963ed147d57ef0cbb6720f`. Inspected the actual remediation diff, cumulative context for the prior findings, first-parent merge `9a540b7b8` and its conflict decisions, and changes after that merge. Governing authority is the approved revision-8 contract plus the maintainer-directed revision-9 correction in working-tree `spec.md`, SHA-256 `007d27e3a0e22a4aa912108da6a7478849a3f2f56c953fdea9e1491c65e96dce`. Its stale draft header does not block this review.

Read TASK-004, R1/R2 verdicts and validated findings, R2 remediation and evidence, root instructions, task-review skill, workflow, maintainer/Rust guides, TESTING, and Bifrost measurement meanings. CodeGraph preceded code discovery. Scope is residual F1/F2/F3/F5, changed benchmark measurement/driver paths and real-smoke lifecycle, validity, evidence, judgment and affected tests/docs. Receiving authentication/classification is reopened but belongs to the selected security reviewer; orchestration ownership additionally belongs to the selected maintainer reviewer. No agents were spawned by this reviewer.

Current TASK-005 explicitly owns the complete L1 matrix, including writes-hot/writes-parquet. The R2 phrase requiring four smoke experiments conflicts with that dependency partition while TASK-004 currently has three. This report checks every currently declared plan and does not require implementing TASK-005 early. Resolve the task wording in its owner; the final complete-matrix proof remains due when TASK-005 implements it.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1: stop-boundary byte counters separate from final drain; CPU and completed request/row/wire work freeze at admission stop | `capacity.rs:371-432` starts a separate stop scrape beside drain, returns it separately from final selected telemetry; isolated/competing retained MiB and Gateway Durable consume that scrape; final integrity retains post-drain scrape | Exact freeze regression and three actual-stop admission regressions in current `verification.md`; source tracing of their consumers | PASS under R2's practical stop-scrape contract |
| F1: late wake never launches new open-loop work after stop | query competing open_loop, Verification arrivals and Gateway Driver check actual clock after wake | Three exact late-wake selectors, current verification 8/8 | PASS |
| F2: raw OTLP spans/logs constructor carries scoped metadata and bearer | `query/run.rs:683-698` `authorized` uses existing benchmark_headers; both actual Sender export paths call it | `query::run::tests::otlp_exports_carry_their_scoped_labels`; actual historical smoke timing artifacts; receiver changes covered separately | PASS for producer; receiving boundary pending security report |
| F3: selected Bifrost L2 maximum shares cohort/rank/bounds with selected percentiles | `query/competing.rs:638-655` adds q=1 `max_ms` through server_percentile | `cohorts_follow_admission_not_completion` checks different case/phase cannot supply max, rank/samples 2/2 | PASS |
| F5: Pending tuple field documented | Intermediate fix documented it; final candidate deletes Pending entirely with activation change | Actual final timing.rs diff; current fmt/clippy green | PASS; changed behavior is reviewed at security boundary |
| Real smoke uses shared normal declaration/prepare/step/warmup/repetition/drain/evidence/report path | Mode, Level smoke timing, levels smoke/prepass, engine normal prepare implementations; handwritten Smoke owners removed | Existing workflow test plus eight clean historical default-plan records, including all three L2 plans | PASS for structural workflow; final integrated release rerun still required |
| Smoke exemptions limited to below-floor percentiles and incompatible references; correctness and other applicable criteria still judge | `experiment.rs:2534-2539` instead exempts every remaining criterion | Existing workflow test explicitly expects a sufficiently sampled 18x miss to PASS; real records waive refusal/shedding criteria | FAIL: R3-B01 |
| Positive CPU/work, nonempty exact timing populations and normal validity remain | repetition_reasons positive CPU; Population validate; isolated positive insertion/wire checks, Gateway capture/write check, Verification component check | Current 46-test lane and exact 8 selectors; missing p99 and zero CPU workflow assertions | PASS for traced validity; this does not excuse criterion-based integrity failures under R3-B01 |
| Smoke records retain mode, remain outside history and cannot calibrate after relocation | Record publish dispatches on mode; serde defaults older records to full; calibrate explicitly requires `record.mode == Mode::Full` independently of path | Mode/publish/read/report/persist-error workflow assertions; source proves relocation cannot defeat mode filter | PASS for behavior; dedicated relocated-record assertion is still part of requested proof |
| Failed prepass refuses before full preparation; clock and isolated state are separate | levels prepass uses existing Postgres child launcher; Declaration prepass refusal; main invokes prepass before execute | Workflow test supplies failed/passed Prepass and checks preparation refusal; historical full-invocation prepass record exists | PASS for implementation, with proof limit that unit test supplies exit status rather than spawning child |
| Controlled delay, CPU and instrumentation calibration uses normal measurement path and independent references | capacity calibration reads production recorder/Population and PID CPU directly; does not drive Window/shared runner or compare observation overhead | New calibration test passes, but only asserts service elapsed/residual and direct CPU reader | FAIL: R3-B02 |
| Existing HDR, complete artifacts, versioned report/persistence, quantization and legacy behavior retained | Shared Population/read/report owners remain; no new collector/schema/report framework | Carry R2 accepted rows and current experiment/capacity tests; current static checks green | PASS, except source-backed changes described above |
| PR215 benchmark merge seams retain owners and renamed identities | First-parent merge preserves timing rewrite; fixtures/queries use bifrost catalog, object segment strips only catalog; BifrostTelemetryDelta resources uses retained fields | Existing resources projection and parquet path regression; current focused checks; imported immutable review remains separate | PASS for inspected benchmark seams; security report owns receiving auth changes |

## Proposed R3-B01 — Smoke bypasses correctness, loss and sufficiently sampled targets

Classification: **INCORRECT / VIOLATION**. New smoke correction failure, not a relabelled old F1-F6 finding.

Obligation: revision-9 REQ-008 explicitly exempts below-floor percentiles and incompatible full-run references, then requires all other applicable criteria and every integrity check to remain enforced with unchanged PASS/FAIL/INVALID rules. R2 says wrong results and capture/counter loss cannot pass. `mise.toml` bench:smoke description repeats that contract.

Location: `crates/wyrd/wyrd-testing/src/experiment.rs:2534-2539`, unconditional `Mode::Smoke` return after presence/sample-floor checks. Changed rustdoc at 2435 and workflow test at 4964/5034 encode the same wrong contract.

Producer-to-consumer trace: Verification `run.rs` records `wrong` and unexpected `errors`; its declaration `verification/mod.rs:388-389` supplies no-errors and correct-verdicts limits. Bifrost supplies exact-answers/no-errors limits (`query/mod.rs:807`, `844`, `1316-1317`). Gateway Observed::repetition (`gateway/run.rs:324,389-407`) exposes capture loss/shedding/drop/remaining-work counts and its declaration (`gateway/mod.rs:310-321`) supplies corresponding limits. Shared repetition_reasons does not independently reject those nonzero scalars. judge_criterion returns `applicable=false`, `verdict=None` before Rule::Limit runs; the step and overall record can PASS.

This happens in actual clean release evidence at:

`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-experiments/b20fe86c-f621-4d14-bf8c-ff976ee65dc5/scratchpad/clean/target/benchmarks/smoke/`

- `gateway/overhead/20261025T215225Z-2f43ca9bd.json`: overall PASS; capture-shed=3315 at chat-stream.c64, 26199 at chat-capture.c16 and 20631 at chat-capture.c64; each criterion is inapplicable with no verdict.
- `verification/direct-verify/20261025T215137Z-2f43ca9bd.json`: overall PASS while direct-verify-c64 no-errors observed 23690 and is inapplicable. Its automatic prepass sibling `20261025T215952Z-2f43ca9bd.json` similarly waives 21459 errors.

These artifacts predate the merge, but the exact unconditional judgment branch survives in the reviewed candidate. The current test also proves the branch deliberately waives a 2000-sample p99 of 9 ms against its 0.5 ms limit. Thus the issue is material both to safety/correctness evidence and to fail-fast prepass authorization.

Smallest correction: remove the unconditional remaining-smoke exemption in the existing judge_criterion owner. Preserve the already implemented reference and nonempty below-floor exemptions, required-reading validation, full behavior, all measured controls, production permissions and capture lifecycle. Do not create an integrity-policy taxonomy, duplicate workload checks downstream, or weaken production limits to make a smoke green. Correct the changed rustdoc and test expectation together.

Focused closure: extend the existing selected-workflow test with a present nonzero correctness/error or loss scalar; it must FAIL and retain the measured value. A sufficiently sampled target miss must FAIL; a genuinely below-floor percentile alone stays inapplicable; missing population/zero CPU stay INVALID. Keep invalid precedence. Feed a failed prepass through the existing refusal seam and assert full preparation never runs. Run the exact existing smoke selector and the narrow experiment lane, then real release smoke on the corrected final source; discovered production failures must remain visible.

## Proposed R3-B02 — Claimed instrumentation calibration never measures instrumentation overhead or the complete measurement path

Classification: **MISSING**. Bounded explicit proof obligation, separate from future TASK-005 comprehensive qualification.

Obligation: R2 maintainer direction and AC-012 explicitly require controlled known-delay, known-CPU and instrumentation-overhead calibration through the same measurement path with declared independent references and justified tolerances, without a side runner.

Location: `crates/wyrd/wyrd-testing/src/capacity.rs:1085-1193`, sole new `known_delay_overhead_and_cpu_calibrate_the_measurement_path` test and its evidence claim. The test times sleeps, calls record_timing directly, takes in-process snapshots and constructs Population. It independently reads `process_cpu_usec` around a 300 ms loop. The observed `overhead_p50_ms` is the request residual after subtracting a simulated wait; it is not timing instrumentation's observer overhead. There is no comparable workload without instrumentation, no independent overhead reference/bound, no Window::open/measure, no stop/drain resource conversion and no normal shared experiment runner in this calibration. Both elapsed/residual and direct PID tests may pass even if resource-window/measurement wiring or observation overhead is wrong.

Materiality: the user explicitly requested this calibration to establish that the measurements can be trusted. Passing isolated arithmetic/reader tests and a workflow run cannot substitute for the named independent calibration. This is an incomplete requested proof, not a claim that measured overhead is excessive or a demand for TASK-005 full-suite profiling.

Smallest correction boundary: extend the existing controlled workload/normal-runner calibration proof to drive the real measurement window and retained timing readout; retain the current local elapsed/residual tests as useful unit checks. Add one identical controlled workload with collection enabled and disabled through that same existing path and compare against independently measured wall/CPU references with a justified nonzero tolerance. Record the observed instrumentation cost and its bound. Reuse current workload, timer/readout, resource and record owners; no new benchmark framework, collector, dependency, synthetic contention, general calibration matrix or full qualification suite is warranted. The implementer must choose references/tolerance before observing the result rather than declaring literal zero overhead.

Focused closure: one bounded calibration case must fail when resource-window wiring or instrumentation cost is deliberately perturbed beyond its declared tolerance, while known delay/CPU references retain their independent meaning. Record exact selectors and results. Reuse the current calibration selector if extended, plus the existing shared workflow selector; real smoke rerun after fixing R3-B01 supplies final-source integration evidence, not this independent calibration by itself.

## Carried closure, calibration and verification limits

F1 now uses a practical independent stop scrape concurrent with drain. An HTTP scrape is asynchronous and cannot atomically freeze a remote process; R2 did not authorize synchronized clocks or a new atomic distributed collector. I do not retain unavoidable scrape skew or a test-only join-poll ordering concern as a new blocker. The previous complete-drain scrape was replaced, and all four affected byte consumers use the distinct stop result.

F2 uses the actual common constructor, F3 uses the selected cohort, and F5's final offending field no longer exists. F6 checked conversion and unchanged HDR/local snapshot/legacy guarantees carry from R2. F4 is deliberately reopened by classification/auth changes and requires the selected domain report; this discovery report does not preempt it.

The orchestrator's current verification.md records the sequential Postgres-backed 46-test affected lane, 8 exact selectors, fmt, clippy and diff checks, all green. This reviewer independently read those tests and source but did not run concurrent Cargo commands. The smoke test's green expectation conflicts with authority under R3-B01.

Actual clean 2f43 smoke records and timing artifacts exist and show the real workflow executed; their eight default-plan successes are historical, not final-candidate proof. The checkout-local five earlier records all refuse dirty source and admit no work. No release smoke was rerun by this reviewer. A clean final integrated rerun remains the explicit V8/R2 proof after corrections, without --allow-dirty. Do not create a separate finding merely for a rerun already needed by the retained correction.

Convergence: prior residual F1/F2/F3/F5 have bounded closures. Current failure is principally a fix-induced weakening of the maintainer-directed smoke contract, already observable in real records; the second gap is incomplete explicit calibration proof. The next review should cover those two correction boundaries, their changed tests/docs, final-source smoke and any validated security/maintainer findings, carrying accepted old evidence rather than restarting the task. Full-size qualification and the complete four-experiment split stay with TASK-005/integrated review.

Skipped: no full suites, broad gate, platform execution or separate PR215 schema audit. Risk: current smoke PASS can admit a full run despite substantial real refusal or capture shedding.
