# TASK-008 round-two findings validation

## Immutable subject and validation coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Caller-named authority: `changes/active/verified-change-contract/spec.md`,
  approved revision 58
- Authority present in the candidate: the same path at approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior verdict and remediation:
  `changes/active/verified-change-contract/review/TASK-008-r1/`

The candidate matched `HEAD` before validation. The repository has no
`.codegraph/` directory, so direct Git, source, caller, and consumer inspection
was used. I read the complete cumulative diff, all required round-two
discovery reports, the focused follow-up, the prior verdict and remediation,
and the cited implementation and proof paths.

The requested approved authority is not present in the immutable subject.
Candidate `spec.md:1-4` declares approved revision 57, and the original task
and remediation both bind themselves to revision 57. A Git object at
`e202e1f272c101f93d13dcc9d0b2a7edc1320f74` contains an approved revision 58,
but that commit is not an ancestor of the candidate. It is sibling-branch
state and even assigns `REQ-178` a meaning different from candidate revision
57's existing `REQ-178`. The repository's spec-driven workflow requires the
approved packet to remain in the reviewed candidate. The sibling object
therefore cannot be substituted as authority.

This prevents a final acceptance ledger against revision 58. To preserve the
source evidence already available, the proposal decisions and conditional
ledger below also state what is independently established against the
committed revision-57 packet. Those conditional decisions do not authorize a
revision-58 verdict.

## Producer-to-consumer trace

`mise run bench:capacity` starts RustFS, enters the repository Postgres
wrapper, builds release `wyrd-server`, and starts the `capacity` binary. The
binary's `main` awaits `Benchmark::prepare`, which creates `Lifetime`, local
fixture material, and the judge. `Benchmark::run` then places only
`Benchmark::measure` under `Lifetime::measure`; measurement provisions the
first `LocalServer`, runs migration and four tenant setup commands, connects
the four public-client tenants, and drives each `Plan` through
`Deployment::run`. `Report` consumes the resulting records and decides the
operator-visible verdict.

The absolute-lifetime state is produced in `Benchmark::prepare`, but its timer
is not applied to preparation itself. The mandatory migration and setup path
then calls `std::process::Command::output()` inside `LocalServer::start`. That
wait blocks the poll executing the bounded future, so the outer Tokio timeout
cannot regain control to cancel it. The current shortened proof passes a
cooperatively pending future directly to `Lifetime`; it never traverses
preparation or a child process. These late-start, pre-run, blocking-child, and
missing-report symptoms share one source: the command deadline is represented
and enforced below work it is required to bound.

`Plan::new` and the sole production sequence currently produce consistent
`judged` and `sample_floor` values. `Report::{result,overhead}` are the only
consumers. No current caller constructs the invalid combinations proposed by
MR-001. By contrast, `Lifetime::bounded` is itself the changed cancellation
boundary: it drops the supplied future on timeout, but lacks the cancellation
and partial-progress contract required for materially changed async Rust.

The only committed capacity execution remains the reduced `L = 20`,
5/10/15-second smoke. It exits 1, records 32 samples per non-judge kind, and
does not reach the default `L = 200` workload. No later candidate artifact
contains an unmodified default report, its identity, or its wall time.

## Proposed-finding decisions

### `STD-R2-001` — CONFIRMED as the authority blocker

The caller named revision 58, while the candidate's specification, task, and
remediation all name revision 57. Under the task-review and spec-driven
authority rules, an external sibling commit cannot repair the immutable
subject. This is not a bounded implementation finding to package under an
unknown contract. Unblock by supplying a candidate that contains the exact
approved revision-58 packet and reconciled task metadata, or by correcting the
review request if revision 57 is the intended authority; then rerun the whole
review against that immutable subject.

### `BHV-R2-001` — REJECTED from this subject's ledger

The behavior review's runner Postgres-bound diagnosis depends entirely on the
revision-58 text found only in non-ancestor commit `e202e1f27`. The candidate
contains no authority that requires that limiter, retry behavior, or 200-run
proof, and revision 57 uses `REQ-178` for a different contract. The proposal
is neither confirmed nor disproved on its merits; it is unavailable for this
review and cannot enter the ledger by importing sibling state. The missing
authority yields `BLOCKED`, not a speculative runner remediation.

### Deadline proposals — REVISED as `FIND-TASK-008-CLOSEOUT-2`

`BHV-R2-002`, `INV-R2-001`, `SYS-R2-001`, `CAP-R2-001`, and `DUR-001` all
identify the same open prior finding. The default path always executes
`Benchmark::prepare`, migration, and four setup commands. Preparation is
awaited before `Benchmark::run` installs the timeout, and migration/setup use
uninterruptible `Command::output()`. The current test proves cancellation only
for a yielding future. These are one root defect, not separate preparation,
subprocess, report, or cleanup findings. Preserve stable ID
`FIND-TASK-008-CLOSEOUT-2` as revised and open.

### Default-run proposals — CONFIRMED as `FIND-TASK-008-CLOSEOUT-13`

`BHV-R2-003`, `INV-R2-002`, and `CAP-R2-002` restate the same prior evidence
gap. The remediation explicitly records R13 as `NOT RUN (integrator)`, and the
only committed execution changes every duration and the level, fails, and
never exercises the required default evidence. Unit, integration, and reduced
smoke results cannot establish release capacity. Preserve stable ID
`FIND-TASK-008-CLOSEOUT-13` as confirmed and open.

### `MR-001` — REJECTED

The proposed invalid `Plan` combinations are representable, but no current
producer creates one: all production construction is in the fixed
`Benchmark::measure` sequence, `Plan::new` derives warmup judgment, and the
one-replica sustained call alone enables the sample floor. The proposal
demonstrates possible future drift, not a reachable failure of the approved
task. Deleting the booleans may be a reasonable later simplification, but it
is not an acceptance finding and would broaden this remediation beyond a
failing caller-to-result path.

### `MR-002` — REVISED in part as prior `FIND-TASK-008-CLOSEOUT-6`

The configurable-limit diagnostic subclaim is rejected: production constructs
`Lifetime` only with the global 30-minute `LIMIT`, so the reported duration is
truthful on every runtime path. The inconsistent 60-second test assertion does
not independently change product behavior or invalidate the timing
assertions.

The documentation subclaim is confirmed. `Lifetime::bounded` is a new async
cancellation owner that calls `timeout_at` and drops arbitrary supplied work,
yet its own rustdoc has no `# Cancellation` contract or statement of retained
partial effects. Caller-specific docs do not document the shared boundary
that performs the cancellation. This reopens prior
`FIND-TASK-008-CLOSEOUT-6`; it is not a new finding ID.

## Prior-finding closure under committed revision 57

| Prior finding | Independent validation result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-1` | CLOSED: the stale mise task, Cargo target, and binary are deleted. |
| `FIND-TASK-008-CLOSEOUT-2` | OPEN, REVISED: the deadline does not cover preparation or terminate/reap blocking migration/setup children. |
| `FIND-TASK-008-CLOSEOUT-3` | CLOSED: provider wait is retained and rendered separately from engine overhead. |
| `FIND-TASK-008-CLOSEOUT-4` | CLOSED relative to revision 57: task frontmatter and current-contract prose agree. Revision 58 remains an authority blocker, not closure evidence. |
| `FIND-TASK-008-CLOSEOUT-5` | CLOSED: `Benchmark` owns the multi-step lifecycle and reconnection. |
| `FIND-TASK-008-CLOSEOUT-6` | OPEN, REVISED: the new shared cancellation boundary `Lifetime::bounded` lacks its required cancellation/partial-progress contract. |
| `FIND-TASK-008-CLOSEOUT-7` | CLOSED: `StepKind` owns the closed identity and external labels. MR-001 does not reopen it without a reachable invalid producer. |
| `FIND-TASK-008-CLOSEOUT-8` | CLOSED: the Scribe backlog includes staged live members. |
| `FIND-TASK-008-CLOSEOUT-9` | CLOSED: the drain distinguishes empty at/before the boundary from empty after it. |
| `FIND-TASK-008-CLOSEOUT-10` | CLOSED: step and operation rows use one shared table. |
| `FIND-TASK-008-CLOSEOUT-11` | CLOSED: CPU and peak memory use one measured interval and its actual duration. |
| `FIND-TASK-008-CLOSEOUT-12` | CLOSED: duplicate-run capacity judgment is deleted while test-owned correctness proof remains. |
| `FIND-TASK-008-CLOSEOUT-13` | OPEN, CONFIRMED: no unmodified default run is recorded. |

## Conditional final deduplicated ledger for revision 57

This ledger is source-valid only if revision 57 is confirmed as the intended
authority. It is not a final ledger for the caller-named revision 58.

### `FIND-TASK-008-CLOSEOUT-2` — REVISED — INCORRECT

- **Discovery source IDs:** `BHV-R2-002`, `INV-R2-001`, `SYS-R2-001`,
  `CAP-R2-001`, `DUR-001`; follow-up resolution 2.
- **Violated obligation:** Revision-57 REQ-171 and the prior remediation
  require one absolute 30-minute default-command boundary from setup through
  bounded cleanup, with expiry stopping later work, retaining diagnostics,
  returning failure, and using owned cleanup.
- **Exact location:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:124-188,227-343,397-418,500-531,621-635`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,347-388,469-483,618-635`.
- **Evidence and observable consequence:** Work owned by the default mise
  command begins before `Lifetime`; preparation is awaited outside
  `Lifetime::measure`; mandatory migration and setup use blocking
  `Command::output()`. A stalled setup child can therefore outlive the
  approved ceiling while preventing the timeout, normal failed report, and
  cleanup path from running. The paused-time test exercises only a cooperative
  pending future and cannot close this path.
- **Decision-complete minimum correction:** Keep lifetime ownership at the
  existing command/`Benchmark` boundary and child ownership at
  `LocalServer`. Start the single absolute wall-clock limit before default
  command setup, apply its remaining budget to fallible preparation,
  measurement, client cleanup, and replica cleanup, and run migration/setup as
  owned child processes that can be terminated and reaped when that same
  deadline expires. Use the standard process lifecycle and existing
  `LocalServer` cleanup/log retention; add no timeout knob, dependency,
  production request deadline, or downstream guard. An in-binary preparation
  error or expiry must enter the existing failed-report path; command-owned
  work before the binary starts must still return nonzero within the same
  absolute limit without leaving owned children running.
- **Focused closure proof:** Add a process-level shortened-lifetime case whose
  controlled migration or setup child never completes. Prove the child is
  killed and reaped, later setup/measurement does not start, replica/client
  cleanup runs when those owners exist, diagnostics are retained, a started
  benchmark writes a failed report, and the command returns nonzero within the
  absolute limit. Keep the cooperative unit proof as supporting evidence, then
  use the unmodified default execution under FIND-13 to prove the real ceiling.

### `FIND-TASK-008-CLOSEOUT-6` — REVISED — VIOLATION

- **Discovery source IDs:** documentation portion of `MR-002`.
- **Violated obligation:** `AGENTS.md` and `architecture/agent-rules.md`
  require every materially changed async Rust boundary to document
  cancellation and partial progress.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:174-188`.
- **Evidence and observable consequence:** `Lifetime::bounded` owns the
  `timeout_at` call that drops its supplied workflow at the deadline. Its
  rustdoc states only that it runs work until a deadline; it does not state
  that timeout drops the future or that already-produced effects remain as
  defined by the work's owner. Maintainers must infer the actual shared
  cancellation contract from selected callers, and a new caller can rely on a
  false rollback assumption at the exact boundary introduced to satisfy the
  lifecycle remediation.
- **Decision-complete minimum correction:** Document cancellation on the
  existing `Lifetime::bounded` owner: deadline expiry drops the supplied
  future at its next cooperative yield, and effects already produced remain
  according to that future's owner. Preserve the more specific measurement
  and client-cleanup contracts on the callers. Add no wrapper type, trait, or
  second timeout abstraction.
- **Focused closure proof:** A static async-item documentation audit covers
  `Lifetime::{bounded,measure,shut_down}` and the existing focused capacity
  target remains green.

### `FIND-TASK-008-CLOSEOUT-13` — CONFIRMED — MISSING

- **Discovery source IDs:** `BHV-R2-003`, `INV-R2-002`, `CAP-R2-002`;
  follow-up resolution 3.
- **Violated obligation:** Revision-57 REQ-171, AC-040, AC-041, and remediation
  R13 require one unmodified default execution to qualify the release
  two-replica/Postgres/RustFS deployment, its three verdict steps, the AC-040
  sample and latency evidence, AC-041 at `L = 200`, and the 30-minute ceiling.
- **Exact location:**
  `changes/active/verified-change-contract/tasks/task-008-closeout.md:1490-1574`;
  `changes/active/verified-change-contract/review/TASK-008-r1/TASK-008-CLOSEOUT-R1-capacity-closure.md:201-216,293-344`.
- **Evidence and observable consequence:** The remediation says R13 is valid
  and `NOT RUN (integrator)`. Its sole recorded execution supplies `--levels
  20` and 5/10/15-second windows, exits 1, and yields 32 samples per kind. No
  immutable artifact records the default report, configuration identity, or
  wall time. The repository therefore has no promoted measurement supporting
  its stated release capacity or command ceiling.
- **Decision-complete minimum correction:** After FIND-2 is closed and the
  intended approved specification is present, run exactly the unmodified
  `mise run bench:capacity` in the prescribed release/Postgres/RustFS and
  per-replica cgroup envelope. Append the exact command, complete Markdown and
  JSON report identity, configuration, verdict evidence, and wall time to the
  task record. Do not replace it with another reduced smoke or a unit test. A
  failed run requires diagnosis and correction within the approved authority.
- **Focused closure proof:** The committed evidence reaches `L = 200`, carries
  at least 1,000 non-judge samples where AC-040 requires them, reports judge
  wait separately, exposes AC-041 refusal and drain evidence, passes the
  one-replica sustained and both two-replica verdict steps, and completes the
  full command within 30 minutes.

## Validation result

The exact approved revision-58 authority cannot be obtained from the immutable
candidate, so source evidence cannot resolve which requirements govern the
task. `BHV-R2-001` is excluded rather than converted into remediation from a
sibling branch. The conditional revision-57 audit retains three findings:
`FIND-TASK-008-CLOSEOUT-2`, `FIND-TASK-008-CLOSEOUT-6`, and
`FIND-TASK-008-CLOSEOUT-13`.

**Validated result: BLOCKED — approved revision 58 is absent from the immutable
subject; conditional revision-57 ledger has three retained findings.**
