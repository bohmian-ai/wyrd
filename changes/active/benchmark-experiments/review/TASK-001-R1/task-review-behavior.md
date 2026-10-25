# TASK-001 behavior acceptance audit

Result: **FAIL**.

## Immutable subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Base: `0739bbf38af7cc05bbfefb09850490c55432ec2d`.
Candidate: `aaa152dd4243d57752b73d3a4e9c140bccc7d171`.
First review: complete base...candidate range; no previous acceptance credit.
Approved authority: `changes/active/benchmark-experiments/spec.md` revision 5.
Original task: `changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md`.

This is independent behavior discovery. I inspected source, declarations, actual
range inventory, producer/consumer paths and tests without other reviewers'
conclusions. No source edits, Cargo commands, benchmark runs, or subreviews were
performed. Every shell invocation used mise. CodeGraph was used before source
search. Only this report was written. The existing uncommitted i64 repair and
new export regression test are outside the candidate; candidate telemetry was
read with git show. Their passing test in `verification.md` does not repair the
immutable candidate.

Authorities consulted: root AGENTS; task-review skill; spec-driven-development;
maintainer-style; the approved spec/task; normative baseline mapping in research;
Bifrost telemetry/measurement meanings, Wyrd Doctrine, security audit integrity;
benchmark README and TESTING command surface. Scope reports no nested AGENTS
for this write set. No task mechanics were found to override the approved spec.

Navigation: levels assembles source/build/host/timing controls and delegates to
experiment::run; engine declarations and Workload implementations provide real
SDK/OTLP/Gateway work; Collector decodes record_timing output and supplies
Population to Repetition; shared derivation, judgment, publication and render
own results. Query isolated/competing, Verification direct/competing and Gateway
overhead/capacity are the engine consumers. Existing release_server, os,
capacity, flags and profile owners are reused. Deleted load/SloGate/Forge/report
owners and task wiring were inspected in the range inventory. Maintainer,
resource-invariant, standards and lifecycle specialists cover their assigned
boundaries independently; this report does not claim their conclusions.

## Acceptance matrix

PASS below means source/static acceptance for the stated row, with verification
limits shown explicitly. It is not a claim the unexecuted release journeys passed.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 fixed complete declaration | experiment::Declaration::refusals/seal; levels::assemble; engine declare functions | declaration_precedes_workload exists; Gateway drops timing controls, BEH-05 | FAIL |
| REQ-002 validity, explicit reasons and budget | experiment::run, repetition_reasons, host_reasons; Collector validates malformed/missing populations | invalid_observations_preserve_reasons exists; no executed full candidate qualification available | PASS (static checks; qualification incomplete) |
| REQ-003 warmup + three repetitions, independent summaries | Level::timing; experiment::run phase loop, summarize; engine fresh-identity cursors/stateful sequences | repetitions_exclude_warmup_and_bound_execution exists; actual runtime proof missing, BEH-04 | FAIL (proof) |
| REQ-004 verdict/exit precedence | Verdict::combine; experiment::run/judge/judge_criterion; levels::aggregate/execute | criteria_distinguish_regression_noise_and_invalidity and level_commands_preserve_records_and_exit_precedence exist | PASS (static) |
| REQ-005 all steps judged, expected overload vs error | Bifrost phase_metrics/queue criteria; Verification STAGES criteria; Gateway capacity criteria; all preserve incorrect/error counters | L2 test uses scripted readings; real overlapping overload proof missing, BEH-04 | FAIL (proof) |
| REQ-006 CPU C, provisioned P, explicit units/scopes | Window/Usage; Repetition::derive; host/process owners reused | arithmetic test exists; Bifrost L2 omits required provisioned metrics, BEH-02 | FAIL |
| REQ-007 exact raw server timing, overhead residuals and all applicable engine metrics | record_timing -> OTLP -> decode_timings -> Population; query Oracle scan counters | exact OTLP test in candidate manually constructs integer fields; actual producer emits wrong field type, BEH-01; metric gaps BEH-02 | FAIL |
| REQ-008 full L1 <=900 s, isolated coverage | Bifrost READS plus three write sweeps = 20 measured steps; Verification four direct steps; Gateway 3 cases x4 clients; shared 900 s budget | declarations covered by level_one_declares_complete_engine_coverage, not full release run; BEH-01/04 | FAIL |
| REQ-009 L2 120s +3x360s, concurrent phases, calibration/mix/sample feasibility | Competing declarations and implementations preserve phase sequences, 100M fixture, harmonic calibration, signal and query mix | Bifrost queries have no offered schedule/rate, BEH-03; real timelines absent, BEH-04 | FAIL |
| REQ-010 opt-in command names, sequential aggregate/profile forwarding/docs | mise.toml:536-575; levels::aggregate/spawn_engine; main dispatch; README/TESTING | source inventory and command wiring; no benchmark gate dependency in changed task definitions | PASS (static) |
| REQ-011 initial historical references, eligibility, units/provenance | all three baselines.toml; Baselines::criterion and convert; research normative mapping | parsed all TOML entries; Bifrost 145 eligible cfe21dc90 +102 excluded/unestablished; Verification16 and Gateway20 all excluded; no failed-new-run reference | PASS (provenance inventory; see limits) |
| REQ-012 conservative regression and spread | regression implements hi<.90b / lo>1.15b and median shift >spread | explicit boundary/noise cases in criteria_distinguish_regression_noise_and_invalidity | PASS (static) |
| REQ-013 versioned attributable append-only history, failures retained | Record::publish/read/parse; Source::detect/from_diff; levels::execute salvage; schema_version=1 | history_is_append_only_private_and_versioned exists; real candidate records absent, BEH-04 | PASS (static persistence; release proof absent) |
| REQ-014 exact common report, record-derived metrics and reasons | Record::render/step_cells, REPORT_HEADER; report dispatch reads Record | reports_regenerate_from_persisted_records exists; Bifrost missing metric evidence cannot be regenerated, BEH-02 | FAIL |
| REQ-015 shared experiment lifecycle and existing owners, optional diagnostics | common run/derive/judge/publish/render; engine Workload; reused flags/release_server/os/profile; samply external | representative scripted Workload integration and profiling_is_optional_diagnostic_evidence exist; no real profiling qualification | PASS (shared workflow static; qualification absent) |
| REQ-016 obsolete paths retired with workload purposes retained | range deletes SloGate/thresholds/load, duplicate engine reports, scoped Forge binary; new Bifrost competing retains compaction/query queue/client queue purposes | diff/consumer inventory; no replacement old bench aliases in changed task block | PASS (static) |
| INV-001 tenant/permission/result protections | engines use real WyrdClient/setup credentials and exact-answer validation; no changed tenancy bypass in inspected workload paths | original V13 real production regressions have no execution evidence here | FAIL (proof) |
| INV-002 acknowledged durability/publication | native/OTLP writes await owner acknowledgments; Residence settlement; client-queue row_mismatch criteria | original durable-recovery/shutdown proofs unavailable in packet | FAIL (proof) |
| INV-003 non-blocking audit and measured cost retained | real permission paths remain; no production audit bypass in range | existing audit seam V13 required, execution unavailable | FAIL (proof) |
| INV-004 distinct CPU scopes and normalizations | shared Usage/derive; L2 Bifrost metric gaps BEH-02; resource specialist audits disjoint collector/helper accounting | arithmetic test exists; no real CPU recomputation | FAIL |
| INV-005 frozen criteria; invalid evidence not baseline | seal, reasons precedence, explicit historical files; no auto-promotion | deterministic judgment tests exist; malformed evidence correctly INVALID but no usable timing evidence BEH-01 | PASS (static policy) |
| INV-006 synthetic summaries and symbolic secrets | Invocation::new scrubs secret/endpoint names, host summary excludes hostnames; fixture data synthetic | privacy cases in history test; no secret data found in baseline files | PASS (static) |
| INV-007 no builds/load generation during measured run | bench:build dependency completes before driver starts; host eligibility uses observations; deterministic injected workload tests | command/source inspection; no reviewer-generated host load | PASS (static) |
| INV-008 append-only versioned records | create_new temporary write / collision-safe publish; parse schema validation | real filesystem history tests exist | PASS (static) |
| INV-009 unchanged supported-platform implementation | shared sysinfo/available_parallelism, plain-child launcher, external optional samply; old scopes removed | no actual Linux qualification, BEH-04; optional arm/mac runs not demanded | FAIL (required proof) |
| AC-001 each complete L1, raw recomputation, report agreement | engine declarations present | candidate exact export broken; no complete records; BEH-01/04 | FAIL |
| AC-002 valid regression vs baseline | regression and baseline snapshot implementation | deterministic examples and integration source present | PASS (static) |
| AC-003 noise cannot excuse absolute SLO | independent criterion judgments plus strict Op::Lt | tests cover noise and strict boundaries | PASS (static) |
| AC-004 invalid data reasons/partial record | shared validity and publication integration | controlled validity tests exist; no required tests executed by this reviewer | PASS (static; execution not claimed) |
| AC-005 actual all L1/L2 Linux accounting/profile | static portable workflow present | no real source-qualified records/recomputations; BEH-04 | FAIL |
| AC-006 machine-readable history/report/collisions | Record contract, read/render and collision handling | filesystem and version tests exist | PASS (static) |
| AC-007 real overlapping L2 phases | three engines have concurrent orchestration | Bifrost schedule requirement missing BEH-03; real records absent BEH-04 | FAIL |
| AC-008 contributor reuse | Workload + shared runner, declaration integration test | representative scripted Workload exercises common lifecycle | PASS (static) |
| AC-009 migration/discoverability | retired paths and new mise/docs, client-queue judged lanes, Gateway fixed defaults | static inventory | PASS (static) |
| AC-010 preserved production guarantees | owner paths and correctness checks retained | original real production checks not evidenced | FAIL (proof) |

## Proposed findings

### BEH-01 — INCORRECT: the real timing producer cannot supply any parsed latency

Obligations: REQ-007, AC-001, AC-005, task scenario4 and V14/V15.
Location: candidate `crates/shared/wyrd-telemetry/src/lib.rs:323-342`,
`crates/wyrd/wyrd-testing/src/capacity.rs:528-559,585-600`.

`record_timing` converts Duration::as_nanos to u64 and records it as a tracing
field. The installed tracing-opentelemetry Visitor implements record_i64 but
no record_u64 (`src/layer.rs:516,539`); its debug path creates OTel String.
`decode_timings` delegates elapsed/excluded to nanos, which accepts only a
nonnegative OTLP IntValue. Every Bifrost/Verification/Gateway successful real
request therefore produces a malformed observation instead of a sample.
The same shared producer has all timing writers: Gate write, query terminal,
OTLP traces/logs, verification direct, Gateway buffered/head/end.

The candidate producer test erases this distinction by reading attributes as
strings; the Collector arithmetic test builds IntValue by hand
(`experiment.rs:3949-3985`), never connecting the real producer to the parser.
The pre-existing repair's real export test passed during the parent review,
but that repair is outside this candidate (verification.md).

Consequence/materiality: every latency population is invalid and full runs
cannot produce required server evidence. A missing population makes each
Collector::collect wait up to30s (capacity.rs:819-833). Warmup plus3 repetitions
adds about120s to each nominal35s step. This is a source-backed explanation for
much of the reported Bifrost overrun; deleted timings cannot establish exact
historical attribution. Do not conclude separate setup slowness from the bad run.

Smallest correction: use the OTLP-supported signed integer representation at
the shared producer, with checked/saturating representable conversion and
correct rustdoc; retain strict typed parsing. Reuse the real Collector and
telemetry init in one regression test including a value above u32::MAX and
excluded residual arithmetic. No permissive string parser or alternate exporter.
Then the human-directed per-engine release smoke must prove actual requests,
CPU and throughput before any long benchmark. A smoke was not an original
independent criterion; it is now explicitly required by the user.

### BEH-02 — MISSING: Bifrost records omit required performance evidence

Obligations: REQ-006/007/014; AC-005; task scenario4 engine metrics.
Locations: `query/isolated.rs:158-189`, `query/mod.rs:837-980`,
`query/competing.rs:223-340`; producer `oracle/exec.rs:272-347` and telemetry.

L1 sets storage_ratio unconditionally missing because publication continues,
without arranging the declared settlement of matching input/published data.
This is unavailable required evidence, not an operation to which storage ratio
does not apply. L1 also computes only window-wide scan counter deltas divided
by successful query count. It does not preserve scan rows/bytes for each
request beside that request's server time. Timing has no scan fields; the
collector retains only duration/component/identity facts. Window averages
cannot reconstruct the required request association.

L2's complete metric list declares only total rate, queries_per_cpu_s,
table_aggregates_per_cpu_s, ingest_rows_per_cpu_s plus phase lifecycle/SLOs and
queue lanes. Competing discards Window's before/after scrape values and records
no byte readings. It has no queries/s/vCPU, rows/s/vCPU, uncompressed or wire
MiB normalizations, CPU/op, per-request scans, storage ratio or memory intensity
with either readings or applicability reasons. Shared derive computes declared
metrics only; having P on Repetition does not create these missing metrics.

Consequence/materiality: a nominal successful L2 can publish PASS while never
measuring the specified provisioned throughput, byte efficiency or scan work;
the report cannot regenerate evidence that is absent. L1 cannot provide its
required compression/storage evidence even after timing parsing is repaired.

Smallest correction: extend existing Bifrost declaration and measurement owners,
reuse Repetition::derive/Window and production Scribe/Oracle facts, and retain
per-request scan facts associated through existing traces. Measure stored/input
bytes for the same data after a declared settlement at a scope that fits the
approved experiment; do not prescribe a new endpoint or per-window fixture
rebuild. Preserve required L2 mixed CPU denominator and per-class work. Explicit
inapplicability is valid only for truly inapplicable operations, not an omitted
required collection path. Add focused metric/record tests with unequal scan
work across requests and recomputable C/P/T/W/B operands. A new parser or
measurement pipeline is unnecessary; weaker report labels cannot supply facts.

### BEH-03 — MISSING: Bifrost L2 has no offered query rate or schedule

Obligations: REQ-009 explicitly requires offered query rate capable of200 scans
(at least4000 total queries at the fixed5% share) per six-minute repetition;
original task L2 scenario preserves that feasibility declaration.
Locations: `query/mod.rs:879-912`, `query/competing.rs:93-104,285,456-488`.

The only declared rate is ingest_rows_per_s. The query control explicitly says
32 closed-loop clients. mixed() starts32 loops, each awaiting completion before
asking its next query; there is no offered query rate, scheduled query arrival,
or query scheduling health accounting. Only ingest has offer() and its arrivals.
The fixed sequence retains the60/25/10/5 mix but cannot establish independent
query arrivals or pre-measurement feasibility. A slow server silently reduces
the offered query load rather than measuring the declared offered query rate.

Consequence/materiality: the required heavy-scan population and open-loop L2
query-load claim have no declaration/driver source, irrespective of the integer
export repair. Sample-count rejection later cannot replace a required offered
rate chosen before observing performance.

Smallest correction: at existing Competing mixed-work owner, declare the query
arrival rate with the fixed32-client limit and unchanged mix, schedule arrivals
independent of completion, and include query scheduling/sent/completed/missed
health in shared validity. Reuse established arrival accounting/flag parsing;
no additional server launcher or generic transport. Focused controlled-clock
proof must show a slow completion does not postpone the next scheduled arrival,
rate arithmetic offers at least4000 total/200 scans, and driver misses invalidate.
Do not weaken the200-sample floor or substitute a different mix.

### BEH-04 — MISSING: required real qualification evidence is absent

Obligations: AC-001/005/007/010; task V13/V14/V15 and material stop condition.
Locations: original task `:688-805` and acceptance mapping `:493-513`;
engine implementations and no committed execution record in the task packet.

The candidate contains deterministic scripted Workload tests, including
level_two_preserves_concurrent_work_and_recovery, whose injected readings can
PASS without starting a release server. They prove judgment behavior, not
production accounting, request timing, actual overlap, sample sufficiency,
setup/drain budget, real privacy/durability/audit regressions or report agreement.
There is no original execution evidence in the active packet and no retained
candidate run records/raw samples. The only reported real L1 attempt yielded
malformed spans; it supplies no usable qualification. No actual L2 result is
available. Prior rejected implementation evidence confers no credit.

Consequence/materiality: the explicit completion gate cannot be established,
and BEH-01 demonstrates the exact integration failure omitted proof would catch.
This is a missing original acceptance obligation, not a demand for new product
behavior or extra platform runs.

Correction: after focused fixes and the user's smoke, finish original real
Linux x86_64 qualification at the appropriate final execution/change gate,
retaining source identity, exact commands, complete matrices, windows, raw
matched timing recomputation, C/P/T/W/B recomputation, timelines and regenerated
report agreement. Start with the user-ordered three separate L1 runs and report
each result honestly. Missing arm/mac real runs are explicitly optional. Do
not add a second broad regression suite to a remediation task or manufacture a
PASS through changed thresholds; original qualified evidence remains necessary.

### BEH-05 — MISSING: Gateway discards common tracing controls from its records

Obligations: REQ-001 controls, REQ-007 clock/resolution/method and recorded
100% tracing configuration/cost; REQ-013 controls reproduce the measurement.
Locations: `levels.rs:138-153`, `gateway/mod.rs:230-268,502-550`.

assemble() adds Collector::declaration plus timer_resolution_ns as the common
controls["timing"]. Gateway overhead() and capacity_declaration() replace the
entire controls map with fixture/workload/limits/process entries. Both Gateway
records therefore lose the exact clock, effective resolution, operator sampling
ratio, batch settings, collection method and stated instrumentation cost that
the common declaration supplied. Runtime still exports, so this is a missing
attribution contract rather than an exporter failure.

Consequence/materiality: both public Gateway levels publish timing judgments
without the mandated collection/tracing controls, preventing an agent from
checking and reproducing how that evidence was collected. Individual metric
boundary prose does not restore sampling or effective resolution.

Smallest correction: preserve/extend the assembled controls when adding engine
controls, using the same existing common declaration as Bifrost/Verification.
No new configuration or duplicate timing declaration. Extend real engine
-declaration coverage to assert the common timing control survives both levels
and persisted record/report regeneration.

## Baseline provenance and budget conclusions

All baseline TOML files parse. Their only eligible source commit is cfe21dc90;
primary Bifrost/Verification/Gateway references remain excluded/unestablished.
Values and operands inspected against normative research include Parquet
selective c1 (460.935 queries/CPU-s,2169.505 us/query), native sweep c1
(815661.765 rows/CPU-s,227.8 MB/CPU-s), OTLP spans c1 (49554 rows/CPU-s,
42 MB/CPU-s,16 wire MiB/CPU-s), and queue source mappings. Files preserve
MB source units and conversion happens in the shared convert function. No
baseline entry references the broken new L1 run; no contamination evidence was
found. This is not a claim every printed decimal was independently recomputed.

The broken timing encoding itself forces long collector waits, so reducing
fixtures or windows before a valid export smoke would attack an unproven cause.
The full Bifrost L1 declaration is20 measured steps, nominal700s plus about16s
of mandatory collector post-waits, host sampling, setup, settlement and drain.
Budget feasibility still needs a valid full run; deleted historical timings
cannot be recovered or replaced by guesses.

## Limits and disposition

No tests or long benchmarks were run by this reviewer; the parent owns
sequential verification. No source was changed. The passing uncommitted export
repair test establishes a promising local correction only. Full runtime proof,
resource-accounting details and mandatory repository-rule checks remain with
the selected reviewers and original execution gates. Human-directed smoke and
commit/run instructions are separate additions, not retroactive FIND IDs.

Retain BEH-01 through BEH-05 for fresh independent validation; no intended final
orchestrator verdict is prescribed. Overall discovery result: **FAIL**.
