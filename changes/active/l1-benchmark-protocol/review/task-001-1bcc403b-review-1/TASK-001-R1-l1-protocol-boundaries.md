---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-l1-benchmark-protocol
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, AC-001, AC-002, AC-003, AC-004]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-6, FIND-TASK-001-7]
---

# Restore L1 protocol boundaries and saved evidence

## Subject and outcome

Implement the settled behavior of [approved spec revision 2](../../spec.md),
using [original TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md)
as historical implementation evidence.
Reviewed base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`; candidate:
`1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
[Validated findings](findings-validation.md) define the diagnosed defects;
[verdict](verdict.md) records coverage and execution limits.

After correction, maintainers can discover cases without a build, run them
under the agreed lifecycle allowances, trust smoke integrity, and regenerate
full case evidence from saved records.
This remains one cohesive correction to the existing category sweep path.

The maintainer voided every previous performance target for these refactored
cases. FIND-TASK-001-5 is withdrawn by that approved scope change and creates
no remediation work; the six other finding IDs remain stable. Do not restore
old thresholds or introduce replacement targets. Correctness, custody,
sample-floor and lifecycle checks remain required. Ensure new L1 declarations,
tests and guidance do not treat legacy performance targets as current;
historical records retain their original interpretation.

## Diagnosed gaps and correction owners

- **FIND-TASK-001-1 / REQ-001:** category listing without a prepared release
  driver executes `cargo run` at `scripts/benchmarks/l1.sh:21–26`. Keep one
  authoritative inventory shared with selection; make the actual listing
  entry point build-free. Preserve exact routine/targeted membership,
  selector qualification and order. Do not add a second hand-maintained list.
- **FIND-TASK-001-2 / REQ-002/003:** `Sweep::attempt` applies a 510 s cutoff
  to a wrapper that includes allowed teardown, while `Lifecycle::finish`
  charges actual process stops against unused drain first. Align existing
  runner, workload, release-server and wrapper ownership with actual drain
  versus process/resource teardown. Preserve every approved phase allowance
  and production stop behavior. Both premature rejection and accepted slow
  teardown require closure.
- **FIND-TASK-001-3 / REQ-002/003:** Docker awaits in `compose_resources` and
  `confirm` are unbounded; late empty checks can set cleanup confirmed; stop
  paths renew allowances. At the existing sweep owner, share one remaining
  monotonic cleanup allowance across owned stop/removal/confirmation work.
  Bound external operations, retain timeout/unknown outcomes, and stop later
  launches when timely cleanup is unproven.
- **FIND-TASK-001-4 / REQ-003/005:** `keep_case` measures persistence duration,
  but `entry` discards it and `Lifecycle.persist_seconds` stays unavailable.
  Retain honest final-phase timing at existing record finalization and make
  the full reader recover it. Preserve late-save invalidation with a saved
  reference and unsaved outcomes without one. Do not fabricate a self-inclusive
  write duration.
- **FIND-TASK-001-6 / REQ-005/006, AC-004:** engine JSON drops
  `Repetition.timings` before the supervisor saves its case. Preserve full
  window timing/scan/counter/gauge snapshots within the new case protocol and
  hydrate the existing full renderer from durable evidence. Preserve original
  historical protocol interpretation; reuse the recorder and report owners.
- **FIND-TASK-001-7 / REQ-003/007:** case smoke uses legacy `Mode::Smoke`
  judgment that skips detected numeric correctness/custody failures. At the
  shared judgment owner, evaluate the existing per-window integrity criteria
  for case-protocol smoke while performance qualification stays inapplicable.
  Preserve unrelated L2 policy and historical readers.

## Observable closure scenarios

Follow bounded implementation execution and TESTING.md. Confirm actual test
names/selectors, record exact focused commands before RED, then record the
expected failure and GREEN result in these entries. No new test names are
prescribed. Use controlled clocks and owned local subprocesses for deadline
tests, and the real existing harness for connected proof.

1. **Build-free category discovery** — FIND-TASK-001-1; REQ-001/AC-001.
   With the prepared driver absent, invoke each actual category listing entry
   point and assert the authoritative selectors/statuses. Assert no Cargo
   build, benchmark preparation or case launch occurs. Exercise listing via
   the shell/mise seam, not solely direct Rust dispatch. Keep selection parity
   assertions in the existing CLI test owner. RED is an attempted build or
   mismatched inventory.
2. **Actual phase boundaries** — FIND-TASK-001-2; REQ-002/003/AC-003.
   Through existing runner/supervisor lifecycle owners, advance controlled
   time so setup/warmup/measurement/drain consume 490 s, process teardown 10 s
   and PostgreSQL cleanup 20 s. The allowed 520 s lifecycle must complete
   without the old 510 s interruption. Separately make an SDK/process stop
   exceed teardown's 60 s despite unused drain; assert invalidity, an accurate
   teardown overrun and bounded cleanup. Slow real drain must still consume
   drain allowance. RED is premature cutoff or a slow stop accepted as drain.
3. **Bounded cleanup failure** — FIND-TASK-001-3; REQ-002/003/AC-002.
   Drive owned external cleanup operations that never reply, reply empty
   after allowance exhaustion, and require termination without an engine
   record. Assert one shared allowance, invalid/unconfirmed cleanup evidence,
   a persisted sweep and NOT RUN next-case rows with no next launch. Use the
   sweep supervisor's actual cleanup path; injecting a precomputed false
   flag alone is insufficient. RED is hanging, renewed grace or late success
   permitting continuation.
4. **Complete saved case evidence** — FIND-TASK-001-4/-6; REQ-003/005/006,
   AC-004. Send recorded timing, scan, counter and gauge snapshots through
   child serialization and supervisor persistence. Remove temporary artifacts;
   read with `capacity report` and assert original window labels/values plus
   honest available persistence timing. Preserve identical regenerated sweep
   output and full case detail. Exercise unsaved and late-saved outcomes with
   their required invalid status/references. RED is lost series, unavailable
   final-phase duration or incorrect persistence outcome. Extend the existing
   saved-record tests and focused connected harness rather than add a second
   reporting system.
5. **No inherited performance targets** — revised REQ-004/006, AC-003/004;
   maintainer-approved scope change, not an independently discovered defect.
   Through the existing new-L1 declaration, judgment and saved-report path,
   exercise measurements that would miss historical thresholds and verify
   those thresholds produce no current target judgment in ordinary or profiled
   runs. Keep aggregate readings and missing-input reporting intact without
   launching omitted cases. Preserve correctness, custody, sample-floor and
   lifecycle failure judgments, and historical record interpretation. Reuse
   existing CLI/report and runner test owners; RED is an inherited target
   still being applied. Already-correct paths need verification only.
6. **Integrity-preserving short sweep** — FIND-TASK-001-7; REQ-003/007,
   AC-002. In the shared judge's focused tests, give case-protocol smoke
   present nonzero correctness/custody evidence alongside otherwise healthy
   measurements; assert INVALID with the existing integrity reason and
   inapplicable performance qualification. In the existing connected smoke
   harness, demonstrate that detected bad evidence produces a saved INVALID
   sweep outcome, with normal cleanup/continuation preserved. Keep the existing
   two sequential real-client/server cases and fresh-resource assertions.
   RED is a detected integrity failure reported VALID.

For focused Rust assertions confirm the actual names, then use
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=<confirmed-test-name>)'`
for CLI/sweep/report tests and
`mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=<confirmed-test-name>)'`
for runner/judgment tests, with default features and repository setup where
required. Exercise the connected path with `mise run test:benchmark:l1`
and record its actual focused selector. Run Cargo-backed checks sequentially.
Find owning tasks with `mise tasks ls --hidden`; run `mise run fmt:check`,
`mise run lints`, the affected fast owner checks and `git diff --check` after
focused closure. Update existing benchmark guidance only where the corrected
entry point or evidence contract requires it.

## Preserved behavior and exclusions

Keep existing case declarations, routine order, fresh resources, workload
clients/providers, durable-ack boundary, correctness and evidence-integrity checks,
historical interpretation and comparison/calibration guards. Do not add
production waits, new recorders, a duplicate registry or general lifecycle
framework. Baseline/comparison methodology and L2 policy remain outside scope.
Full-size feasibility stays unverified while approved AC-005 execution is held;
short harness proof does not establish performance feasibility.
Previous performance targets are void for new L1 cases; replacement targets
remain outside this task. This does not rewrite historical records or L2 policy.

Original TDD compliance remains VIOLATED for scenarios 1–4. Record new closure
work test-first; do not undo correct behavior or manufacture historical RED.
No commit or push is authorized by this task-review artifact.
