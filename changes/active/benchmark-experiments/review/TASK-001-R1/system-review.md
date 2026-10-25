# System resilience review

Overall result: **FAIL**

Subject: base `0739bbf38af7cc05bbfefb09850490c55432ec2d` to candidate
`aaa152dd4243d57752b73d3a4e9c140bccc7d171`. First review, no carried acceptance.
Scope: changed deployed/process runner, server, collector and helper lifecycle;
interruption, drain, budget and production instrumentation propagation.
Two pre-existing telemetry/export-test repairs are outside this candidate.
No source edits or Cargo commands performed. CodeGraph preceded source search;
all shell commands used mise. Read provided root AGENTS, spec/task, maintainer
style, spec-driven workflow, Bifrost resource/failure and telemetry authority.
Scope inventory establishes no nested AGENTS in changed source directories.

## Coverage and acceptance

| Requirement/path | Source evidence | Verification and result |
|---|---|---|
| Shared deadline/interruption and partial evidence | experiment.rs:1552–1800, levels.rs:191–241 | Runner cancels awaited work and preserves partial records; scripted runner test does not prove actual descendants stop. FAIL for Gateway L2 below. |
| Release server/operator child cleanup | release_server.rs:152–260,328–420,532–618 | Owned pending children, drop kill/reap, abnormal log preservation; static PASS, real failure/recovery proof not run here. |
| Portable server CPU, absent readings | release_server/os.rs and LocalServer CPU/memory methods | sysinfo/plain-child scope, optional readings propagate absence. Static PASS; platform runs not executed. |
| Exact timing collection and budget propagation | candidate wyrd-telemetry/src/lib.rs:323–349; capacity.rs:473–477,528–564,618–899 | FAIL SYS-001; malformed samples cannot satisfy collection counts. |
| Bifrost request task ownership | query/run.rs:240–278; isolated.rs:90–201,231–335; competing.rs task sets/finish | Owned JoinSets cancel with measurement future; static PASS for this boundary. Runtime budget unproved. |
| Verification and L1 Gateway tasks/drain | verification/run.rs:535–603,740–855; gateway/run.rs:492–612; gateway/mod.rs:785–834 | Owned JoinSets, cleanup and integrity checks; no independent defect found in assigned ownership paths. |
| L2 Gateway stop admission and descendants | gateway/capacity.rs:313–349,806–856,963–1090,1125–1133 | FAIL SYS-002. Detached scheduler continues admission during finish. |
| Profiling only diagnostic, REQ-015 | full profile.rs; experiment.rs:1650–1697,4673–4746 | FAIL SYS-003. Save delay participates in budget verdict; existing tests cover fast savers far below budget. |
| Production failure isolation | query timing drop guard, OTLP timed wrapper, Verification emit, Gateway timing owner | Static PASS: timing observes existing outcomes and adds no durable-write dependency or process-exit path. Required production regression journeys not run here. |
| Full Bifrost L1 within 900 s, REQ-008/AC-001 | query/mod.rs measured declarations and isolated workload | No complete valid release record supplied; broken reported run incomplete and raw file deleted. FAIL proof obligation; no speculative fixture optimization finding. |

## SYS-001 — INCORRECT: normal export produces no accepted timings and repeated waits exhaust L1

Obligation: exact raw server timing through standard tracing/export (REQ-007,
AC-001), full L1 within 900 s (REQ-008).

Location: candidate `crates/shared/wyrd-telemetry/src/lib.rs:323–349`, consumed
by `crates/wyrd/wyrd-testing/src/capacity.rs:528–564,790–839` and every engine.

`record_timing` produces u64 fields. The tracing/OpenTelemetry path exports
those unsigned fields as debug strings; `decode_timings` requires nonnegative
OTLP integer durations. Genuine requests therefore yield malformed spans and
no counted timings. Candidate's capture test stringifies attributes before
asserting, losing the type distinction. Reported real run corroborates exactly
this rejection. The signed-field repair is uncommitted and outside candidate;
parent reports its real-export test passed 1/1 in 0.256 s.

Malformed timings never satisfy Collector::collect's count predicate, so each
window consumes COLLECTION_WAIT=30 s plus 0.2 s export settling. Bifrost
collects after warmup and all three repetitions: nominal 35-second steps become
at least 155.8 s before setup, scraping and request overshoot. Six such steps
already exceed 900 s. This directly explains incomplete coverage without
inventing fixture bottlenecks from deleted evidence. Regressions judged with
no valid server timings cannot establish performance regressions.

Smallest correction: use export-compatible signed integer durations in the
existing shared producer, including excluded duration and documented saturation;
retain strict collector validation. No string-accepting consumer workaround,
new transport, longer budget or shorter windows. Affects sibling timing callers
in Bifrost Gate/query/OTLP, Verification direct/queued and Gateway buffered/stream.
One shared-owner correction and real exporter/Collector integration proof,
including exact nanoseconds above u32::MAX, closes the cause. The separately
human-directed fast release smoke must precede long qualification runs.

## SYS-002 — INCORRECT: L2 Gateway cancellation leaves traffic admission running

Obligation: interrupt/budget stops admission and drains owned work while keeping
partial INVALID evidence (REQ-008 clock/cancellation, AC-004, shared runner
contract experiment.rs:1545–1551).

Location: `crates/wyrd/wyrd-testing/src/bin/capacity/gateway/capacity.rs:313–349,
806–856,999–1018,1125–1133`; bounded runner experiment.rs:1621; shutdown in
`gateway/mod.rs:785–834`.

Ctrl-C or deadline while Capacity::step awaits Driver::spawn drops its local
JoinHandle. Tokio detaches that task; the driver keeps the full schedule and
continues launching open_loop arrivals. open_loop itself spawns each request
without an owning JoinSet. Capacity::finish never cancels or joins the scheduler
or those descendants; it immediately attempts server settle/shutdown. Fresh
calls therefore arrive after the command says admission stopped and while
drain checks for an empty server. This ordinary interruption of a multi-minute
supported L2 window extends/defeats drain and can produce shutdown errors and
misleading partial cleanup evidence. Eventual runtime teardown is too late.

Sibling L1 Gateway/Bifrost/Verification loops already use owned JoinSets.
Minimal correction: retain schedule/rates/upstream/server behavior and make the
existing driver scheduling and request descendants owned by the measurement
lifecycle, reusing established Tokio owned task/cancellation mechanisms. Stop
future launches and cancel or boundedly reconcile in-flight work before drain.
Aborting only the outer scheduler leaves per-call tasks detached. No additional
driver, public flag, changed admission, workload or durability semantics needed.

Focused closure: exercise actual Driver/open_loop against a controlled future
or local upstream; cancel before a later scheduled arrival; advance beyond its
time; assert no new launch and no live descendants before finish. Preserve full
normal schedule counts and partial INVALID interrupted/budget runner evidence.
This is one task-ownership correction boundary and one focused test, not a new
process-management abstraction.

## SYS-003 — INCORRECT: unusable optional capture can itself make budget INVALID

Obligation: REQ-015 says unusable profiling contributes a note only; profiles
never participate in verdicts and never make a run INVALID/FAIL.

Location: `crates/wyrd/wyrd-testing/src/experiment.rs:1650–1697`;
`crates/wyrd/wyrd-testing/src/profile.rs:33–35,134–179`.

A profiler starts but ignores SIGINT or cannot promptly save. Profiler::finish
waits SAVE_GRACE=60 s per PID, kills it, and returns an incomplete-capture note.
The shared runner awaits that whole wait before workload.finish, post-host
sample, and elapsed-budget judgment. An otherwise within-budget workload with
less than 60 s headroom becomes budget.exceeded INVALID solely because optional
saving failed. Drain is delayed during the same wait on interrupt. This is an
explicitly supported unusable-capture case, not a request to remove real profile
CPU perturbation from measured work. The existing profiling test uses fast fake
savers and a workload far below budget, so does not exercise this path.

Minimal correction at shared lifecycle owner: keep capture ownership, interrupt,
bounded saving and notes, but arrange finalization so optional save delay cannot
delay required stop/drain or supply the budget verdict. Continue charging actual
server/workload lifecycle, retain real finalization timestamps and genuine
workload overruns. Reuse Profiler/runner; no profiler replacement, new control,
increased budget or weakened validity gate.

Focused closure: controlled delayed/unsavable capture with a within-budget
workload near its budget has the same benchmark verdict as no profile, records
an incomplete-capture note, and does not postpone drain; actual workload overruns
remain INVALID with and without profiling. Short controlled durations suffice.

## Bifrost budget diagnosis and limits

The candidate declares **20 measured L1 steps**: point 4 + aggregate 2 + scan 2
= 8 reads; 3 write paths x 4 clients = 12 writes. Derived steps add no workload
(query/mod.rs:46–53,105,145–149,467–484). Required work alone is 20 x (5 + 3 x
10) = **700 s**. Each of 80 valid collections adds unconditional 0.2 s, giving
**716 s** before startup, fixture publication, resource scraping, request
overshoot, 20 s total host checking, and stop. About **164 s** remains after
those fixed host samples for setup and all other overhead.

With broken exports, each collection adds 30.2 s: a complete run would need
at least **3,116 s** before setup. This amplification belongs to SYS-001, not
a separate optimization finding. The 900 s deadline stops the broken run;
910 s total is consistent with the 10 s post-host sample after expiry.
Signed-field repair removes the 30 s wait when all expected timings arrive,
but does not prove setup fits the remaining allowance. No valid-run timing
artifact was supplied; the deleted timings.jsonl cannot locate phase costs.
Preserve a fresh valid smoke and complete L1 record before prescribing fixture
optimization or changing constants. Parent reports shared runner tests, engine
V10/V11/V12, and lints passed; those are not real duration qualification.

Human-directed fast release smoke, clean-tree commits and sequential real runs
are separate work, not retroactive independent finding IDs. No baseline
provenance conclusion is claimed by this scoped review.

Skipped: real benchmarks, profiler, platform and server recovery runs; no source
edits or Cargo checks. Risk: valid-run duration and actual lifecycle closure
remain unproved until focused tests and release-process evidence exist.
