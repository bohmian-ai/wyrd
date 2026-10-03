# TASK-008 round-three findings validation

## Immutable subject and validation scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`
- Prior blocked review used as source hypotheses: `changes/active/verified-change-contract/review/TASK-008-r2/`

I inspected the complete cumulative changed-path set and diff, the applicable
repository and architecture authority, the original task and prior reviews,
and every round-three discovery report. The candidate stayed at the named
commit throughout validation. The checkout has no `.codegraph/` directory, so
caller and consumer tracing used Git and repository source directly. Existing
untracked review artifacts are outside the immutable candidate.

The reports materially agree. They propose only two root findings, both stable
continuations from the prior ledger. No report conflicts on behavior or
authority, reveals a reachable path outside the union already traced, or
suggests a repeated-remediation source not covered by those two findings.
Accordingly no `followup-review.md` was needed. This is consistent with the
skill's conditional follow-up rule rather than an omitted required report.

Per the caller's explicit sequencing instruction,
`FIND-TASK-008-CLOSEOUT-13` is **DEFERRED** to integration after the other
workstreams merge. It is not a blocker or a retained candidate finding in this
ledger. The deferral supplies no empirical capacity evidence and must not be
read as an AC-040/AC-041 performance pass.

## Producer-to-consumer validation

The operator invokes `mise run bench:capacity`. The task starts RustFS, runs
its setup container, enters the repository Postgres wrapper, builds release
`wyrd-server`, and only then starts the `capacity` binary. The binary's `main`
awaits `Benchmark::prepare`; that method creates `Lifetime`, local fixture
material, and the judge. `Benchmark::run` applies `Lifetime::measure` to
`Benchmark::measure`, which provisions the first `LocalServer`, migrates the
database, runs four tenant setup commands, provisions workloads, and drives
the warmup/ramp/sustained/scale-out sequence. `Benchmark::clean_up` then shuts
clients down and synchronously stops replicas before `Report::write_to`
produces the Markdown/JSON result and the process exit code.

The command-lifetime value is therefore produced after command-owned setup has
already begun and is enforced only by `tokio::time::timeout_at` around an
in-binary future. The mandatory migration and setup consumers call
`std::process::Command::output()` in `release_server::run`. A stalled child
holds the future inside one poll, so the timer cannot regain control to drop
it. The current deadline test passes a cooperative `pending` future directly
to `Lifetime` and does not traverse the wrapper, preparation, or child-process
path. The same root also affects cleanup/report fidelity: synchronous replica
termination and report finalization occur after the measured timeout, and the
reported total begins after pre-binary command setup.

Sibling behavior does not close this path. `LocalServer::terminate` bounds and
reaps a serving replica once cleanup reaches it, and `Drop` kills an owned
serving child after ordinary cooperative cancellation. Neither owner can run
while the executor is blocked waiting for an unowned migration/setup child.
Production request deadlines, durable queue behavior, readiness polling, and
the dedicated fairness/exactly-once journeys are separate owners and must stay
unchanged.

`Lifetime::bounded` is also the shared producer of cancellation semantics for
`measure` and `shut_down`: it owns `timeout_at` and drops arbitrary supplied
work at the next cooperative yield. Its callers document selected effects, but
the shared method itself does not state that cancellation behavior or the
survival of already-produced effects. That omission is reachable because both
current lifecycle phases call the method, and it remains relevant to any
correction that retains this owner.

## Proposal dispositions

| Discovery proposal | Disposition | Validation |
|---|---|---|
| `BHV-R3-001`, `INV-R3-001`, `STD-R3-001`, `MNT-R3-001`, `SYS-R3-001`, `CAP-R3-001`, `DUR-R3-001` | **REVISED** as `FIND-TASK-008-CLOSEOUT-2` | All identify one reachable source: the absolute lifetime is owned and enforced below command setup and cannot preempt mandatory blocking operator children. Cleanup/report omissions are consequences of that source, not separate findings. |
| `BHV-R3-002`, `INV-R3-002`, `STD-R3-002`, `MNT-R3-002` | **CONFIRMED** as `FIND-TASK-008-CLOSEOUT-6` | `Lifetime::bounded` is the changed shared cancellation owner and lacks the cancellation/partial-progress contract required by `AGENTS.md` section 16 and the Rust authority. |
| Round-three treatment of prior `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED, non-blocking** | The caller expressly sequences the unmodified default run after integration of the other workstreams. No discovery report uses it as a candidate blocker. |

No proposal is rejected. The reviewers' optional observations about deriving
`Plan` flags, splitting large cohesive owners, reshaping client vectors, or
narrowing test-harness visibility correctly remain outside the finding union:
they have no reachable failure against the approved task.

## Ponytail correction analysis

For `FIND-TASK-008-CLOSEOUT-2`, deletion is unavailable because revision-57
REQ-171 expressly bounds the default `mise` command to 30 minutes including
setup, and the prior remediation requires setup-through-cleanup enforcement.
Existing behavior supplies useful pieces—`Benchmark` lifecycle ownership,
`LocalServer` child ownership, replica kill/reap, log retention, and report
failure—but not an owner spanning the complete command or a cancellable wait
for operator children. The standard process primitives already in use are
sufficient; no dependency, public timeout setting, second benchmark, or
production request deadline is justified. The smallest safe correction stays
at the existing command/`Benchmark` and `LocalServer` boundaries: establish one
absolute wall-clock deadline before command-owned setup, carry its remaining
budget through preparation, measurement, client cleanup, replica cleanup,
report finalization, and exit, and retain handles for migration/setup children
so expiry terminates and reaps them while preserving diagnostics. Work that
expires before the in-binary report owner exists must still fail nonzero and
leave no owned child behind; once that owner exists, expiry or setup failure
must use the existing failed-report path. This preserves the approved
workload, SLOs, production server behavior, and dedicated correctness tests.

For `FIND-TASK-008-CLOSEOUT-6`, deleting or wrapping the shared method adds no
value. Existing caller comments do not replace documentation on the operation
that performs cancellation. The standard `timeout_at` behavior is already the
implementation; the minimum correction is only substantive rustdoc on the
existing shared owner, or on its direct replacement if FIND-2 changes that
owner, stating cooperative future drop, surviving effects, cleanup ownership,
and that retry safety belongs to the supplied workflow. No new helper, trait,
test harness, or runtime behavior is warranted.

## Final deduplicated finding ledger

### `FIND-TASK-008-CLOSEOUT-2` — REVISED — INCORRECT

- **Discovery source IDs:** `BHV-R3-001`, `INV-R3-001`, `STD-R3-001`,
  `MNT-R3-001`, `SYS-R3-001`, `CAP-R3-001`, `DUR-R3-001`.
- **Violated obligation:** Revision-57 REQ-171 and remediation R2 require the
  complete default command, including setup, to finish within one absolute
  30-minute boundary, with expiry stopping later work, retaining diagnostics,
  returning failure, and completing owned cleanup.
- **Exact location:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:79-188,227-344,397-429,500-531,621-637`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:130-189,347-388,469-483,618-635`.
- **Evidence:** RustFS startup/setup, the Postgres wrapper, and release build
  precede `Lifetime`. `Benchmark::prepare` is awaited outside the bounded
  workflow. Mandatory migration and four tenant setup commands synchronously
  wait in `Command::output()`, preventing the Tokio timer from being polled.
  Replica stops and report writes are not governed by the same absolute
  deadline, and `Report::total_seconds` omits pre-binary command time. The
  paused-clock test exercises only a cooperative future.
- **Observable consequence:** A slow wrapper/build or wedged migration/setup
  child can keep the operator command alive beyond 30 minutes. A wedged child
  can also prevent failed-report generation, server cleanup, child reaping,
  and diagnostic finalization; slower outer setup or cleanup can be absent
  from the reported duration even when the process eventually exits.
- **Decision-complete minimum correction:** Retain `Benchmark` as the
  in-binary lifecycle owner and `LocalServer` as the process owner. Put the
  actual default command's setup-to-exit process tree under one absolute
  deadline begun before command-owned setup; propagate the remaining budget
  through preparation, measurement, client shutdown, replica stop, log/report
  finalization, and exit. Replace blocking migration/setup waits with owned
  child lifecycles that can be terminated and reaped on that same expiry while
  retaining stdout/stderr diagnostics. Use the existing failed-report path
  whenever its owner has been initialized; otherwise fail the command nonzero
  without leaving owned children. Preserve all production timeouts, benchmark
  defaults/SLOs, and existing server/log cleanup semantics. Add no public
  timeout knob, dependency, parallel supervisor abstraction, or downstream
  duration guard.
- **Focused closure proof:** Exercise a controlled migration or tenant-setup
  child that never exits under a shortened command deadline, with a serving
  replica already owned for the setup case. Prove bounded nonzero command
  completion, operator-child and replica termination/reaping, no later
  setup/measurement, retained diagnostics, and a failed report whenever the
  report owner exists. Preserve the cooperative lifetime unit proof, run the
  complete `capacity` binary target, and use the separately deferred
  unmodified default execution to qualify the real 30-minute ceiling after
  integration.

### `FIND-TASK-008-CLOSEOUT-6` — CONFIRMED — VIOLATION

- **Discovery source IDs:** `BHV-R3-002`, `INV-R3-002`, `STD-R3-002`,
  `MNT-R3-002`.
- **Violated obligation:** `AGENTS.md` section 16,
  `architecture/agent-rules.md`, and the Rust core authority require a
  materially changed async operation to document relevant cancellation,
  partial progress, cleanup, and retry behavior.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`.
- **Evidence:** `Lifetime::bounded` directly owns `timeout_at` and drops the
  supplied workflow on expiry, but its rustdoc only says it runs work until a
  deadline and names the phase in the failure. The more specific current
  caller docs do not define the shared operation's contract.
- **Observable consequence:** A maintainer changing or reusing the shared
  boundary must infer whether expiry is immediate, rolls effects back, leaves
  durable residue, or relies on another owner for cleanup. A new caller can
  therefore rely on a false atomicity or rollback assumption at the exact
  cancellation boundary.
- **Decision-complete minimum correction:** Document on
  `Lifetime::bounded`—or its direct replacement under FIND-2—that expiry drops
  the supplied future at its next cooperative yield; already-produced effects
  remain according to the supplied workflow's owner; cleanup is performed by
  those owners; and retry safety is workflow-specific. Preserve the existing
  caller-specific contracts. Add no wrapper, trait, second timeout abstraction,
  or runtime behavior solely for documentation.
- **Focused closure proof:** Perform a complete static async-item documentation
  audit of the final lifetime/process path, including
  `Lifetime::{bounded,measure,shut_down}` when retained, and keep the focused
  `wyrd-testing` capacity target plus rustdoc/Clippy checks green.

## Prior-finding closure

| Prior finding | Validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: only the consolidated server-capacity entry point remains. |
| `FIND-TASK-008-CLOSEOUT-2` | OPEN, REVISED: the attempted lifetime does not cover the complete command or preempt/reap blocking setup children. |
| `FIND-TASK-008-CLOSEOUT-3` | CLOSED: judge provider wait is separate from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | CLOSED: the task and approved specification both identify revision 57. |
| `FIND-TASK-008-CLOSEOUT-5` | CLOSED: `Benchmark` owns the multi-step lifecycle and reconnection. |
| `FIND-TASK-008-CLOSEOUT-6` | OPEN, CONFIRMED: the shared timeout owner lacks its cancellation/partial-progress contract. |
| `FIND-TASK-008-CLOSEOUT-7` | CLOSED: `StepKind` owns the closed verdict identity and labels. |
| `FIND-TASK-008-CLOSEOUT-8` | CLOSED: staged live members participate in Scribe backlog. |
| `FIND-TASK-008-CLOSEOUT-9` | CLOSED: drain/report judgment preserves the exact 60-second edge. |
| `FIND-TASK-008-CLOSEOUT-10` | CLOSED: aggregate and operation rows share one table schema. |
| `FIND-TASK-008-CLOSEOUT-11` | CLOSED: CPU and peak memory share the measured interval and actual duration. |
| `FIND-TASK-008-CLOSEOUT-12` | CLOSED: duplicate-claim judgment remains test-owned, while approved request errors remain. |
| `FIND-TASK-008-CLOSEOUT-13` | DEFERRED by caller direction to post-integration default execution; not a blocker for this candidate and not evidence of empirical acceptance. |

## Validation result

The final validated ledger contains two bounded implementation findings:
`FIND-TASK-008-CLOSEOUT-2` and `FIND-TASK-008-CLOSEOUT-6`. Both corrections fit
the approved revision-57 behavior and existing private benchmark/release
harness owners; neither requires a specification revision, public API,
production concurrency, durability, security, compatibility, or persistent
data decision.

**Validated result: FIX_REQUIRED.**
