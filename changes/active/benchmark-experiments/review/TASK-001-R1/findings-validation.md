# Independent findings validation — TASK-001

Recommended verdict: **FIX_REQUIRED**.

## Subject, independence and coverage

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`; base `0739bbf38af7cc05bbfefb09850490c55432ec2d`; candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`. First full three-dot review, no prior acceptance credit. Approved authority: revision-5 `changes/active/benchmark-experiments/spec.md`; original `tasks/TASK-001-benchmark-experiments.md`. Its approval paragraph overrides stale proposed/draft wording in that task.

Read scope, claim comparison, all five discovery reports, verification.md and focused followup-review.md; specification/task, root AGENTS, spec-driven workflow, maintainer/Rust guides and applicable Bifrost measurement/resource and security audit authority. CodeGraph preceded source searches. Independently inspected actual range inventory, candidate telemetry via Git, installed tracing visitor, full collector decode/intake/collection, resource/window/derivation owners, affected Bifrost producers/consumers, Gateway declaration/scheduler/request lifecycle, runner/profiler and cited rustdoc/test bodies. The follow-up independently validated the two specific Verification sibling gaps discovered during validation; I inspected those paths too. No comprehensive rediscovery, source edits, Cargo or benchmark runs occurred here. All shell commands used mise.

Two pre-existing uncommitted repair files remain outside candidate; their passing exporter test cannot repair its unsigned producer. Selected reports cover behavior, invariants, ownership, standards and system resilience. Maintainer's empty ownership ledger is accepted within scope: these functional defects do not establish a separate abstraction defect. Existing owners to extend are telemetry, Oracle terminal scan evidence, Collector/Population, portable process accounting, Repetition derivation, engine drivers, shared runner and Profiler. No replacement transport/exporter/endpoint/query/launcher/store or dependency is earned.

## Proposal reconciliation

| Proposal IDs | Disposition | Final ID |
|---|---|---|
| BEH-01, INV-01, SYS-001 | CONFIRMED; one shared producer cause | FIND-TASK-001-1 |
| BEH-02, INV-06 | REVISED; consolidate required Bifrost collection and request association, no per-window fixture rebuild | FIND-TASK-001-2 |
| BEH-03 | CONFIRMED | FIND-TASK-001-3 |
| INV-02 | CONFIRMED | FIND-TASK-001-4 |
| INV-03 | CONFIRMED | FIND-TASK-001-5 |
| INV-04 | REVISED; select exact offered-write correlation | FIND-TASK-001-6 |
| INV-05; focused follow-up | REVISED; same-window work correction includes independently validated Verification sibling | FIND-TASK-001-7 |
| BEH-05 | CONFIRMED | FIND-TASK-001-8 |
| SYS-002 | CONFIRMED | FIND-TASK-001-9 |
| SYS-003 | CONFIRMED; failed final save differs from measured instrumentation cost | FIND-TASK-001-10 |
| REP-001; focused follow-up | REVISED; batch three verified sibling docs with existing bounded gate correction | FIND-TASK-001-11 |
| BEH-04 | REVISED; original qualification gate, no new broad remediation suite | FIND-TASK-001-12 |

No proposal is rejected outright. Corrections are narrowed/consolidated below; reviewer agreement is not the proof. Human smoke additions and speculative fixture optimization receive no retrospective finding ID.

## Validated ledger

### FIND-TASK-001-1 — real timing export cannot supply accepted durations

**CONFIRMED; INCORRECT.** Sources BEH-01, INV-01, SYS-001. Violated REQ-007, AC-001/005, task scenario 4.

Candidate `crates/shared/wyrd-telemetry/src/lib.rs:323–344` converts elapsed/excluded nanos to u64. Installed tracing-opentelemetry SpanAttributeVisitor (`src/layer.rs:498–555`) implements record_i64 and record_debug, without record_u64; unsigned fields become OTel strings. `crates/wyrd/wyrd-testing/src/capacity.rs:528–600` requires nonnegative OTLP IntValue. Intake preserves malformed reasons; collect (`819–836`) waits for timings that cannot decode. Gate writes, public query terminal, OTLP exports, Verification and Gateway share this producer. Candidate capture test stringifies attributes and misses the representation defect; arithmetic test constructs integers itself.

Ordinary supported requests provide no valid judged latency across all engines. Four failed 30-second collection waits per nominal 35-second Bifrost step amplify budget failure. Deleted historical timings cannot prove a more precise setup attribution or a performance regression.

**Smallest correction:** emit signed integer fields at record_timing with documented representable saturation for both components; preserve strict parser, normal tracing and all callers. A permissive string parser would preserve the broken producer contract. No new exporter/transport/dependency or weakened budget/window is needed.

**Focused proof:** real telemetry init → OTLP exporter → Collector regression, asserting integer exactness above u32::MAX and elapsed/excluded/residual behavior. The pre-existing repair's reported 1/1 PASS supports the repair approach only. Human-required release smoke remains separate additional proof.

### FIND-TASK-001-2 — Bifrost omits required metrics and per-request scan evidence

**REVISED; MISSING.** Sources BEH-02, INV-06. Violated REQ-006/007/014, AC-005 and task scenario 4.

`query/isolated.rs:158–189` divides process scan-counter deltas by query count and always marks storage_ratio missing because publication continues. `query/mod.rs:837–980` L2 declares some W/C metrics but omits provisioned query/row/byte throughput, byte efficiency, CPU/operation, storage ratio and memory intensity. `query/competing.rs:241–340` discards opening/closing scrapes and supplies no corresponding byte operands. `experiment.rs:1258–1310` derives declared metrics only, so recorded P/CPU cannot fill those omissions.

Oracle owns final OracleQueryScanStats, but `oracle/mod.rs:558–615` emits process counters only. `capacity.rs:482–503` Timing has identity/time without scan facts. Two queries with unequal scan work/time cannot be reconstructed as distinct triples; mixed L2 has no scan evidence. Even after integer repair, supported levels cannot produce specified measurements or regenerate them from records.

**Smallest correction:** extend existing Bifrost declarations/readings and shared collection. Attach already-owned final Oracle rows/bytes to existing trace context; associate request trace/process/window with its server timing, preserving unavailable facts explicitly. Retain summary facts needed for record-only reporting. Measure retained Arrow input bytes and published stored bytes for the same data after a declared publication/compaction settlement using existing Residence/Scribe/storage owners; state excluded WAL/staging/catalog. Supply missing L2 W/B operands and derive C/P/T normalizations at Repetition with total mixed server CPU unchanged. Missing required collection is not inapplicability.

Required measurement justifies the collection changes; deleting labels or reporting averages cannot close it. No new public endpoint, identity metric label, second parser/pipeline, repeated scan execution or per-window fixture rebuild is prescribed. If an actual public-contract/architecture change proves necessary, stop under the original material stop condition rather than invent it here.

**Focused proof:** unequal query rows/bytes/durations exported out of order retain correct triples; missing facts remain unavailable/INVALID as applicable. Both isolated/mixed readers and persisted reports expose required values. Known C/P/T/W/B and settled same-data stored/input bytes recompute all derivatives. Use focused fixtures, not a new complete long suite.

### FIND-TASK-001-3 — Bifrost L2 has no offered query schedule

**CONFIRMED; MISSING.** Source BEH-03. Violated REQ-009 and task scenario 10.

`query/mod.rs:879–912` declares only ingest rate and explicitly closed-loop queries. `query/competing.rs:456–488` runs 32 loops awaiting each result before issuing the next query. mixed_case preserves the mix; only ingest offer schedules independent arrivals. A slow result silently lowers offered query load. No pre-measurement rate establishes at least 4,000 queries/200 scans in a six-minute repetition. Later sample rejection is not that declaration.

**Smallest correction:** extend existing mixed-query declaration/driver with a fixed offered query rate capable of required mix/population, schedule independent arrivals under the unchanged 32-client bound, and record query scheduled/sent/completed/missed/lateness through existing arrival validity. Preserve ingest calibration, mix, phases, windows and sample floors. Reuse existing scheduling/owned-task mechanisms; no generic driver or new server launcher.

**Focused proof:** controlled time with slow completion leaves later due times unchanged; offered arithmetic reaches 4,000 total/200 scans; exhausted client capacity produces missed arrivals and INVALID; normal schedule preserves fixed mix.

### FIND-TASK-001-4 — collector CPU is charged to the driver

**CONFIRMED; VIOLATION.** Source INV-02. Violated explicit REQ-006/007 helper boundary and task scenario 4 PID/resource/health evidence.

`capacity.rs:647–654,667–696,734–740` runs Collector as a Tokio task inside driver and declares its CPU driver cost. driver_cpu_seconds (`449`) reads the same process through sysinfo; Window and Verification resource windows consume it; `experiment.rs:2087–2109` judges driver saturation from combined cost. There is no independent collector resource evidence. Quiet-host descendant exclusion cannot split same-process CPU.

Supported full exports can look like saturated traffic generation; records cannot distinguish costs as explicitly required.

**Smallest correction:** run the existing Collector as a separately measured declared benchmark helper using existing plain-child/sysinfo owners; retain its decoder/service and declare PID, resources and collection health independently. Server instrumentation/export stays server CPU; helper loss, unavailable required readings or collection saturation/loss invalidates timing evidence. The approved specification already selects this helper boundary. No new production ownership decision, transport/dependency, estimated subtraction or relabelled combined reading.

**Focused proof:** independently controlled helper/driver readings produce distinct costs/health and saturation decisions; exercise helper startup/stop/loss and honest missing readings; quiet-host excludes the owned helper without hiding unrelated processes. Actual hardware accounting remains the original qualification gate.

### FIND-TASK-001-5 — collector has no retention bound or overflow invalidation

**CONFIRMED; MISSING.** Source INV-03. Violated specification opening/REQ-007 bounded collection and Bifrost owned-buffer authority.

`capacity.rs:611–640` holds unbounded timing/malformed vectors; Intake extends each export. Collector::start (`679`) sets usize::MAX message size. Entire workload/setup populations accumulate before collection; BSP queue size bounds exporter pending batches, not receiver retention across batches. No overflow state exists. Supported long full-recording windows can exhaust the driver and lose partial evidence; independently the explicit overflow-to-INVALID path is absent.

**Smallest correction:** impose finite declared incoming/retention/diagnostic bounds on existing Collector, stop retaining excess and retain bounded specific loss/overflow evidence attributable to affected measurement. Preserve in-limit exact/raw evidence. Use conventional finite message/state limits, no generic store or dependency. Do not silently acknowledge completeness after loss.

**Focused proof:** real local OTLP exceeding a small controlled bound leaves retention bounded and a precise INVALID loss reason; in-limit samples remain complete/exact. Demonstrate setup/window loss attribution or clearing so loss is neither silently erased nor accidentally misattributed. No synthetic host saturation.

### FIND-TASK-001-6 — incidental mixed writes can replace a missing required timing

**REVISED; INCORRECT.** Source INV-04. Violated REQ-007 and task scenario 4 matched completeness.

`query/competing.rs:324–359` requires trace IDs for successful explicit queries but only boundary-count minima for offered writes. Client queue lanes at `252–267` concurrently write through WyrdState to the same native Gate timing boundary. Collector::traced/collect (`capacity.rs:763–889`) permits extras. read_ingest (`579–593`) judges all successful writes of those boundaries by phase. Drop one offered native timing and add one distinct lane timing: minimum and unique-span checks still pass. Supported mixed traffic can therefore conceal a missing slow/write observation.

**Smallest correction:** correlate every offered write's expected boundary/outcome using existing caller trace/request context through native and OTLP sender transport owners, and reconcile exact request identities in Collector. Preserve failures and incidental lane/probe populations separately. Keep the consumer completeness guard at Collector because it owns received evidence. Aggregate exact counts conflict with intentional shared writers; lower bounds cannot prove identity. No parallel transport/identity scheme or removal of background work.

**Focused proof:** omit a required offered write while supplying an unrelated same-boundary span enough for today's minimum; remain INVALID. Complete out-of-order matches pass; duplicates/missing/outcome mismatches stay explicit without dropping phase/failure evidence.

### FIND-TASK-001-7 — drained completions use a frozen resource denominator

**REVISED; INCORRECT.** Source INV-05 plus independently validated focused follow-up. Violated REQ-006 matching W/C/T windows and task scenarios 3/4.

Bifrost `query/competing.rs:241–305` closes Window at end while joined request futures finish later. read_queries (`544–550`) and read_ingest (`618–624`) include all successful late Asked/Acked; their completion instants already exist. W/T and W/C (`experiment.rs:1258–1310`) use earlier CPU/T. A final in-flight request completing after end is ordinary required drain work, so this inflates work against an incomplete denominator.

Verification follows the same invariant: `verification/run.rs:697` observe freezes ResourceWindow. arrivals (`768–775`) records calls after await into their scheduled-stage tally, then joins all pending requests. competing clones those tallies, retains fixed seconds/frozen CPU (`380–384`) and sums their successful work into verifications (`503–513`), including post-freeze completions. Ended's server latency cutoff does not filter this numerator. Full trace is independently confirmed in followup-review.md; I inspected competing/observe/arrivals/derive. Gateway closes resources after joined traffic, so this specific path is not retained there.

**Smallest correction:** retain completion cutoffs/timestamps at existing Bifrost/Verification tally owners and count measured successful W/B only over the declared C/T interval. Keep late completions, every failure and full expected timing/arrival/drain reconciliation separately; do not forgive late errors, exclude background CPU or arbitrarily extend fixed repetitions. Correct both siblings once for the same invariant; do not redesign verification stage or submission semantics.

**Focused proof:** one before-end and one late completion credit only the former to measured work/bytes and derived rate/efficiency, while both remain in total/drain/timing reconciliation. Fixed resource endpoint remains unchanged. Exercise both established engine readers.

### FIND-TASK-001-8 — Gateway loses common timing controls

**CONFIRMED; MISSING.** Source BEH-05. Violated REQ-001/007/013 reproducible timing configuration/clock/resolution/cost.

levels.rs (`136–146`) assembles controls[timing] from Collector declaration plus timer resolution. gateway/mod.rs overhead (`227–268`) and capacity_declaration (`502–550`) replace the whole map and never restore it. Runtime still exports, but both records omit common clock/resolution, sampling/batch settings and collection/cost attribution. Metric boundary prose cannot restore those fields.

**Smallest correction:** extend assembled controls with Gateway entries rather than replace them. Reuse the common declaration, no duplicate setting/option.

**Focused proof:** use the real shared base for both Gateway declarations, persist/read records, and assert timing controls survive alongside engine controls at existing declaration/record tests.

### FIND-TASK-001-9 — interrupted Gateway L2 leaves detached admissions and calls

**CONFIRMED; INCORRECT.** Source SYS-002. Violated task scenario 3 and shared runner cancellation/partial-evidence contract.

gateway/capacity.rs Driver::spawn (`806–856`) returns a JoinHandle; Capacity::step (`999–1018`) awaits it. Shared bounded drops measure on deadline/interrupt; dropping JoinHandle detaches its scheduler. open_loop (`313–349`) independently tokio::spawns each call without an owned task set. finish (`1125–1133`) shuts down the server without stopping either task level. Ctrl-C in a supported multi-minute schedule permits new requests during settle/drain, extending cleanup and corrupting partial cleanup evidence. Eventual runtime teardown is too late.

**Smallest correction:** give the existing scheduler and all request descendants measurement-lifetime ownership using Tokio owned-task/cancellation mechanisms already used by siblings. Cancel stops launches and cancels or boundedly reconciles in-flight work before server drain. Fix both levels; aborting scheduler alone leaves calls detached. Preserve normal rates/admission/capture/shutdown semantics; no process-manager abstraction or new product concurrency decision.

**Focused proof:** actual scheduling/launch logic with controlled future/local upstream, cancelled before a later arrival; advance beyond it and prove no launch/live descendant before finish. Preserve normal counts and runner partial INVALID evidence.

### FIND-TASK-001-10 — optional capture save failure changes budget verdict

**CONFIRMED; INCORRECT.** Source SYS-003. Violated REQ-015/task scenario 8 diagnostic-only profiling.

experiment.rs (`1650–1697`) awaits Profiler::finish before workload.finish and budget judgment. profile.rs (`134–179`) waits 60 seconds per unresponsive saver before killing it and noting incomplete capture. A valid workload with less headroom becomes budget.exceeded INVALID solely because optional saving failed; drain also waits. This is the explicit supported unusable-capture case, distinct from real instrumentation cost during measured work.

**Smallest correction:** at existing runner/Profiler owners, initiate capture stop without postponing required workload stop/drain; exclude optional artifact-saving delay from workload budget judgment while retaining actual artifact/finish timestamps, bounded capture ownership/notes and genuine workload overruns. No budget increase, overrun suppression or replacement profiler.

**Focused proof:** delayed/unsavable capture and otherwise within-budget workload near limit preserve no-profile verdict and prompt drain with an incomplete note. Actual workload overrun stays INVALID with/without profile. Short controlled durations suffice.

### FIND-TASK-001-11 — mandatory lifecycle and test rustdoc is incomplete

**REVISED; VIOLATION.** Source REP-001 plus independently verified focused sibling expansion. Root AGENTS Code/agent_docs/rust.md require Errors/Panics and async cancellation/partial-effect contracts; missing rustdoc explicitly blocks merge.

experiment.rs Workload prepare/measure/finish (`1342–1357`) have only summaries without Errors/cancellation obligations. query/isolated.rs (`232–235,313–324`) has the same omissions while propagating setup/loading/settlement/OTLP/stop failures and partial progress. Candidate telemetry lib.rs (`560–563`) and oracle/exec.rs (`2087–2090`) add assertion/expect-bearing tests without Panics sections. Verification Bench prepare (`535`), measure (`581`) and finish (`594`) likewise omit required contracts; followup-review.md independently checked full bodies/diff and actual partial effects. I inspected these bodies too. finish is synchronous internally despite async signature: do not invent an await-cancellation guarantee.

**Smallest correction:** supply accurate Errors/cancellation/partial-effect/retry descriptions for the cited existing lifecycle items and Panics for cited tests. This batches eleven bounded items with functional remediation; it is a mandatory gate, not an asserted new cleanup defect, new checker or style-only round. Document actual behavior without promising missing guarantees.

**Focused proof:** inspect docs against full bodies and owning contracts, then existing format/lints. No benchmark run solely for documentation.

### FIND-TASK-001-12 — required original real qualification is not evidenced

**REVISED; MISSING.** Source BEH-04. Violated AC-001/005/007/010, original task V13–V15 and missing-required-Linux-evidence stop condition.

Original task (`688–805`) requires real named production regressions and complete release L1/L2 Linux x86_64 evidence, raw timing/C/P/T/W/B recomputation, overlap timelines and persisted/regenerated report agreement. No source-qualified retained evidence supplied establishes those gates. verification.md contains deterministic tests, repair-only exporter proof and lints, explicitly no real smoke/L1/L2. Scripted workloads can pass without release servers; reported real L1 produced malformed spans and deleted raw records. This does not establish that every historical check was never run; it establishes unavailable acceptance proof.

**Correction:** preserve original qualification at the final execution/change gate after focused fixes. Retain exact commands/source identity, complete matrices, honest verdicts, recomputations and original real accounting/concurrency/production-guarantee evidence. Start with user's ordered clean-tree smoke/separate L1 runs, report each. Optional macOS/arm64 hardware runs are not new blockers. Never manufacture PASS by weakening thresholds.

**Proof/burden:** no new code or additional broad suite. Do not place V13–V15/full journeys into focused remediation commands; existing final gate runs them once. Focused source remediation can finish while original qualification remains explicitly pending; unit tests cannot waive required real observation.

## Acceptance and correction-burden reconciliation

| Obligation | Evidence/result |
|---|---|
| REQ-001 declaration | Shared immutable mechanism accepted; FAIL Gateway controls, 8 |
| REQ-002/004/005 validity/complete steps | Deterministic judgment tests green; FAIL collection/helper/completeness/cancellation, 4–6/9 |
| REQ-003/008 repetitions/full L1 | Declared matrices/windows present; FAIL export and unavailable full duration, 1/12 |
| REQ-006/007/014 metrics/report | FAIL producer, missing Bifrost facts, helper scope and W/C/T, 1/2/4–7 |
| REQ-009 L2 | Mix/harmonic ingest calibration retained; FAIL offered query schedule and real proof, 3/12 |
| REQ-010/015/016 invocation/reuse/retirement | Static selected inventories accepted; FAIL optional finalization, 10; actual command qualification pending 12 |
| REQ-011/012 baseline/noise | PASS reviewed provenance/static scope; report inventory found no broken-new-run source contamination; no baseline correction justified |
| REQ-013/014 persistence/report | Shared mechanisms/tests accepted; missing facts/controls cannot regenerate; FAIL completion through 2/8/12 |
| INV-001–009 and AC-001–010 | Static preservation/portability coverage accepted where unaffected; mapped functional failures and original Linux qualification 12 prevent completion |
| Mandatory repository standards | Static ownership/tiering accepted; FAIL changed-item docs, 11 |

All applicable obligations are covered by selected reports plus focused source validation, not only agreement. No new material product/API/architecture/security/persistent-data decision is needed for selected correction outcomes. Purely private mechanics remain implementation-owned. Explicit helper separation is already approved; no estimate can waive it. No DRIFT is retained solely for size or conventional alternatives.

Reported nine shared tests, three exact engine tests and lints PASS ran on the preserved worktree including pending repairs; they do not certify candidate export. fmt:check failed only the pre-existing repair test layout: no candidate-format finding is invented, but repair must be formatted before completion. No baseline decimal inventory is claimed beyond behavior report's stated scope.

Bifrost has 20 measured L1 steps: nominal 700 seconds, about 16 seconds successful collector settling and 20 seconds host checks, leaving about 164 seconds for other lifecycle costs. Broken export adds approximately 2,400 seconds if every step runs. Fix exporter first; valid full-suite timing remains unproved. Deleted timings cannot justify speculative fixture optimization or altered windows/coverage/production semantics.

## Follow-up and human additions

Focused followup-review.md RESOLVED both specific validator-discovered sibling gaps: Verification docs join finding 11; Verification numerator/frozen-resource mismatch joins finding 7. It independently source-validated new scope before retention. No other conflict remained and no further discovery was needed. No prior rounds/convergence assessment apply.

Human-requested mandatory seconds-scale release smoke per engine (integer/nonempty timings, CPU/throughput and precise diagnostics), clean-tree sequential L1 runs, valid Bifrost phase-cost diagnosis and authorized commits/no push are separate additions. They receive no retrospective FIND ID. Review itself implements none.

Skipped: source edits, new Cargo checks, long runs and optional-platform execution. Risk: signed export repair alone does not establish compliant accounting, metrics, lifecycle or complete budget qualification.
