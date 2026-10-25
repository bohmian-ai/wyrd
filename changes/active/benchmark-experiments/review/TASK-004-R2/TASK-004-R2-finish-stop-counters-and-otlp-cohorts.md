---
id: TASK-004-R2
kind: remediation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 9
requirements: [REQ-006, REQ-007, REQ-008, REQ-013, REQ-015, INV-004, INV-005, AC-004, AC-005, AC-012]
depends_on: [TASK-004-R1]
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-5]
---

# Finish measured counters, OTLP cohorts and real benchmark smoke

## Immutable acceptance context

Route this task to `$wyrd-implement`. It contains the bounded remainder of the
prior remediation plus the maintainer-directed replacement of the request-only
smoke. The four residual findings remain grounded in approved revision 8; the
added smoke behavior is specified in draft revision 9. Its concrete revision must
be approved before implementing that changed contract. Revision 8's existing
approval is not in question.

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Approved spec: `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`; explicit maintainer approval persists from the preceding sessions.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md`.
- Original base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`.
- Prior reviewed candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
- Current reviewed candidate / implementation base: `c9d72729b85187f2399725e0ed9496c4c4a39298`.
- Prior remediation: `../TASK-004-R1/TASK-004-R1-complete-cohort-timing-evidence.md`.
- Current review authority: this directory's `verdict.md`, `findings-validation.md`, `task-review-invariants.md`, `domain-review-security.md`, `followup-review.md` and `verification.md`.

Preserve original tasks, candidate identities and review evidence. Do not rewrite past completion claims or prior verdicts to imply earlier closure. This review accepts the corrected CPU freeze, measured/late request-row-wire accounting, admitted cohort and scan selection, full snapshot artifact persistence, ranks/shared maximum conversion, verified authentication activation, original rustdoc corrections and checked conversion. Keep those fixes and their proof. The following residual findings and separate maintainer direction define the remaining scope.

## FIND-TASK-004-1 — Stop server work counters and actual admissions with CPU

### Diagnosis

REQ-006/007 require all work numerators to use the same measurement interval as CPU/T, with drain separate. Current `Window::measure` in `crates/wyrd/wyrd-testing/src/capacity.rs:371–404` awaits `until_stop`, which freezes CPU and then waits for request drain at `:423–425`. Only afterward does it take its closing metrics scrape. Its returned `Metrics` and `Usage.telemetry` therefore include work performed during drain even though CPU and elapsed time correctly stopped earlier.

Bifrost L1 `query/isolated.rs:190–192,238–241` and L2 `query/competing.rs:340–344,380–381` use that post-drain inserted-byte delta as `retained_mib`. Required MiB/T/P and MiB/C consumers at `query/mod.rs:588–600,1229–1230` divide it by measured T/C. Gateway L1 `gateway/run.rs:581–605` and L2 `gateway/capacity.rs:1014–1016,1084` similarly derive `Durable::between(baseline, ended)` and capture bytes from the post-request-drain scrape; `gateway/mod.rs:472–490` normalizes them by measured T/C. A final write or stream staging durable bytes during drain can therefore improve measured byte efficiency without its matching CPU. Rows/wire/request late accounting is already corrected and must remain so.

The same original stop obligation also remains incomplete in Bifrost `query/competing.rs:174–190`, Verification `verification/run.rs:907–937` and Gateway `gateway/capacity.rs:360–379`: scheduled due offsets are checked before sleeping, but the actual clock is not checked against end after waking and before admitting. An ordinary final timer waking just after stop can still send new work after CPU froze; Gateway explicitly allows small lateness within its existing threshold. This is a required boundary, not permission to change pacing or rate policy.

### Selected correction

Complete the existing shared Window measurement handoff so the practical stop-boundary server work counters are captured separately from final drained/cohort/correctness counters. Route retained Arrow and durable capture byte work and their existing normalizations to the measured boundary. Preserve their existing metric meanings, including actual inserted Arrow memory; do not estimate retained bytes from payloads/rows, silently change which real server work the metric represents, or count final drain bytes against frozen CPU. Keep later integrity, settlement and final cohort scrapes for their legitimate checks.

Enforce actual stop at the existing open-loop admission points after waiting and before sending. Preserve scheduled/offered/missed-arrival accounting, existing lateness policy and bounded draining of already admitted tasks. Keep the corrected CPU/client work behavior. Reuse Window/ResourceWindow, current metrics reads and existing engine task/schedule owners; no synchronized clock service, new collector, cancellation policy or driver redesign is needed. A denominator adjustment or downstream zero substitution cannot recover an incorrectly selected numerator.

### Focused proof

At an actual measurement-to-Repetition seam, hold final write/capture work until CPU has frozen. Assert that measured retained/capture bytes and both required byte normalizations exclude that work, while final integrity/cohort/late accounting retains it once. Check sibling L1/L2 conversions through the same handoff. A controlled last scheduled wake after end must admit nothing, preserve missed/offered accounting, and still drain earlier admissions. The existing freeze callback and late-row tests do not prove these omitted consumers.

## FIND-TASK-004-2 — Copy scoped labels into raw OTLP requests

### Diagnosis

The finite context and final cohort selection are implemented, but raw OTLP producers do not read that context. `query/run.rs:198–205` constructs a plain tonic Channel. Both `Sender::send` export siblings at `:286–312` call generated `TraceServiceClient`/`LogsServiceClient` with `authorized(body, &token)`. The complete constructor at `:678–684` inserts only `x-wyrd-access-token`, without benchmark case/phase metadata. An outer `with_benchmark_labels` task-local does not automatically modify generated tonic requests.

Downstream smoke (`query/mod.rs:1397–1414`), isolated (`query/isolated.rs:207–214`) and L2 expected cohorts (`query/competing.rs:396–435`) now match boundary/outcome/case/phase exactly. Successful spans/logs exports record `none/none` instead of their expected op/phase, so ordinary mandatory Bifrost smoke cannot reconcile and measured OTLP cases become INVALID. The existing native Bifrost, HTTP and raw Gateway metadata consumers do not intercept these sibling raw clients; helper-only header tests miss this path.

### Selected correction and proof

Copy the current scoped closed metadata from the existing `benchmark_headers()` mechanism into tonic metadata at the existing shared `authorized` constructor for both export types. Preserve bearer construction, credentials and errors. Do not add an interceptor, replacement transport, SDK API, label registry or downstream exception.

Exercise actual outgoing requests through that constructor for spans/logs and smoke/repetition/admission phase contexts. Assert both cohort metadata fields, unchanged bearer metadata and absence outside scope. Reuse the accepted receiving activation/cohort mechanism; a focused export-to-receiver assertion may establish exact reconciliation without full capacity runs. Another assertion of the unrelated header helper's output does not close the constructor path.

## FIND-TASK-004-3 — Record Bifrost L2's required selected maximum

REQ-007 separately requires a recorded count/p50/p95/p99/maximum and full distribution retention. Full snapshots, ranks, reconciliation and write-failure handling now work. However `query/competing.rs:590–620` bypasses the shared maximum-producing conversion and emits only p50/p95/p99 from its selected successful point/capacity cohort through `server_percentile` at `:557–569`. Reconstructable target buckets do not emit the required selected maximum reading.

Add q=1 as `max_ms` at the existing top-level conversion for exactly that same cohort, using the current `server_percentile`/`Reading::quantile` path. Preserve phase/case selection, rank/value-bound semantics and other quantiles. This is a local scalar completion; no new helper, schema/artifact owner, algorithm or dependency is warranted.

Extend the existing L2 conversion fixture with a slower successful observation. The emitted maximum must match q=1 of the selected cohort, including rank and bound, while another case/phase cannot affect it. Do not reopen accepted full-artifact architecture or require more unrequested scalar projections.

## FIND-TASK-004-5 — Document the new Pending tuple field

All originally enumerated documentation omissions are fixed. The remediation added `crates/shared/wyrd-telemetry/src/timing.rs:172`, `struct Pending(Labels);`, with enclosing type documentation but no field rustdoc. The root and Rust guide explicitly include private tuple fields and make missing rustdoc a merge gate; fresh independent `followup-review.md` confirms this narrow omission.

Add one meaningful field doc explaining the declared classification awaiting authenticated activation. Keep the wrapper, representation and activation behavior. No runtime defect is alleged; no new helper, wording test, documentation sweep or separate style round is warranted. Source inspection plus owning format/lint checks is sufficient closure.

## Maintainer direction — Smoke must execute the real benchmark

This scope was added by the maintainer after the re-review: a smoke test must
exercise the actual benchmark so a green result establishes that its whole
workflow works. It is not a fifth independently validated revision-8 finding,
and the immutable review verdict is not being rewritten.

Implement revision 9 REQ-008 and AC-012 in this remediation, using the existing
shared experiment owner. The normal L1/L2 commands gain `--smoke`, which reduces
declared fixture scale and windows to one short warmup and one repetition while
executing the same selected steps and all applicable capacity/overload/recovery
phases. Default smoke covers every declared step, all engines and all four Bifrost
L1 experiments. Fixture reduction must preserve the physical shape needed by
the workload; it cannot avoid publication, scans, capture or background work.

Exercise the real declaration validation, fixture loader and settlement, step
loop, warmup, measure, stop/freeze, drain, reconciliation, judging, record and
full timing-evidence persistence, and report generation. Remove handwritten
per-engine smoke implementations and the separate capacity smoke subcommand.
Keep `mise run bench:smoke` as the aggregate entry to the normal commands in
smoke mode. Move its useful positive-work/CPU, exact labelled timing, Scribe,
memtable/capture and scan checks into normal validity/correctness so full runs
enforce them too. Do not introduce another runner or a parallel measurement path.

Only below-floor percentile judgments and incompatible full-run reference
comparisons become explicitly inapplicable at smoke size, with counts/reasons
retained. Missing/zero measured populations, zero CPU, counter loss/reset,
reconciliation errors, wrong results and persistence failures remain enforced.
Smoke is functional evidence, never a performance qualification or reference.
New records declare full/smoke mode; older records retain their original meaning.
Use the same recorder and report generator, but put all smoke output under
`target/benchmarks/smoke/`. Reject smoke as a baseline even if moved elsewhere.

Full commands first execute an isolated/reset smoke-sized pass of the same
selected plan; a failure stops before full-size preparation. Its budget and
elapsed time are explicit and separate from the unchanged full experiment clock;
its data/counters cannot pollute full measurements. Smoke does not recursively
preflight itself. Preserve normal full-run sample floors, windows and three
repetitions. Record the extra prepass cost rather than hiding it in measurements.
Revision 9 supersedes TASK-005's instruction to include smoke in the full
experiment clock; it does not remove host checks or setup from the full clock.

Reword the conflicting execution instructions when reconciling the task packet:
TASK-005's "no waits for publication in the smoke" must require the normal
fixture/publication waits; its "smoke, fixtures ... remain inside it" must
separate the prepass clock from the full clock while keeping full-run fixtures,
host checks and drain inside the latter; and its Implementation Reference note
about `levels.rs` smoke must describe the shared normal runner in smoke mode.
Likewise replace TASK-004's future-facing request-only/`Workload::smoke` and
"Do not wait for publication" instructions. Preserve recorded historical
evidence and verdicts; they are not claims about the replacement workflow.

### Required proof

Run actual release-process smoke through the revised normal path for every
default L1/L2 engine plan, including all Bifrost L1 experiments, using repository
Postgres setup. Show the written mode, declared/finished steps and phases,
nonempty reconciled cohorts, real CPU/work/scan evidence, full timing artifacts
and regenerated report. Trace export remains optional. Record exact commands,
source identity and output paths; a handwritten request probe or a summary of
passing unit tests cannot substitute. Use `mise run bench:smoke` once its
aggregate has these semantics. Do not use `--allow-dirty` for final smoke proof.

Focused tests must drive the actual shared workflow, including selected-step
coverage, automatic prepass failure stopping full-size preparation, lower sample
floor applicability, missing evidence/counter and failed persistence rejection,
and smoke baseline rejection after relocation. Controlled delay, CPU and
instrumentation-overhead calibration must exercise that same measurement path
with independent declared references and justified tolerances. Do not assert
literal zero overhead or create synthetic host load to manufacture CPU readings.
Keep deterministic histogram boundary, drain and quantization checks. Full-size
qualification is still TASK-005/integrated review work; smoke cannot replace it.

## Constraints and accepted behavior

FIND-TASK-004-4 and FIND-TASK-004-6 are closed; preserve authenticated activation, refusal behavior, tenant/permission/audit owners and checked rank conversion. Preserve accepted portions of F1/F2/F3: CPU and request/row/wire late separation, final admitted cohorts, exact keyed counts, scan matching, snapshot retention/error invalidation and summary-only v1/v2 reporting. Keep normal tracing, existing bounded HDR/Prometheus/listener behavior, actual durable acknowledgement, retries, allocation/queue ownership, task drain and recorded metric units.

No new product/public SDK or endpoint, arbitrary/tenant/request labels, replacement clocks/collectors/transports/queues/report frameworks, baseline migration, new full-size qualification scope or broad old-code audit. Incidental/probe groups remain separate; no new individual reconciliation demand is introduced for them. Counter/metadata loss must remain INVALID rather than fabricated favorable evidence. The four residual findings require no semantic revision; the added real-path smoke contract requires revision 9. Its explicit supersession of old smoke instructions governs TASK-004/005 without altering historical evidence.

## Acceptance mapping and verification

| Finding | Acceptance / focused proof |
|---|---|
| F1 | Measured retained/capture byte numerators and CPU/T share the declared window, final drain remains separately observable, and the three actual-stop admission paths launch no new work after end; tests exercise the omitted consumer/admission seams. |
| F2 | Actual raw OTLP spans/logs constructor sends its scoped case/phase and existing credential; absent scope stays absent and finite receiving cohorts reconcile. |
| F3 | Bifrost L2 emits `max_ms` with correct rank/bound for the same selected cohort as its current percentile triplet. |
| F5 | The Pending tuple field itself has meaningful rustdoc, with no behavior change or sweep. |
| Maintainer direction | Actual reduced benchmark runs traverse the complete normal lifecycle and produce reproducible evidence/report; full prepasses fail fast; no smoke record can become a reference. Revision 9 REQ-008 / AC-012 govern, separately from the residual findings. |

Use existing owner tests and installed dependencies. Add/select focused regressions at the actual missing seams, record their exact names/selectors and expected RED cause, then GREEN and narrow owner regression results. Do not replace producer tests with manually labelled populations, hide failed checks or select zero tests. Any unexpected failure diagnosis follows `$wyrd-implement`; review proof is not permission to weaken assertions or timeouts.

Existing focused commands, plus exact selectors for newly added/extended regressions recorded by the implementer:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(/^capacity::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/)'"
mise exec -- cargo fmt -p wyrd-testing -p wyrd-telemetry -- --check
mise exec -- cargo clippy --locked -p wyrd-testing -p wyrd-telemetry --features wyrd-telemetry/test-support --all-targets -- -D warnings
mise exec -- git diff --check
```

Confirm any new actual test targets and use repository Postgres setup when required.
The real-path release smoke above is now required by the maintainer's added scope.
Full-size L1/L2 suites, full language journeys and broad aggregates remain outside
this remediation; final integrated verification retains its broader obligations.
The current green tests and old-candidate request-only smoke do not by themselves
prove these paths.

Record evidence and the next immutable candidate without changing historical verdicts. Commit/push only if asked. Next review checks these four residual closures plus the maintainer-directed real-path smoke and affected paths, carrying forward accepted work rather than restarting discovery.

## Implementation evidence

Candidate: `2f43ca9bd` (reviewed base `c9d72729b`). Commits: `6d19ad5cf` (F3, F5),
`e75d7f36f` (F2), `75a03119e` and `4c71562e9` (F1), `735292546`, `722f33d1b` and `1fd416d55`
(smoke through the normal workflow), `1ff82f11f` (TASK-004 wording), `9516f6c22`,
`c0e4b47d3`, `9f6ae848f`, `ea442a3c1`, `2f27ce132` and `d135f13e2` (defects found by the real
smoke), `d34112a2b`, `4006265e1` and its revert `2f43ca9bd` (smoke judging rule).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1: byte work and CPU share the measured stop; nothing is admitted after it | `capacity.rs` `until_stop` / `Window` stop snapshot; actual-stop admission in `query/competing.rs`, `verification/run.rs` and `gateway/capacity.rs` | `capacity::tests::the_measurement_freezes_at_the_stop_before_work_drains`, `query::competing::tests::arrivals_waking_after_the_end_are_missed`, `verification::run::tests::arrivals_waking_after_the_end_are_missed`, `gateway::capacity::tests::the_last_arrival_waking_after_the_end_is_missed` | PASS |
| F2: raw OTLP spans/logs carry the scoped case/phase and credential | `query/run.rs` OTLP constructor | `query::run::tests::otlp_exports_carry_their_scoped_labels` | PASS |
| F3: Bifrost L2 emits `max_ms` for the selected cohort | `query/competing.rs` `read_queries` | `query::competing::tests::cohorts_follow_admission_not_completion` asserts `max_ms` equals the selected cohort's quantile 1.0 with rank/samples 2/2 | PASS |
| F5: `Pending` tuple field has rustdoc | `wyrd-telemetry/src/timing.rs` `Pending` | review of the diff; clippy `-D warnings` | PASS |
| Smoke runs the normal L1/L2 workflow at reduced scale; records carry `mode`, stay under `target/benchmarks/smoke/`, never become a baseline | `experiment.rs` `Mode`, `Record::publish`; `levels.rs` `smoke`; handwritten smoke removed | `experiment::tests::smoke_runs_the_selected_workflow_and_stays_out_of_history`; real `mise run bench:smoke` below | PASS |
| A smoke shows the workflow runs: it judges no criterion and is PASS or INVALID | `experiment.rs` `judge_criterion` smoke return | the same test: a smoke missing its p99 limit 18× still PASSes; a missing reading and zero CPU are INVALID | PASS |
| A full command runs a smoke prepass and starts only when it is not INVALID | `levels.rs` `prepass`; `Declaration` `prepass.failed` refusal | the same test (failed prepass refuses before preparation; passing prepass is kept on the record); real run below | PASS |

Focused lanes (repository Postgres wrapper):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --lib --bin capacity -E 'test(/^capacity::tests::/) | test(/^experiment::tests::/) | test(/^query::run::tests::/) | test(/^query::competing::tests::/) | test(/^gateway::run::tests::/) | test(/^gateway::capacity::tests::/) | test(/^verification::run::tests::/) | test(/^client_queue::/) | test(/^tests::/)'"
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/)'
mise exec -- cargo clippy --locked -p wyrd-testing --all-targets -- -D warnings
mise run fmt
```

46 passed at `d135f13e2`; the 14 `experiment::tests` pass at `2f43ca9bd`; clippy and fmt are clean.

### Real-path smoke

From a clean detached worktree at `2f43ca9bd` (`git status` empty, no `--allow-dirty`):
`mise run bench:smoke` exited 0 in 822 s. A report was regenerated with
`capacity report target/benchmarks/smoke/verification/direct-verify/20261025T215137Z-2f43ca9bd.json`
(exit 0; header `SMOKE: functional pipeline evidence …`). `benchmarks/` holds only
`baselines.toml` afterwards.

| Record (`target/benchmarks/smoke/…-2f43ca9bd.json`) | Mode | Verdict | Steps / reps | Min server CPU |
|---|---|---|---|---|
| `bifrost/writes/20261025T214545Z` | smoke | PASS | 9 / 9 | 0.59 s |
| `bifrost/hot/20261025T214845Z` | smoke | PASS | 5 / 5 | 3.11 s |
| `bifrost/parquet/20261025T214932Z` | smoke | PASS | 10 / 10 | 3.29 s |
| `verification/direct-verify/20261025T215137Z` | smoke | PASS | 4 / 4 | 3.90 s |
| `gateway/overhead/20261025T215225Z` | smoke | PASS | 12 / 12 | 5.51 s |
| `bifrost/competing/20261025T215451Z` (L2) | smoke | PASS | 1 / 1 | 29.84 s |
| `verification/competing/20261025T215704Z` (L2) | smoke | PASS | 1 / 1 | 20.04 s |
| `gateway/capacity/20261025T215815Z` (L2) | smoke | PASS | 1 / 1 | 26.09 s |

At `d135f13e2` the same records carried timing observations (writes 110, hot 5742, Parquet
2526, direct-verify 12581, overhead 12796, L2 Bifrost 6190 in 54 series, L2 Verification
27000, L2 Gateway 15750) and scan facts (hot 2871 queries, Parquet 1263, L2 Bifrost 1512).

`bifrost/writes/20261025T214449Z` is INVALID: an orphaned server from an interrupted run held
port 8080, and the smoke reported the failed boot as INVALID rather than a result.

Prepass: `capacity l1 verification` (full) ran its smoke prepass in 49 s, wrote
`verification/direct-verify/20261025T215952Z-2f43ca9bd.json`, then proceeded into the full run,
which was stopped by hand; no full record was written.

### Diagnoses of defects the real smoke found

| Symptom | Evidence | Cause | Fix |
|---|---|---|---|
| Aggregate stopped at the first failing plan | `bench:smoke` under mise `set -e` | a non-zero plan aborted the script | `9516f6c22`: `cmd \|\| keep $?`, exit with the worst code |
| Hot reads INVALID: scan facts missing | `OracleQueryScanStats::finalize`, `oracle/exec.rs` | memtable/LiveScribeExec reads have no scan facts by design; spec requires them only for Parquet | `c0e4b47d3`: require them only for Parquet |
| L2 Bifrost `WYRD_LOADER_400_INVALID_ENVELOPE` | `[WYRD_DRIFT_400_VALIDATION] condition Above is not supported; use Statistical` | the fixture's drift Verifier used an unsupported condition | `9f6ae848f` plus `client_queue::fixture::tests::drift_verifier_is_a_valid_card` (RED on the old fixture) |
| L2 queue never drained; recovery empty | the drain was timed with holders still admitted; a 30 s grace cannot fit a 36 s smoke window | measurement order and a fixed grace | `ea442a3c1`: release before draining; `recovery_grace_fits_the_window` |
| L2 missed arrivals | the 433rd query was due 144 ns before the end | float rounding of the arrival count | `2f27ce132`: plan the rounded count; `mixed_queries_are_offered_at_a_fixed_rate` expects exactly (432, 432, 0) |
| L2 send-lateness p99 26–32 ms | a probe showed every loop stalling together, including at t=0 | client-queue lanes ran synchronous WyrdState work inside the scheduler's task | `d135f13e2`: one task per lane; late sends went from 431 to 0 |

### Limits

- The spec names four Bifrost L1 experiments; the code has three (writes, hot, Parquet).
- One writes run at `9f6ae848f` timed out migrating (`pool timed out`); it did not recur and its cause is not identified.
- Full-size suites were not run. Earlier smokes and the 2026-10-24 full run show real server findings left for production benchmarking: Verification at 64 clients is refused with `WYRD_VERIFICATION_429_STAGING_FULL` (the Scribe outbox at its 49,152 limit; 42,459 refusals in the full run), and Gateway sheds captures at 16 and 64 clients.
- TASK-005's rewording sits in another contributor's uncommitted edits and is not part of this candidate.

Result: `IMPLEMENTED`.
