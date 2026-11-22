# TASK-001 independent validation

Current recommendation: **FIX_REQUIRED**, with six active findings.
The maintainer subsequently approved spec revision 2, voiding every previous
performance target for the refactored L1 cases. FIND-TASK-001-5 is therefore
**WITHDRAWN by approved scope change**, not fixed or rejected as a false
source claim. The other six findings and original TDD disclosure carry forward.
This update adds no independent discovery finding or new runtime evidence.

Original revision 1 validation: all discovery proposals were independently
traced against source and existing consumers. Seven findings were retained after
consolidating SYS-1 and SYS-4 at the lifecycle phase owner. No new blocker was
introduced by validation, and no rejected proposal creates remediation work.

Subject: `2418dc7848375c00ac6a991cafca88f722bcfdfe...1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`;
approved SPEC-l1-benchmark-protocol revision 1 and original
`tasks/TASK-001-standardize-l1-benchmark-sweeps.md`. Checkout HEAD remains
`6e158cd39407a5c25e5edad163633765d99fdad9`; its only candidate-relative
difference is the unrelated fence-lifecycle task-status record. Review writes
are confined to this review directory. No implementation was modified.

Read the invoked task-review skill, root rules, approved specification, original
task and evidence, applicable design/case mapping, primary behavior report,
system report and invariant report. Source navigation used CodeGraph before
direct inspection. Validation checked the actual diff and affected producer,
consumer and unchanged owner paths, rather than treating reviewer agreement
or GREEN summaries as proof.

## Proposal disposition

| Discovery proposal | Disposition | Retained reference |
|---|---|---|
| B-1: listing compiles the driver | CONFIRMED | FIND-TASK-001-1 |
| SYS-1: premature wrapper cutoff | REVISED: consolidated with SYS-4 under phase ownership | FIND-TASK-001-2 |
| SYS-4: teardown charged to unused drain | REVISED: consolidated with SYS-1; each failure mode remains required closure | FIND-TASK-001-2 |
| SYS-2: cleanup confirmation unbounded/late | CONFIRMED | FIND-TASK-001-3 |
| SYS-3: persistence duration discarded | CONFIRMED | FIND-TASK-001-4 |
| B-2: existing write floor targets dropped | CONFIRMED against revision 1; WITHDRAWN under maintainer-approved revision 2 | FIND-TASK-001-5 (historical only) |
| B-3: detailed server snapshots lost | CONFIRMED | FIND-TASK-001-6 |
| INV-1: smoke skips integrity criteria | CONFIRMED | FIND-TASK-001-7 |

## Validated findings and current disposition

Findings 1–4, 6 and 7 remain active. Finding 5 is preserved below only to
explain the revision 1 review and subsequent withdrawal; do not implement its
original correction.

### FIND-TASK-001-1 — INCORRECT: listing builds on an unprepared checkout

Authority: REQ-001 and scenario 1 require category listing without building;
the linked design explicitly says listing performs no build or execution and
uses the same authoritative inventory as selection.

Location: `scripts/benchmarks/l1.sh:21–26`, called by the three category tasks
in `mise.toml:573–583`.

Starting state/trigger: `target/profiling/release/capacity` is absent on a fresh
checkout, and a maintainer runs a category task with `-- --list-cases`.
Expected: selectors and routine/targeted status without compiling or launching
case work. Actual: the fallback executes `cargo run --locked --quiet -p
wyrd-testing --bin capacity`, compiling the benchmark driver/dependency graph
before it can list. Direct-dispatch listing tests do not traverse this shell
branch. The implementation task acknowledges this limitation, but that record
does not revise the approved requirement.

Consequence: discovering selectors requires a working Rust build and can pay
the complete driver build cost or fail before listing. This is a structural
source-proven defect; no failing build is needed to establish that `cargo run`
builds missing artifacts.

Smallest correction: expose the existing authoritative registry to the listing
entry point without compiling the driver, retaining one owner for inventory,
routine membership and selection. Test the actual entry point with no prepared
binary and prove no build command or case launch occurs. Do not maintain a
second hand-written inventory.

### FIND-TASK-001-2 — INCORRECT: phase supervision does not follow actual teardown

Authority: REQ-003 separately grants setup 240 s, warmup 30 s, measurement 90 s,
drain/capture 150 s, teardown 60 s, persistence/report 20 s. Its linked lifecycle
design assigns process stops and PostgreSQL removal to teardown and requires
shared monotonic allowances. REQ-002 requires retaining actual cleanup outcomes.

Locations: `capacity/sweep.rs:293–322`; `experiment.rs:2210–2215,2672–2678`;
`capacity/verification/run.rs:1471–1489`, `verification/driver.rs:372–383`;
PostgreSQL wrapper EXIT cleanup at `scripts/postgres/with-test-postgres.sh:24–32`.

Two supported triggers demonstrate one phase ownership error:

1. A case consumes 240 s setup, 30 s warmup, 90 s measurement, 130 s drain,
   and 10 s process teardown, writing its engine record at 500 s. PostgreSQL
   disposal then takes 20 s. Every phase and the 590 s total permit this
   lifecycle, but the supervisor races the entire wrapper against
   `total - persist - teardown = 510 s` and interrupts it halfway through
   allowed disposal. The expected wrapper exit at 520 s becomes INVALID.
2. A short SDK case has little pending drain, then its SDK child takes 70 s
   to exit after stdin closes. `Driver::stop` waits in workload `finish`.
   The shared runner permits `drain_left + teardown_left`, and
   `Lifecycle::finish` charges that elapsed time to unused drain first.
   A process teardown exceeding 60 s can therefore be recorded as drain and
   remain VALID after quick PostgreSQL cleanup.

The existing clock test checks the accounting implementation, not these
actual operation boundaries; the connected short healthy sweep cannot prove
the phase edge cases. Both paths were independently followed through the
child runner, SDK stop, wrapper and supervisor. The first falsely rejects an
admissible case; the second can accept an overrun and records misleading phase
times.

Smallest correction: align existing runner/workload/supervisor phase ownership
with actual drain versus process/resource teardown, share the appropriate
remaining allowance across their occurrences, and allow admitted wrapper
teardown to finish. Keep the existing accepted budgets and server stop policy.
Controlled tests must distinguish slow drain from slow process stop and cover
allowed cleanup across the current 510 s cutoff. No new global lifecycle
controller or changed production behavior is required.

### FIND-TASK-001-3 — INCORRECT: cleanup can hang or confirm too late and admit work

Authority: REQ-002 stops later launches when cleanup cannot be confirmed within
its allowance. REQ-003 records phase overruns and external-service stop failures;
the linked design requires bounded blocking subprocesses and shared deadlines.

Locations: `capacity/sweep.rs:249–256,353–362,451–568`, with continuation at
`:173–178`.

Starting state/trigger: Docker stalls during resource listing or fallback
Compose removal after an attempted case. `compose_resources` awaits each
`docker ... .output()` without a timeout; `confirm` does the same for fallback
`docker compose down`. Its deadline check occurs only after these awaits.
Expected: return an invalid attempt with unconfirmed cleanup and NOT RUN later
rows at allowance exhaustion. Actual: a stuck Docker call can prevent any
outcome or persisted sweep indefinitely. If a late call eventually returns no
resources, `confirm` returns an empty list even beyond its deadline, and
`attempt` sets `cleanup.confirmed = true`; supervision can start the next case.
An engine lifecycle overrun may mark the current row invalid, but continuation
still depends only on that true cleanup flag.

The termination path independently renews the full teardown grace for TERM,
then KILL, then confirmation when there is no engine record; the elapsed stop
waits are not charged to that final confirmation allowance. Scheduler tests
inject a false flag; the real negative harness plants preexisting state. Neither
exercises a stuck or late actual cleanup check.

Consequence: the workflow can fail to return failure evidence or launch a case
after timely cleanup was unproven, violating the explicit isolation/recovery
boundary.

Smallest correction: use one remaining monotonic cleanup allowance across stop,
removal and confirmation, bound the owned external operations, and retain
timeout/unknown cleanup evidence. Late absence must not count as confirmation
within the allowance. Test a stuck check, late empty result and missing-engine
termination path through the existing supervisor, asserting later cases do not
start. Do not infer cleanup success from timeout.

### FIND-TASK-001-4 — MISSING: the measured persistence phase is discarded

Authority: REQ-005 requires saved phase timings and standalone report inputs;
REQ-003 assigns the final persist/report allowance.

Locations: `experiment.rs:2140–2142,2162`; `capacity/sweep.rs:163–165,206–213`;
`capacity/summary.rs:753–760,863–897`.

Starting state/trigger: keep a normal case and reopen its saved full report.
Expected: the final phase has honest measured timing in saved evidence.
Actual: `Lifecycle.persist_seconds` starts `None` and has no production
assignment. `keep_case` measures write duration and returns it, but `entry`
only tests it for overrun and discards successful elapsed time. Neither saved
case nor sweep retains it; the full case renderer prints the final phase as
`unavailable`. Independent source inspection confirms this is an implementation
omission, rather than merely absent execution proof.

Consequence: saved evidence cannot reproduce the complete lifecycle timing or
establish the final phase duration. Existing late-save invalidation is present
and must remain.

Smallest correction: retain the measured final-phase evidence at the existing
case/sweep finalization boundary and recover it through the full report. Record
what was actually measured rather than inventing a self-inclusive save duration.
Extend round-trip assertions for available persistence timing, late save with a
reference, and unsaved record without a reference.

### FIND-TASK-001-5 — WITHDRAWN: Bifrost write sanity targets disappear

Current disposition: withdrawn under approved spec revision 2. All previous
performance targets are void for refactored L1 cases; restoring the thresholds
below would contradict current authority. The original revision 1 validation
follows for provenance only. No remediation or replacement targets arise from
this finding.

Authority: REQ-004 preserves existing thresholds/operators and judges the same
unrounded aggregates displayed; REQ-006 reports target results.

Locations: `capacity/sweep.rs:590–595`; existing criteria in
`capacity/query/mod.rs:1077–1103`; replacement derived summary at
`capacity/summary.rs:569–681`.

Starting state/trigger: a Bifrost routine sweep, or an explicit complete c1/c64
write pair, yields valid measurements whose native peak is below 200,000 rows/s
or standard OTLP spans/logs peak is below 30 input MiB/server CPU-second.
Expected: the existing peak sanity-floor target is reported as missed, without
making a valid measurement invalid. Actual: the owner declares these criteria
against derived `*-peak` steps. `narrow` retains only the single measured step
and criteria directly naming it, dropping all three floors. The replacement
summary derives peak rates but never evaluates these criteria. Write rows have
target `none`; `main.rs:765` expressly asserts that output for an OTLP write.

Consequence: standard category execution silently loses established performance
judgments even with complete inputs. The arithmetic for other measured targets
does not close this derived consumer gap.

Smallest correction: carry existing derived criteria to the sweep boundary and
evaluate them from saved case aggregates with their declared peak winner,
metric, operator and threshold. Do not invent per-concurrency targets or silently
run omitted inputs. Test complete met/missed pairs, omitted inputs, saved report
round-trip, and profiling with unchanged predicates; misses must remain distinct
from invalidity.

### FIND-TASK-001-6 — MISSING: serialization loses the full server evidence

Authority: REQ-005 preserves detailed evidence and report inputs independently
of temporary artifacts; REQ-006/AC-004 require the full saved case report.
`benchmarks/AGENTS.md` also requires every recorded server series in the report.

Locations: `experiment.rs:1599–1603,4144–4154`;
`capacity/sweep.rs:333–337,699–704`; `capacity/summary.rs:830–854`.

Starting state/trigger: a case records server timing, scan, counter and gauge
snapshots, then a maintainer reads its persisted case report, including after
temporary artifacts are removed. Expected: full retained series and window
detail regenerate from saved evidence. Actual: `Repetition.timings` has
`#[serde(skip)]`. The child writes JSON; the supervisor reads it with
`Record::parse`, losing all snapshots before saving the enclosing `CaseRecord`.
The new case reader deserializes directly and bypasses historical
`Record::read_with_timings`. Existing `render_server` iterates the now-empty
snapshot collection, omitting the series silently. Scalar readings and pooled
judged histograms do not preserve all counter/gauge/scan inputs.

The new round-trip test builds scripted scalar evidence and compares the
decoded engine renderer with itself; it does not supply these snapshots.
Thus it cannot establish the missing full-report promise.

Consequence: full diagnostics vanish at a live subprocess boundary and become
unrecoverable when temporary artifacts are deleted.

Smallest correction: retain full window evidence within the new durable
case-protocol boundary and hydrate the established renderer from it, preserving
historical protocol interpretation. Round-trip actual timing, counter/gauge and
scan evidence, delete temporary artifacts and assert window labels and values
remain in the report. Do not introduce another recorder or reporting system.

### FIND-TASK-001-7 — INCORRECT: case smoke bypasses detected correctness failures

Authority: REQ-007 requires smoke workflow and evidence-integrity checking;
REQ-003 preserves correctness/custody checks and makes integrity failures invalid.

Locations: `experiment.rs:3316–3322`, reached via
`capacity/sweep.rs:597–602`; query detection/reconciliation at
`capacity/query/run.rs:137–138,800–803`, readings at `query/isolated.rs:613–631`.

Starting state/trigger: a short Bifrost read case has healthy timing evidence
and some correct answers, but at least one successful server response contains
wrong rows. The client detects `WRONG_RESULT`; the adapter retains nonzero
`errors` and `wrong_results`. Wrong-result responses count as answered in the
server cohort, so reconciliation can remain consistent. The declaration's
`no-errors`/`exact-answers` criteria are the correctness guard.

Expected: the case and saved sweep show INVALID for failed correctness while
performance qualification remains inapplicable at smoke size. Actual:
`narrow` uses `Mode::Smoke`, and `judge_criterion` returns inapplicable for every
criterion before evaluating `Over::Every`. Present nonzero errors therefore
create no invalid criterion reason; otherwise healthy evidence can be VALID.
The existing legacy smoke test at `experiment.rs:6130–6148` explicitly asserts
errors `10` are inapplicable. The new target-negative test uses Full mode and
the connected smoke uses healthy workloads, so neither closes this branch.

Consequence: the required harness lane can pass detected wrong answers or
analogous custody failures, contrary to its integrity purpose.

Smallest correction: at the shared judgment owner evaluate existing
case-protocol smoke correctness/custody criteria per window while retaining
inapplicable performance targets/floors at smoke size. Preserve legacy reader
interpretation and avoid changing unrelated L2 policy. Add a focused numeric
correctness failure plus connected sweep proof that this becomes a saved INVALID
outcome. Reuse the current criteria and record path.

## Reconciled coverage handoff

The primary report's matrix supplies detailed assertion references. Reconciled
statuses below concern assigned existing requirements, not new obligations.

| Existing requirement / acceptance and task scenario | Reconciled result and proof basis |
|---|---|
| REQ-001, AC-001 / 1 | FAIL: finding 1. Exact routine/targeted inventory, selection order, qualified collisions and declaration independence have source/assertion proof in `cases.rs:428–647` and `main.rs:959–1029`; actual shell listing violates the no-build boundary. |
| REQ-002, AC-002 / 3 | FAIL: findings 2–3. Fresh process/project/storage ownership and sequential healthy cleanup have connected assertions in `pg_tests.rs:720–975` and recorded C3 execution; timely cleanup failure behavior is not established by injected flags/preexisting-volume refusal. |
| REQ-003, AC-003 / 2 | FAIL: findings 2–4 and 7. Interval stop/late populations and sequential window drain are supported by existing capacity/experiment assertions; actual final drain/stop phase ownership and persistence timing fail. |
| REQ-004, AC-003 / 2 | Original revision 1 FAIL under finding 5 withdrawn by revision 2. Aggregate/pooling and missing-input assertions remain usable; remediation checks no inherited performance targets. No revised execution result is claimed. |
| REQ-005, AC-004 / 4 | FAIL: findings 4 and 6. Case/sweep identity/order/outcome/reference structures and persistence error handling exist, but phase duration and full snapshot evidence are missing. Protocol parsing and historical comparison/calibration guards are supported by source and round-trip assertions. |
| REQ-006, AC-001/004 / 1,4 | FAIL: finding 6 remains; finding 5 withdrawn by revision 2. Saved table order/units/headline mappings, invalid/unstarted blank cells and profile notes/diagnostic labels retain their original proof; full case detail is incomplete. |
| REQ-007, AC-002 / 3 | FAIL: finding 7. The harness genuinely drives multiple fresh cases through the normal sweep/release-server/PostgreSQL path; its reuse of legacy smoke judgment skips detected numeric integrity failures. |
| AC-005 / 5 | PASS for required hold/disclosure. Full-size feasibility remains explicitly UNVERIFIED under approved revision 1; this is not a current execution requirement or independent blocker. |

Specialist coverage is sufficient: primary covers selection, CLI/report/protocol,
repository rules and owner reuse; system covers subprocess lifecycle/recovery;
invariant covers shared window/aggregate/criterion behavior. The changed recorder
is pooling support, and no production auth, tenant SQL capability, domain
durability or wire/SDK behavior is changed. Separate repo, maintainer or security
specialists would repeat already assigned coverage without a distinct material
boundary. Existing workload owners remain the authority; no optional structure
or style remediation is retained.

## Evidence and process limits

The implementation task records C1 (181 passed, 3 skipped), C2 GREEN, C3
(one connected smoke passed, 152 s), formatting/lints GREEN and clean diff.
These execution claims were inspected alongside assertions, not independently
rerun by this validator. The invariant reviewer's focused five-test nextest
attempt was cleanly interrupted with SIGINT (exit 130) while compiling the
`wyrd-server` dependency, before tests started, at the orchestrator's instruction
to bound the cold review build. Its exact command is recorded in
`task-review-invariants.md`. This is neither a passing result nor a test failure;
no new defect is inferred from it. No Cargo, Docker or full-size performance
command ran in this validation context.

Most new named-test exact selectors are absent from the implementation evidence;
package-wide C1/C2 claims do not establish exact-selector execution history.
This limits process proof, without changing the source-proven defects above.

TDD compliance is separately **VIOLATED** for task scenarios 1–4, as explicitly
recorded: production preceded tests and there is no expected behavioral RED.
Later execution cannot repair that history; no historical RED or production
churn is required as remediation. Behavioral acceptance remains FIX_REQUIRED
for settled approved behavior. No spec revision or material clarification is
needed for the retained corrections.
