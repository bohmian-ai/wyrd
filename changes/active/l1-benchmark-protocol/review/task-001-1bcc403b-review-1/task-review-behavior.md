# TASK-001 behavioral review

Subject: `2418dc7848375c00ac6a991cafca88f722bcfdfe...1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
Authority: approved `SPEC-l1-benchmark-protocol` revision 1; original
`tasks/TASK-001-standardize-l1-benchmark-sweeps.md`. HEAD is
`6e158cd39407a5c25e5edad163633765d99fdad9`; its sole change beyond the
candidate is an unrelated fence-lifecycle task record. Reviewed source paths
match the candidate. This is initial discovery, not independent validation.

Read the actual cumulative diff, approved spec, design notes and case audit,
root and benchmark AGENTS, spec-driven development and implementation execution,
TESTING, maintainer style, Rust and agent-surface guidance, and Bifrost's
durability/visibility authority. CodeGraph was the first code-navigation tool.
No reviewed source was changed. No runtime tests were executed by this reviewer.

## Navigation and ownership

The category mise tasks call `scripts/benchmarks/l1.sh`, then `main::dispatch`,
`cases::resolve` and `Sweep::supervise`. `Case::plan_arguments` selects the
existing query, Verification or Gateway declaration. `Sweep::attempt` owns the
case subprocess and PostgreSQL cleanup; `experiment::run` owns measurement,
judgment and the nested engine record. `summary` owns the case/sweep protocol
and rendering; `levels::report` dispatches new and historical readers.
`wyrd-benchmark::Distribution` retains pooled histogram inputs. Inline CLI,
runner and recorder tests cover pure behavior; `tests::pg_tests` exercises
the actual release-server sweep. The lifecycle and timing reviewers own
their specialist conclusions; this primary review retains requirement coverage,
selection, consumers, reporting, compatibility and repository rules.

## Acceptance matrix

Statuses below include source-level failures found here; specialist-owned
behavior awaits its reports and independent validation. C1/C2/C3 refer to the
original task's recorded execution results, not review reruns.

| Existing requirement / scenario | Implementation and inspected assertion evidence | Discovery result |
|---|---|---|
| REQ-001, AC-001 / 1: routine and targeted selection | `cases.rs:55–143,231–305,339–391`; exact 5/5/18 memberships, complete 14/8/43 inventory, collision handling and explicit order asserted at `:428,483,526,559`; all declarations narrow at `main.rs:998`; C1 recorded GREEN | FAIL: B-1 concerns the actual mise listing path; registry behavior otherwise matches the audit |
| REQ-001, REQ-002, AC-001 / 1: declaration independence and profiles | `Case::plan_arguments` does not read other selected cases; `cases.rs:597,621` covers profile modes and selection-independent planning; `main.rs:998` checks every registered declaration | PASS for structural selection behavior; lifecycle proof is specialist-owned |
| REQ-002, AC-002 / 3: fresh state and cleanup | `Sweep::attempt/finished`, existing workload `prepare`, release-server and Postgres owners; real-client test `pg_tests.rs:720` checks distinct projects, storage roots and process identities, cleanup and stopping later launches; C3 records 1 passed / 152s | UNRESOLVED pending lifecycle review; source shows connected producer-to-consumer harness rather than disconnected mocked resources |
| REQ-003, AC-003 / 2: fixed clock and stop populations | `Level::case_timing`, `Declaration::step_timing` explicitly ignores old `window_seconds` overrides when `Timing::phases` is present; runner lifecycle and capacity window; arithmetic/clock assertions at `experiment.rs:7190,7337,7473`, existing late/stop assertions at `:6228` and `capacity.rs:1386`; C1/C2 recorded GREEN | UNRESOLVED pending timing/lifecycle reports |
| REQ-004, AC-003 / 2: sums, pooled bounds, targets and validity | `Repetition::aggregate`, `judge_criterion`, pooled `Reading` evidence; `summary::cells` calculates displayed throughput/CPU from aggregate work/seconds/CPU; unequal-window and target/invalidity assertions at `experiment.rs:7190,7473` | FAIL: B-2 drops the existing Bifrost write targets; other arithmetic is specialist-owned |
| REQ-005 / 4: persisted case and sweep records | `CaseRecord`, `SweepRecord`, `Sweep::keep_case/keep_sweep`, `summary::entry`; ordered outcomes and missing/late persistence assertions at `sweep.rs:915`; C1/C3 recorded GREEN | FAIL: B-3 loses full server evidence at the subprocess serialization boundary; persistence-phase detail is lifecycle-review scope |
| REQ-005, AC-004 / 4: protocol compatibility | New records use explicit protocol/version and cannot parse as schema-3 records; `levels::compare` uses `Record::read`; `Record::compare` and `query::calibrate` reject lifecycle-bearing records; historical round-trip checks at `main.rs:815–860` and `experiment.rs:7567` | PASS from source and recorded C1; no silent comparison/calibration mixing found |
| REQ-006, AC-004 / 4: reproducible category table | `render_sweep` uses saved entry order and adjacent case records; explicit headline mappings cover Verification, Gateway, OTLP/native writes, point/trace reads, scan p95 and UI page p99. Invalid/unstarted performance cells are withheld. `main.rs:655` asserts table order/units, failure rows, missing derived inputs and artifact-independent regeneration; C1/C3 recorded GREEN | FAIL for lost write-target judgments under B-2; otherwise table behavior inspected |
| REQ-006, AC-001 / 4: profiles and full case report | `summary::render_case`, `record.render`; `main.rs:869` asserts diagnostic labeling, unchanged target predicate and capture notes; `:655` checks case report shape | FAIL under B-3 for full evidence. Profile test injects capture failure notes and proves persistence/rendering, not a live profiler run |
| REQ-001, REQ-006 / 1,4: mise and contributor migration | New category tasks and listing/selection/profile descriptions; benchmark README and AGENTS prescribe the emitted table verbatim. TESTING maps the new harness lane; no active old L1 task caller found in scripts/mise | FAIL only for the build-free listing promise under B-1; documentation otherwise follows the new commands |
| REQ-007, AC-002 / 3: short harness sweep | `test:benchmark:l1` runs the connected release-server test; it drives native-write then memtable lookup through the same supervisor/case path, checks records and report after artifact removal, and tests failed/unstarted cases | UNRESOLVED: timing reviewer owns the identified smoke integrity bypass |
| AC-005 / 5: full-size feasibility | Original task and approved spec explicitly retain the execution hold; no full-size run claimed | PASS for required disclosure; full-size feasibility remains unverified by design |

## Proposed blocking findings

### B-1 — INCORRECT: a build-free listing starts a Cargo build

- Requirement: REQ-001; `-- --list-cases` lists selectors without building or
  running a benchmark. The task's scenario 1 explicitly requires listing to
  avoid build preparation. README `:47` repeats that it builds nothing.
- Location: `scripts/benchmarks/l1.sh:21–26`, reached by all three category
  mise tasks at `mise.toml:573–583`.
- Starting condition and trigger: a supported fresh checkout, or a host that
  has not prepared `target/profiling/release/capacity`; invoke
  `mise run benchmark:l1:bifrost -- --list-cases`.
- Expected: print selectors/status and return without compiling benchmark
  code or starting case work.
- Actual and evidence: the fallback executes `cargo run --locked --quiet -p
  wyrd-testing --bin capacity`, which builds the debug benchmark driver and
  its dependency graph before listing. Existing CLI tests call `dispatch`
  directly and therefore bypass this shell path. The task itself records this
  limitation; it is not a revision to the approved requirement.
- Consequence: discovery depends on a functioning Rust build and can incur the
  large benchmark dependency build or fail before showing selectors.
- Smallest correction: make the same authoritative case inventory available
  to the category listing path without compiling the driver; retain one owner
  for listing, selection validation and routine membership. Add a focused
  entry-point check with no prepared driver that proves no build command or
  case launch is invoked. Do not add another independently maintained registry.

### B-2 — MISSING: narrowing removes all existing Bifrost write targets

- Requirements: REQ-004 preserves existing target thresholds/operators and
  judges recorded aggregates; REQ-006 displays the target result.
- Location: `sweep.rs:579–595`, in particular the criterion filter at `:592`,
  and `summary.rs:569–681` (replacement derived peak reader).
- Starting condition and trigger: run the full routine Bifrost sweep, or
  explicitly select both c1/c64 write inputs for native, OTLP spans or logs.
  All selected cases can be valid while the native peak misses 200k rows/s or
  the standard spans/logs peak misses 30 input MiB/server CPU-second.
- Expected: preserve the declared peak sanity-floor judgments, using the
  existing thresholds/operators and unrounded aggregate evidence. A miss is a
  performance result and does not invalidate measurement.
- Actual and evidence: `query/mod.rs:1077–1103` declares the three targets only
  against `native-write-peak`, `otlp-spans-peak`, and `otlp-logs-peak`.
  `narrow` retains only the selected measured step and criteria naming that
  measured ID, dropping all three target criteria. The summary computes peak
  row rates but never evaluates the dropped criteria. Its Target cells are
  `none`; `main.rs:765` explicitly expects that for an OTLP write. Historical
  declaration tests at `main.rs:1344–1395` still prove the old targets exist,
  but do not exercise this new category consumer.
- Consequence: the standard command stops reporting the existing write
  sanity targets, even when it has every required input and performance is
  below the established floor.
- Smallest correction: retain and evaluate these existing derived target
  declarations over their selected case aggregates at the new sweep/derived
  reporting boundary. Reuse the declared winner/metric, threshold and operator;
  do not invent per-concurrency thresholds. Missing inputs must remain missing
  and must not trigger hidden runs. A saved complete write sweep should report
  both met and missed floor cases; record round-trip and profiling must preserve
  those judgments, with valid target misses remaining VALID.

### B-3 — MISSING: the new case path loses full server evidence

- Requirements/authority: REQ-005 keeps detailed evidence and inputs required
  to regenerate reports without temporary artifacts; REQ-006 renders the full
  case report; AC-004. `benchmarks/AGENTS.md` requires every recorded server
  series to appear in `capacity report`.
- Location: `sweep.rs:333–337` and `:699–704` (engine-record write/read),
  `experiment.rs:1599–1603`, and `summary.rs:830–854`.
- Starting condition and trigger: complete a normal case that captures server
  timing/scan/counter/gauge series, then open its saved case record through
  `capacity report`; remove temporary artifacts as AC-004 permits.
- Expected: the case record's full report retains all recorded server series
  and associated window detail without depending on temporary artifacts.
- Actual and evidence: `Repetition.timings` is `#[serde(skip)]`; the case child
  serializes the engine record, and the supervisor uses `Record::parse`, so
  the full `TimingEvidence` snapshots disappear before `CaseRecord` is saved.
  `summary::report/read_case` deserialize that record directly. The legacy
  reader's `Record::read_with_timings` is bypassed. `Record::render_server` at
  `experiment.rs:4144–4154` iterates those now-empty snapshots, silently
  dropping all server-series sections. Derived scalar readings/histogram pools
  do not contain the complete counter, gauge and series evidence. The report
  test's script creates no such snapshots and compares the decoded record's
  renderer to itself, so its passing assertion does not cover this loss.
- Consequence: the new protocol cannot supply the established detailed report,
  and deleting temporary artifacts permanently removes the missing evidence.
- Smallest correction: preserve the captured full window evidence at the new
  case-protocol boundary and have its reader hydrate the existing full report
  from that durable evidence. Keep historical schema-3 interpretation unchanged.
  Add a round-trip report check containing a real recorded server timing,
  counter/gauge and scan series; delete temporary artifacts and assert the
  full evidence is still rendered with its window labels and values.

## Proof, process and scope limits

The original task records C1 (181 passed, 3 skipped), C2 GREEN, C3 (1 passed,
152s), format/lints GREEN and `git diff --check` GREEN. Those are implementation
claims inspected alongside assertions, not fresh execution by this reviewer.
Named-test focused commands are not individually recorded for most new tests;
C1/C2 are package-wide commands. This does not erase their broad GREEN claim
but does not establish the repository's exact-selector execution requirement.
The parent is coordinating any focused reruns; no Cargo process was launched
here. The source is sufficient to establish B-1/B-2/B-3 structural paths.

TDD compliance: **VIOLATED**, as explicitly recorded for scenarios 1–4.
Production preceded the tests; there is no expected-missing-behavior RED.
Later execution can establish behavior but cannot repair that history. Do not
undo correct code or manufacture historical RED as remediation.

The new registry, supervisor and record renderer have concrete responsibilities
and reuse the existing workload declarations, release-server/Postgres owners,
recorder and engine judgment. No speculative abstraction/style finding is
retained. Full-size feasibility and real profiler capture were not rerun.

Primary recommendation: **FIX_REQUIRED**, subject to independent validation
and the required lifecycle/timing specialist reports.
