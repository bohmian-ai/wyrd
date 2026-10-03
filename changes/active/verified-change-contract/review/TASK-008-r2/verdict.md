# TASK-008 closeout task-review verdict, round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative diff: `7d96c30066425e0cde2290842d5801307843283d..5c3bb79b3598abd88a3a234611fc400096adc975`
- Caller-named approved specification: `changes/active/verified-change-contract/spec.md`, revision 58
- Specification present in the candidate: the same path, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-008-r1/`

The candidate commit remained unchanged throughout review. Only the assigned
`TASK-008-r2` review artifacts were added to the working tree. CodeGraph is not
available in this checkout.

## Authority result

The exact approved authority named by the caller is absent from the immutable
subject. Candidate `spec.md:1-4` declares approved revision 57, and both the
original task and r1 remediation bind themselves to revision 57. No revision-58
specification is present in the candidate packet.

A sibling, non-ancestor Git object contains text labeled revision 58, but it is
not part of the reviewed candidate and assigns `REQ-178` a meaning different
from candidate revision 57. Under the repository's spec-driven authority order,
that external branch state cannot be substituted for the caller-named file.
The review therefore cannot decide whether the cumulative candidate satisfies
revision 58.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Review the candidate against the exact approved revision 58 | Candidate `spec.md`, task frontmatter, and remediation all identify revision 57 | Candidate blob, ancestry, packet, and history inspection | **BLOCKED** |
| Prior FIND-1: retain one server-capacity entry point | Legacy mise task, Cargo target, and `bifrost_query_capacity` source are deleted | Repository search and changed-path inspection | PASS under revision 57 |
| Prior FIND-2: enforce one complete-command 30-minute boundary | `Lifetime` begins inside the binary and bounds cooperative async work; mandatory migration/setup still uses blocking `Command::output()` and the mise wrapper begins work earlier | Paused-time test proves only a yielding future; no stalled-child proof | **FAIL under revision 57** — `FIND-TASK-008-CLOSEOUT-2` |
| Prior FIND-3: report judge provider wait separately | `Judge` produces wait samples consumed by JSON and Markdown apart from engine overhead | Focused report test and reduced smoke | PASS under revision 57 |
| Prior FIND-4: current task contract | Task metadata and prose consistently identify revision 57 review | Static inspection | PASS for revision 57; BLOCKED for requested revision 58 |
| Prior FIND-5 and FIND-7: cohesive lifecycle owner and exhaustive step identity | `Benchmark` owns setup-to-report lifecycle; `StepKind` owns labels and verdict selection | Source and caller trace; capacity unit target | PASS under revision 57 |
| Prior FIND-6: document every changed async cancellation boundary | Most changed boundaries carry cancellation contracts, but `Lifetime::bounded` owns `timeout_at` and drops supplied work without documenting cancellation or retained effects | Static async-item audit | **FAIL under revision 57** — `FIND-TASK-008-CLOSEOUT-6` |
| Prior FIND-8 and FIND-9: complete Scribe backlog and exact drain edge | Scribe backlog includes staged live members; drain distinguishes at/below/above the limit | Focused staged-member and boundary tests | PASS under revision 57 |
| Prior FIND-10 and FIND-11: one report table and comparable resource window | One shared schema renders step and operation rows; CPU and memory use one interval and measured denominator | Focused render and resource-window tests | PASS under revision 57 |
| Prior FIND-12: exactly-once remains test-owned | Duplicate-run capacity judgment is deleted while approved error categories remain | Focused report test and recorded two-replica claim proof | PASS under revision 57 |
| Prior FIND-13: record one passing unmodified default execution | Remediation explicitly records `NOT RUN (integrator)`; only a shortened `L = 20`, 5/10/15-second failing smoke exists | No immutable default report, identity, or wall time | **FAIL under revision 57** — `FIND-TASK-008-CLOSEOUT-13` |
| Preserve revision-57 non-goals | Remediation changes benchmark/test support, task evidence, and removal of the obsolete entry point; it adds no production limiter, public contract, or storage format | Cumulative and remediation diff inspection | PASS under revision 57 |

## Independent review results

| Report | Result | Material contribution |
|---|---|---|
| `task-review-behavior.md` | FAIL | Proposed revision-58 runner work from external branch state, plus the lifetime and default-run gaps; validation rejected the external-authority proposal |
| `task-review-invariants.md` | BLOCKED | Established the authority mismatch and conditionally retained the complete-command lifetime and default-run gaps |
| `standards-review.md` | BLOCKED | Confirmed that the caller-named approved revision is absent; prior standards findings are provisionally closed under revision 57 |
| `maintainer-review.md` | BLOCKED | Confirmed the authority mismatch and proposed two maintainability claims for validation |
| `system-review.md` | BLOCKED | Traced the reachable blocking migration/setup path that bypasses the async deadline |
| `domain-review-capacity.md` | BLOCKED | Confirmed the authority mismatch, deadline gap, and missing default performance proof |
| `domain-review-durability.md` | BLOCKED | Confirmed that blocking setup can prevent timeout, cleanup, and reporting |

All required discovery reports are present.

## Follow-up decision

`followup-review.md` was required because the behavior reviewer imported a
revision-58 specification from a sibling commit while every other reviewer
treated the caller-named authority as absent. It resolved the conflict:

- no approved revision-58 authority exists inside the immutable candidate;
- the sibling commit cannot be substituted and its runner requirement is
  excluded from this review's ledger;
- the late lifetime start, unbounded preparation, and blocking subprocess wait
  are one reachable root continuation of prior FIND-2; and
- prior FIND-13 remains open because the remediation explicitly did not run the
  required default benchmark.

The follow-up result is `RESOLVED`.

## Validated finding ledger

`findings-validation.md` independently checked every discovery proposal and
returned `BLOCKED` because revision 58 is unavailable. It rejected the proposed
revision-58 runner finding rather than converting sibling state into authority,
and rejected the unreachable `Plan`-boolean maintainability proposal.

The following ledger is validated only against committed revision 57 and is
preserved so the authority blocker does not erase source-backed remediation
state:

| ID | Status | Class | Location | Result |
|---|---|---|---|---|
| `FIND-TASK-008-CLOSEOUT-2` | REVISED, OPEN | INCORRECT | `mise.toml:507-524`; `capacity/main.rs:124-188,227-343,397-418,500-531,621-635`; `release_server.rs:147-189,347-388,469-483,618-635` | The complete default command and blocking setup children are not enforceably bounded by one 30-minute deadline. |
| `FIND-TASK-008-CLOSEOUT-6` | REVISED, OPEN | VIOLATION | `capacity/main.rs:174-188` | `Lifetime::bounded` owns cancellation but does not document dropped work or surviving partial effects. |
| `FIND-TASK-008-CLOSEOUT-13` | CONFIRMED, OPEN | MISSING | `task-008-closeout.md:1490-1574`; r1 remediation evidence | No unmodified passing default benchmark execution is recorded. |

These findings are conditional evidence, not a remediation authorization under
the missing revision-58 contract.

## Verification limits

- Independently rerun capacity unit target: 14 passed, 0 failed.
- Focused staged-Scribe, drain-boundary, and async-deadline tests: 3 passed.
- `git diff --check`: passed.
- The remediation records passing focused Postgres-backed exactly-once,
  fairness, ingestion, judgment, formatting, lint, and Clippy commands; those
  recorded results were reviewed but not all rerun in this round.
- No stalled migration/setup child proof exists.
- No unmodified default `mise run bench:capacity` result exists.
- No test result can establish conformance to a specification revision absent
  from the immutable subject.

## Prior-finding closure

Conditionally against revision 57, prior findings 1, 3, 4, 5, and 7 through 12
are closed. Prior findings 2, 6, and 13 remain open as listed above. No prior
finding is considered finally closed against caller-named revision 58 because
that authority is unavailable.

## Verdict

**BLOCKED**

The immutable candidate does not contain the caller-specified approved
revision-58 specification, so the task cannot be accepted, rejected, or routed
to bounded implementation remediation against that unknown contract. Supply a
candidate containing the exact approved revision-58 packet and reconciled task
metadata, or correct the requested authority to revision 57, then rerun the
complete task review. Because the verdict is `BLOCKED`, no remediation task is
written.
