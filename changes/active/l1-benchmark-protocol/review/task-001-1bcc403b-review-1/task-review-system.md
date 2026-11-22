# TASK-001 lifecycle and failure review

Reviewed base `2418dc7848375c00ac6a991cafca88f722bcfdfe` to candidate
`1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`, against
`SPEC-l1-benchmark-protocol` revision 1. This is discovery evidence for the
independent validator, not a final verdict.

The checkout HEAD is `6e158cd39407a5c25e5edad163633765d99fdad9`; its only
candidate-relative change is the unrelated
`changes/active/bifrost-write-throughput-and-retention/tasks/TASK-001-fence-lifecycle.md`.
All reviewed paths match the candidate. Source was not changed.

## Boundary and authority

Assigned boundary: subprocess/resource isolation, setup and teardown deadlines,
cleanup continuation, persisted lifecycle evidence, and connected smoke proof
for REQ-002, REQ-003, REQ-005, REQ-007 and AC-002. Primary review retains full
requirement coverage; measurement mathematics belongs to the invariant reviewer.

Read the approved spec, original task, lifecycle/proof design, root AGENTS,
benchmark AGENTS, task-review skill, spec-driven-development workflow,
maintainer-style/testing guidance, and Bifrost durability/visibility authority.
CodeGraph was used before source tracing. No nested AGENTS governs the changed
testing crate or PostgreSQL wrapper beyond root rules.

Navigation: `Sweep::supervise` -> `Sweep::attempt` -> repository PostgreSQL
wrapper -> `capacity case` -> `experiment::run` -> workload prepare/measure/
outstanding/finish -> wrapper cleanup -> supervisor confirmation -> case and
sweep persistence -> saved-record reader. `LocalServer` owns server processes,
temporary storage/WAL and operator children; workload owners retain client and
provider state. `Owned` tracks PID plus process start time.

## Source and proof coverage

- Fresh-case mechanism is connected: `sweep.rs:235-302` creates a separate
  Compose project, empty TMPDIR root and process group for each case, and passes
  one exact case into the child. The wrapper sets database URLs for its own
  project; `LocalServer::start` creates a new temporary storage root and performs
  migration/setup. Workloads use those fresh resources and retain them through
  their case. Each child has its own runtime, recorder and workload state.
- `experiment.rs:2510-2548` prepares under a setup deadline and records owned
  server/driver/client process identities before warmup. Existing workload
  fixture preparation and correctness checks remain in their owners. Workload
  `outstanding` implementations use `LocalServer::outstanding` before finish,
  retaining publication/maintenance backlog without redefining durable ACK.
- `sweep.rs:137-197` continues after an invalid attempt with confirmed cleanup,
  and marks remaining cases NOT RUN after unconfirmed cleanup or interruption.
  Unit assertions at `sweep.rs:829-913` establish scheduler behavior with supplied
  outcomes; they do not establish actual timeout handling by `attempt`.
- Connected test `tests::pg_tests::l1_sweeps_isolate_and_record_cases`
  (`capacity/tests/pg_tests.rs:720-975`) invokes the real release driver under
  `Sweep::attempt`, runs write/read cases sequentially, asserts distinct Compose
  projects, storage roots and owned process identities, checks removal, confirms
  lifecycle and outstanding-work keys, and regenerates saved output after
  removing artifacts. Its second sweep asserts early failures can continue and
  a planted preexisting volume stops the next launch. This is credible connected
  coverage of normal cleanup and refusal of unowned state. It does not force an
  actual teardown operation to outlast the allowance.
- The task records `mise run test:benchmark:l1` GREEN (one test, 152 seconds),
  C1/C2 GREEN and format/lints GREEN. I inspected assertions and execution claims;
  I did not rerun Cargo, Docker, or performance commands. No additional runtime
  result is claimed. Full-size execution remains on hold under AC-005.

## Proposed blockers for independent validation

### SYS-1 — INCORRECT: the wrapper is stopped before its allowed teardown can finish

**Requirement:** REQ-003 assigns setup 240 s, warmup 30 s, measurement 90 s,
drain/capture 150 s, teardown 60 s and persistence 20 s, total 590 s. REQ-002
requires retaining and confirming the actual cleanup outcome.

**Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/sweep.rs:293-322`.
The deadline is `total - persist - teardown = 510` seconds and races the
entire wrapper's `child.wait()`. The wrapper runs the case command and only
then performs its EXIT-trap PostgreSQL cleanup
(`scripts/postgres/with-test-postgres.sh:24-32,148-153`). The case command itself
also performs workload finish/server teardown before writing its engine record
(`experiment.rs:2655-2688`, `sweep.rs:692-704`).

**Concrete supported scenario:** a full-size case finishes its engine work and
writes its record at elapsed 500 s, all of its individual phases within their
allowances. Normal PostgreSQL disposal then takes 20 s, within the 60 s teardown
allowance and leaving ample room for persistence. The wrapper should be allowed
to exit at 520 s. At 510 s the supervisor instead sends TERM, records a deadline
failure and can interrupt legitimate cleanup. The result is INVALID even though
the agreed phase and total envelope were available.

**Evidence/consequence:** the timer surrounds wrapper lifetime rather than a
notification of the transition into teardown. The short smoke test cannot
exercise this timing. This narrows the approved full-size lifecycle and may
discard its engine evidence at the boundary.

**Smallest correction boundary:** align `Sweep::attempt`'s deadline with the
actual phase ownership across child and wrapper; allow the accepted teardown
budget while enforcing the individual pre-teardown phases at their existing
owners. Add a controlled lifecycle test that reaches the pre-teardown boundary
and proves allowed wrapper cleanup is not interrupted.

### SYS-2 — INCORRECT: cleanup confirmation can hang or succeed after its allowance

**Requirements:** REQ-002 says cleanup unconfirmed within teardown allowance
stops later launches. REQ-003 requires recording overruns and external-service
stop failures. Root AGENTS requires bounded external operations.

**Locations:** `sweep.rs:249-256,353-362,478-568` and `:451-476`.

**Concrete supported scenario:** Docker becomes unresponsive while the supervisor
queries project resources or attempts fallback removal after a case. Every
`docker ... .output().await` in `compose_resources` and `confirm` has no timeout;
`confirm` checks its deadline only after these awaits and synchronous storage
removal finish. A stuck Docker operation therefore prevents the invalid outcome,
sweep persistence and NOT RUN rows indefinitely. A response that eventually
reports no surviving resources after the deadline returns an empty list;
`attempt` sets `cleanup.confirmed = true` without checking elapsed time, and
`supervise` admits the next case. That is continuation after confirmation was
unavailable within the allowance.

The timeout path also gives TERM a full teardown grace, then KILL another full
grace (`stop`), and when no engine record exists gives `confirm` another full
teardown allowance (`map_or(teardown, ...)`). Those waits are not charged to one
shared teardown deadline. Without an engine record `cleanup.seconds` omits the
stop waits and no lifecycle overrun check can recover them.

**Evidence/consequence:** these are direct supported external-service failures,
explicitly covered by the contract. Existing scheduler unit tests inject an
already-false cleanup flag; the real harness plants preexisting resources before
launch. Neither drives a hung/late external cleanup operation through this code.

**Smallest correction boundary:** reuse `Sweep::attempt`/`confirm`/`stop` with
one monotonic remaining allowance for cleanup operations and confirmation;
bound Docker subprocesses and retain timeout/unknown cleanup evidence. Ensure
late absence cannot become timely confirmation. Add a controlled external-process
failure test for stuck and late resource checks, a missing-engine-record stop,
and the resulting NOT RUN next case. Do not change production cleanup semantics
or add a global outage mechanism.

### SYS-3 — MISSING: successful persistence phase timing is never retained

**Requirements:** REQ-005 requires full phase timings in a saved case record;
REQ-003 assigns persist/report a 20-second allowance. The detailed record must
support later report regeneration without temporary artifacts.

**Locations:** `experiment.rs:2140-2142,2162`; `sweep.rs:163-165,206-213`;
`summary.rs:863-897,753-760`.

**Starting conditions and trigger:** any successful normal case is kept and then
its saved case report is read. `Lifecycle.persist_seconds` starts `None` and has
no assignment anywhere. `keep_case` serializes that record, measures the elapsed
write time and returns it; `summary::entry` only uses the elapsed value to add an
overrun reason, then discards it. No successful elapsed persistence timing lives
in either case or sweep output. The detailed report consequently prints
`persist and report | unavailable | 20` for every normal attempt.

**Expected/actual/consequence:** the accepted lifecycle's final phase should
retain its measured duration. The code measures it but loses it, so the saved
evidence cannot establish that phase's timing or the complete case envelope.
This is a record implementation omission, distinct from absence of full-size
performance qualification. A late save still invalidates its sweep row as
required; that existing behavior should be preserved.

**Smallest correction boundary:** preserve persistence timing at the existing
case/sweep record owner and make the existing full report recover it. Choose a
record finalization boundary that can honestly record what was measured; do not
fabricate a self-inclusive duration. Extend the saved-output test to assert
available final-phase timing and preserve the unsaved/late-save outcomes.

## Process and limits

### Additional lifecycle proposal SYS-4 — INCORRECT: unused drain allowance hides teardown overruns

**Requirement:** REQ-003 separately caps drain/capture at 150 s and teardown
at 60 s; stopping the SDK/server processes is teardown under the approved
lifecycle design. A missed phase deadline makes the attempt invalid.

**Locations:** `experiment.rs:2210-2215,2671-2681`; Verification
`run.rs:1471-1489` and `driver.rs:372-383`.

**Concrete supported scenario:** a targeted Python or TypeScript case has no
substantial pending request drain, then its SDK process takes 70 seconds to
exit cleanly after stdin closes. `Driver::stop` waits for that exit without its
own timeout. The Verification workload's `finish` consists of stopping drivers,
clearing state/provider and stopping the server. The shared runner allows
`drain_left + teardown_left` for that call; `Lifecycle::finish` then charges
all elapsed time to drain first. With unused drain budget, a 70-second process
stop is recorded as drain, not a teardown overrun. A subsequent quick wrapper
cleanup is the only significant recorded teardown, so the case can remain VALID.

**Consequence and correction:** distinct accepted phase bounds are not enforced
at their actual operation boundaries, and persisted phase timings describe the
wrong work. Keep final request/integrity drain and actual process stop timing at
their existing owners, enforcing each allowance separately. Add a controlled
test that distinguishes a slow drain from a slow SDK/server stop; the latter
must be invalid beyond 60 seconds even if drain time remains. This is not a
request to reduce the accepted combined case budget. The primary validator may
consolidate SYS-1 and SYS-4 if one correction at phase ownership closes both.

Behavioral discovery finds the above lifecycle/record gaps. Independent
validation must confirm or reject them before retention. No stable FIND IDs
are assigned by this discovery report.

TDD compliance is separately **VIOLATED**, as explicitly recorded for task
scenarios 1–4. Nothing in this review repairs or rewrites that history.
Full-size feasibility is unverified by design while execution is on hold;
this report does not demand a full-size run during the hold.
