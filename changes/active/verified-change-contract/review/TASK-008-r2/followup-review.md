# Focused follow-up review

## Immutable subject and question

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Requested authority: `changes/active/verified-change-contract/spec.md`, approved revision 58
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`

This follow-up resolves three conflicts from discovery: whether the immutable
subject contains usable revision-58 authority, whether the benchmark deadline
failure is reachable and one root defect, and whether prior
`FIND-TASK-008-CLOSEOUT-13` is closed. `HEAD` resolved to the candidate before
this report was written. `.codegraph/` is absent.

## Paths inspected

- Authority and workflow:
  - `AGENTS.md`
  - `architecture/agent-rules.md`
  - `architecture/references/languages/spec-driven-development.md`
  - `architecture/references/languages/maintainer-style.md`
  - `changes/active/verified-change-contract/spec.md`
  - `changes/active/verified-change-contract/tasks/task-008-closeout.md`
  - `changes/active/verified-change-contract/review/TASK-008-r1/verdict.md`
  - `changes/active/verified-change-contract/review/TASK-008-r1/TASK-008-CLOSEOUT-R1-capacity-closure.md`
- Runtime and verification path:
  - `mise.toml`
  - `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs`
  - `crates/wyrd/wyrd-testing/src/bin/capacity/judge.rs`
  - `crates/wyrd/wyrd-testing/src/bin/capacity/fixture.rs`
  - `crates/wyrd/wyrd-testing/src/release_server.rs`
  - all discovery reports already present under `review/TASK-008-r2/`
- Git/source identity checks:
  - candidate versions of the spec and task via `git show`
  - ancestry and tree relationship between candidate
    `5c3bb79b3598abd88a3a234611fc400096adc975` and sibling commit
    `e202e1f272c101f93d13dcc9d0b2a7edc1320f74`
  - cumulative and remediation changed-path lists

## Resolution 1: revision 58 is unavailable inside the immutable subject

**RESOLVED.** No source inside the immutable candidate supplies an unambiguous
approved revision-58 specification.

- Candidate `spec.md:1-4` says `revision: 57` and `status: approved`.
- Candidate `task-008-closeout.md:1-18` says `spec_revision: 57`; the r1
  remediation also identifies revision 57 as its authority.
- Candidate revision 57 already defines `REQ-178` at `spec.md:1837` as the
  capture-writer topology requirement. Nothing in the candidate change packet
  identifies the runner Postgres-bound text as revision-58 authority.
- Commit `e202e1f272c101f93d13dcc9d0b2a7edc1320f74` does contain a file whose
  frontmatter says approved revision 58 and adds a different `REQ-178` for a
  runner Postgres connection bound. It is a child of sibling-branch commit
  `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`, not an ancestor of the immutable
  candidate. `git merge-base --is-ancestor e202e1f27 5c3bb79b3` fails.
- The spec-driven workflow requires the approved change packet to remain in the
  reviewed candidate and prohibits tests, implementation, or repository
  discovery from silently redefining it. A reachable Git object or sibling
  branch is therefore not authority for this review. The duplicate `REQ-178`
  meaning further demonstrates why it cannot be merged conceptually into the
  candidate's revision 57 without an authorized candidate containing the
  actual approved document.

Consequently the behavior review's revision-58 runner finding cannot enter the
validated finding ledger from this subject. It is not disproved; it is
unreviewable here. The task review is blocked until the caller supplies an
immutable candidate containing the exact approved revision-58 packet, or
corrects the requested authority to revision 57.

## Resolution 2: the deadline defect is reachable and has one root

**RESOLVED.** The failure is reachable on every default benchmark invocation,
and the apparently separate late-start, preparation, and blocking-subprocess
symptoms are one root defect against available revision-57 `REQ-171` and prior
`FIND-TASK-008-CLOSEOUT-2`.

Revision-57 `REQ-171` at `spec.md:1706-1713` says the default
`mise run bench:capacity` must complete within 30 minutes, including setup. The
default caller in `mise.toml:507-524` performs RustFS startup/setup, enters the
Postgres wrapper, builds the release server, and only then starts the capacity
binary. The binary creates `Lifetime` inside `Benchmark::prepare`
(`main.rs:240-245`), so none of those command-owned setup stages is inside that
clock.

Within the binary, `main` awaits all of `Benchmark::prepare` before calling
`Benchmark::run` (`main.rs:621-635`). The lifetime clock has started, but the
only enforcement is `Lifetime::measure(self.measure())` inside `run`
(`main.rs:311-314`). A preparation await such as `Judge::start`'s listener bind
(`judge.rs:62-79`) is therefore not itself raced against the deadline, and a
preparation error exits through stderr without the normal report path.

The default measured path then always calls `Benchmark::provision`, which
always calls `LocalServer::start` (`main.rs:359-361,397-418`). That start path
always runs one migration and one setup command for each of the four tenants
through `run` (`release_server.rs:147-189`). `run` uses synchronous
`std::process::Command::output()` with no deadline (`release_server.rs:618-635`).
If any child stops making progress, its poll does not yield, so
`tokio::time::timeout_at` in `Lifetime::bounded` (`main.rs:174-187`) cannot fire.
This is not a dormant or optional branch: migration plus the four setup calls
are mandatory for the default first replica.

The focused unit test at `main.rs:647-679` passes a cooperative
`std::future::pending` future directly to `Lifetime`; paused Tokio time can
cancel that future because it yields. It does not execute `Benchmark::prepare`,
the mise wrapper, or a blocking operator child, so it cannot prove the
setup-through-cleanup wall-clock obligation. `LocalServer::stop` has an owned
bounded terminate/kill/reap path (`release_server.rs:347-388`) and abnormal
drop kills and reaps the serving child (`release_server.rs:469-483`), but
neither can run while the same executor thread is blocked waiting for the
migration/setup child.

These are not independent findings. They have the same violated obligation,
the same observable outcome (the default command can outlive 30 minutes
without the promised failed report and owned cleanup), and the same source:
the absolute lifetime is represented and enforced too far inside the command
and only through cooperative future cancellation, rather than being owned at a
boundary that covers the complete invoked setup and controls every child
process. Preserve stable ID `FIND-TASK-008-CLOSEOUT-2` as **REVISED and open**;
do not split preparation and `Command::output()` into additional findings.

The testable correction remains within revision 57: make the one absolute
deadline cover the complete default command from setup through cleanup, and
make owned migration/setup children terminable and reapable at that deadline.
Proof must exercise an actually stalled child/process setup path and establish
nonzero failure, retained diagnostics/reporting, cleanup, and termination
inside the absolute ceiling. The cooperative paused-time test may remain as a
unit proof but is not closure by itself.

## Resolution 3: prior finding 13 is not closed

**RESOLVED.** `FIND-TASK-008-CLOSEOUT-13` remains open.

- The remediation requires an unmodified `mise run bench:capacity` after the
  source fixes, reaching `L = 200`, passing the required one- and two-replica
  verdict steps and AC-040/AC-041 evidence, and recording the full report
  identity and wall time (`TASK-008-CLOSEOUT-R1-capacity-closure.md:201-216,
  285-287`).
- Its own finding validation says FIND-13 is valid but deferred to integration
  (`:293-300`), and its evidence table says `NOT RUN (integrator)` (`:320`).
- The only recorded execution changes the workload to one `L = 20` level and
  the windows to 5/10/15 seconds, exits 1, and has only 32 samples per
  non-judge kind in the one-replica sustained step (`:329-336`; original task
  `task-008-closeout.md:1549-1559`). It is explicitly a plumbing smoke, not the
  acceptance run.
- No later candidate change records a default report or wall time. The local
  untracked `target/capacity/report.md` likewise identifies the shortened
  `L = 20`, 5/10/15-second failing smoke and is neither authoritative nor
  qualifying evidence.

This is the existing stable finding, not a new discovery finding. Source and
unit/integration proof can establish benchmark structure but cannot replace
the empirical performance evidence that the requirement and remediation
explicitly require.

## Proposed finding reconciliation

| Claim | Follow-up disposition |
|---|---|
| Revision-58 runner connection bound is missing | Exclude from this subject's finding ledger. The purported authority is outside the immutable candidate; the review is `BLOCKED`, not authorized to substitute sibling state. |
| Benchmark whole-command lifetime | Retain prior `FIND-TASK-008-CLOSEOUT-2` as one **REVISED**, still-open finding covering the late ownership and unpreemptible child path. |
| Default empirical execution | Retain prior `FIND-TASK-008-CLOSEOUT-13` as **CONFIRMED**, still open. |
| Additional unique finding | None. Preparation, wrapper setup, and blocking operator waits are symptoms of the same lifetime-ownership defect rather than separate remediation units. |

## Verification limits

- This was a read-only source and evidence reconciliation. No production code
  or tests were modified, and no benchmark or test lane was run.
- A real stalled-child proof does not exist in the candidate; the absence of
  that proof is part of the retained FIND-2 gap.
- No unmodified default benchmark evidence exists in the candidate; running it
  during this blocked review would not repair the immutable evidence packet.

## Result

**RESOLVED**

The three uncertainties are source-resolvable. The immutable review remains
blocked because approved revision 58 is absent. Conditional on using available
approved revision 57, prior findings 2 and 13 both remain open, with finding 2
consolidating all deadline symptoms under one root defect.
