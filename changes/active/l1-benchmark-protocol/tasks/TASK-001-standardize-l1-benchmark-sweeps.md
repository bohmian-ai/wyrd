---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-l1-benchmark-protocol
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
---

# Standardize L1 benchmark sweeps

## Outcome and approach

Deliver the approved [L1 contract](../spec.md) through all three category
commands. Today a selected case can inherit the experiment's resources, and
reviewers must navigate the full report to find the category results. With
the refactor, a targeted Verification run and the same case in the five-case
routine sweep use the same workload declaration and lifecycle. The saved
sweep reproduces the table printed at completion and points to the detailed
case records.

Extend the existing `capacity` path in `wyrd-testing`. Read the [lifecycle,
case mapping, sweep and proof design](../design/notes.md#proposed-implementation)
and [case audit](../design/case-audit.md). Build preparation is shared by the
sweep. Separately starting and cleaning up each case increases total sweep
time. The case deadline covers the case phases. Full-size timing remains
unverified while performance execution is on hold.

Use the existing workload definitions in `bin/capacity/{query,verification,gateway}`
and the release server and SDK artifacts produced by the benchmark build tasks.
`bin/capacity/{main,levels}.rs` and `experiment.rs` own selection and
execution. Move lifecycle ownership to each case using the existing release
server and PostgreSQL setup and cleanup owners, including
`scripts/postgres/with-test-postgres.sh`. Preserve fixture preparation, actual
client and provider paths, and correctness and custody checks in their
workload owners.

Feed recorded window evidence from the existing benchmark recorder and
accounting into the case record and the current `capacity report` entry.
The supervisor consumes per-case outcomes to persist the sweep. The report
reader consumes the sweep for the category table and a case record for its
full detail. Keep historical readers at their existing protocol boundary.
Migrate active L1 mise callers and benchmark guidance to the category
commands. Baseline selection, comparison methodology, performance-change
qualification, L2 calibration policy, and production workload or durability
changes are outside this task.

## Acceptance scenarios

Follow [bounded implementation execution](../../../implementation-execution.md)
and [testing guidance](../../../../TESTING.md#named-tests). Work through the
scenarios in order. Record the actual assertion locations, exact focused
commands, RED failures, GREEN results, and limitations in their scenario
entries.

1. **Resolve the category's cases** (REQ-001, AC-001). In the `capacity` CLI
   tests, use the Verification, Gateway and Bifrost workload declarations and
   their mise entry points to exercise list, default and explicit selection.
   Check the routine order and exact 5, 5 and 18 case memberships from the
   audit. Check targeted workloads, family-qualified collisions and explicit
   order. Resolve a shared case alone and in reordered selections and verify
   that the declaration and preparation inputs match. Listing and invalid
   selectors must avoid launching work, and listing must avoid build
   preparation. Exercise CPU, heap and both profile options through the same
   selection path, with and without case selection. RED is a missing or
   incorrect selection, a changed case declaration, or an unexpected build or
   case launch. Check the migrated mise help and benchmark guidance against
   the category invocations and selection terminology.

2. **Measure within the case clock** (REQ-003, REQ-004, AC-003). At the shared
   runner, window accounting and record owners, drive the existing timing
   boundaries with a controlled clock and unequal work, CPU and latency
   samples. Check the phase allowances, including the shared drain allowance,
   and force a deadline overrun. A completion after window stop belongs in
   late evidence. The server CPU charged to that window uses its own stop
   boundary. Hold an in-flight request across the stop and verify that the
   next window admits work only after its drain. Verify ratios of sums, pooled histogram bounds and counts, and
   target judgment from the displayed aggregate before rounding. Exercise
   per-window sample and integrity failures and distinguish a valid target
   miss from an invalid case. RED is an overrun accepted as valid, mixed
   evidence populations, a wrong aggregate or a conflated verdict.

3. **Run isolated cases through the harness** (REQ-002, REQ-007, AC-002).
   In the Postgres-backed `capacity` harness, send two sequential cases
   with small fixtures and short windows through the same sweep path used
   by the category commands. Drive their actual workload clients against
   the release server. Assert distinct case-owned server, database, storage
   and client state, verified fixtures, recorded drain and teardown, and
   case and sweep outputs. Include a Bifrost write case to exercise the
   durable acknowledgement boundary and verify that the case captures
   outstanding publication and maintenance work before teardown. Exercise a failed case with confirmed cleanup
   and a failure with cleanup unconfirmed within its allowance. The former
   permits the next case to start, and the latter records later cases as
   unstarted. RED is shared state, missing cleanup evidence, or a case
   launched after cleanup could not be confirmed.

4. **Read what the sweep saved** (REQ-005, REQ-006, AC-004). Take the saved
   sweep and case outputs from the connected harness path and render them
   again through `capacity report` after removing temporary artifacts.
   Assert matching Markdown, fixed output order and units, full case detail,
   and the required invalid and unstarted rows. Exercise each workload’s
   headline mapping from the summary design, including Bifrost’s OTLP
   input units, scan p95, and overall UI page p99. Check that omitted
   inputs to a derived reading are shown as missing and do not start
   unselected cases. Check that profile capture
   references or failure notes and diagnostic labeling survive a profiled
   case while its target predicate stays the same. Exercise both an unsaved
   record and a saved record that misses the persistence deadline. Both
   yield an invalid sweep row with the reason. The saved record keeps its
   reference, and the unsaved reference is unavailable. At the existing
   comparison and L2 calibration boundaries, exercise historical records
   and new protocol records. Historical interpretation must remain intact,
   and new evidence must not be silently consumed as old data. RED is a
   different or incomplete regenerated summary, missing failure or profiling
   evidence, or protocol mixing.

5. **Establish full-size feasibility when execution resumes** (AC-005).
   On supported benchmark hosts, use the category commands to exercise the
   full fixtures and retain the recorded phase times and verdicts. Check
   the end-to-end case envelope and the invalid result for any overrun.
   Record feasibility as unverified until performance execution resumes.
   The controlled-clock and short harness checks establish behavior while
   the hold is in effect.

Use Cargo nextest through mise. Confirm the actual selectors and record each
focused command before RED. After the focused checks, run the affected fast
owner targets and repository formatting and lint checks.

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib --bin capacity
mise run fmt:check
mise run lints
git diff --check
```

For the real-service scenario, use the existing benchmark artifact
preparation and confirm and record the focused Postgres-backed command.
Exercise the [repository Postgres wrapper](../../../../scripts/postgres/with-test-postgres.sh)
through the sweep's case lifecycle so the test can verify that setup and
cleanup happen per case. Full-size category commands are reserved for
scenario 5.
