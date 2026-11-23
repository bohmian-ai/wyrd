# TASK-001-R2 independent working-tree validation

**Substantive recommendation: PASS for the frozen working-tree snapshot.**
FIND-TASK-001-2 and FIND-TASK-001-3 close: the actual runner transition now
supplies the supervisor's teardown start, and late physical cleanup cannot
renew confirmation or next-case admission. No implementation defect or
material evidence gap remains in this bounded remediation review.

**Formal task completion: BLOCKED pending a candidate commit.** The invoked
skill requires unambiguous base and candidate commits. The maintainer explicitly
requested review while R2 remains uncommitted, so this report assesses the
hashed snapshot without creating a commit or claiming committed acceptance.
The missing identity is separate from the substantive source result.

## Subject and independent method

Base/HEAD: `98a58443afac1f74cfefd607f3cd429c3b1cdee1`. The reviewed difference is
`git diff HEAD --`, with tracked diff SHA-256
`b6f072f75ebfe8414ec55313e6c5f00a955212e847e13ca2a868e3bfd7505a96`.
Individual source, specification and task/evidence hashes are recorded in
[subject.md](subject.md); they were independently rechecked unchanged at the
end of source inspection.

Authority is [approved spec revision 2](../../spec.md), with
[TASK-001-R2](../task-001-r1-98a58443a-review-1/TASK-001-R2-finish-lifecycle-deadlines.md),
the prior [R1 verdict](../task-001-r1-98a58443a-review-1/verdict.md) and
[validated findings](../task-001-r1-98a58443a-review-1/findings-validation.md).
The original base remains `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
No new product authority or finding ID is introduced.

This fresh remediation reviewer read the invoked skill, root/benchmark rules,
applicable workflow, maintainer-style/Rust/agent-surface/testing references,
approved contract and Bifrost durability authority. CodeGraph preceded source
tracing; the actual working-tree diff, complete changed supervisor flow,
runner ordering, relevant callers, assertions and connected harness were then
inspected. The recorded execution summaries were checked against assertions,
not treated as proof merely because they say GREEN.

One fresh remediation reviewer covers the affected lifecycle, subprocess
cleanup and evidence-projection boundaries. There is no new production auth,
tenant SQL, SDK wire or domain-durability boundary needing another specialist.
No new finding needs separate validation. Only this report was written by the
reviewer; implementation and task evidence remain untouched.

## Closure and proof

Source locations below are relative to `crates/wyrd/wyrd-testing/src/`.

| Existing finding / scenario / authority | Independently inspected implementation and assertions | Result |
|---|---|---|
| FIND-TASK-001-2 / R2 scenario 1 / REQ-002/003, AC-003 | `Lifecycle::announce_teardown` writes the case-clock start atomically (`experiment.rs:2196–2223`); `run` announces before `workload.finish()` (`:2704–2721`). `Sweep::attempt` polls that announcement rather than inferring teardown from a final record (`capacity/sweep.rs:310–369`). Runner test at `experiment.rs:7558` reads the announcement inside finish and verifies the stop charge; stepped supervisor test at `sweep.rs:1136` has no record at 510 s, writes it at 520 s, exits at 530 s and asserts VALID with 30 s total teardown. | PASS — closed in snapshot |
| FIND-TASK-001-3 / R2 scenario 2 / REQ-002/003/005, AC-002 | `ends(began)` retains the original deadline; process stop, wrapper wait and confirmation no longer derive fresh admission deadlines (`sweep.rs:315–426`). Physical cleanup can use remaining overall envelope after phase expiry, but `cleanup.confirmed` also requires completion before the original deadline (`:399–423`). Test at `:1238` records cleanup at 555 s after the 550 s deadline as INVALID/unconfirmed with no survivors and subsequent NOT RUN. Test at `:1304` preserves two launches when invalid cases nevertheless clean up in time. | PASS — closed in snapshot |
| Shared deadline overrun and failure paths / both scenarios / REQ-002/003 | Test at `sweep.rs:1186` stops an announced overlong teardown at 560 s, retains missing final evidence and unconfirmed cleanup, and starts no next case. Existing missing-announcement/hung case and late/missing Docker tests remain with bounded external calls; actual `stop` and `confirm` callers were traced. | PASS |
| Interrupt/cutoff/exit transition ordering / root-cause correction / REQ-002/003 | Polling captures the announcement at `sweep.rs:352`; cutoff reads it before fallback. Interrupt uses existing `began`, then rereads announcement, then uses its own instant only if neither exists (`:362–366`). Process exit similarly reads announcement before fallback (`:383–386`). The chosen start is retained; none of these paths resets an existing deadline. | PASS from source structure plus executable handoff/deadline proof; exact interrupt-between-polls scheduling not separately exercised |
| Saved outcomes and continuation / REQ-005, AC-002 | The supervisor charges teardown through confirmation into both cleanup and lifecycle, retains explicit late-cleanup reasons and persists case/sweep outcomes. `supervise` at `sweep.rs:139–190` gates later launches on timely `cleanup.confirmed`. Stepped tests read saved case outcomes; the connected lane drives the actual release runner/announcement and real PostgreSQL cleanup. | PASS |

The root cause is addressed at the existing owners: `Lifecycle` publishes the
transition through its already-owned artifact directory, and `Sweep` holds one
start/deadline across the remainder of the lifecycle. The obsolete
final-record inference and renewed stop/confirmation calculations are deleted.
The `Cleanup` rustdoc change describes the new honest distinction between
physical absence and timely confirmation; it does not change a domain wire
contract or introduce another recorder.

The latest interrupt correction is source-correct: an announcement published
between polls is consulted before interruption establishes a fallback start.
A bespoke test for that precise scheduling window was not added. This is a
disclosed verification limit, not a retained blocker: real announcement writing
and pre-stop ordering, announced-start consumption, deadline preservation,
expiry rejection, saved evidence and continuation already have executable
proof, while the short terminal branch visibly preserves the same chosen
start. No additional interrupt hook or production test seam is required by
this review merely to reproduce that scheduling window.

## Executed versus recorded evidence

The parent independently ran on the frozen snapshot:

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^sweep::tests::/)'`: **10 passed**, 0 failed (0.217 s).
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::a_case_announces_teardown_before_it_stops_its_processes)'`: **1 passed**, 0 failed (0.012 s).
- `mise run fmt:check` and `git diff --check`: **PASS**.
- `mise run lints`: **PASS**, exit 0; workspace all-features/all-targets
  Clippy completed in 16.09 s and the standalone server check in 0.23 s.

The R2 task records 112 capacity tests, 27 runner tests, eight repeated sweep
group passes and `mise run test:benchmark:l1` passing once in 221 s, plus
format/lint/diff results. The real connected harness retains multiple cases,
fresh resource assertions, saved report regeneration, invalid-integrity cases
with typed reasons and confirmed cleanup. These broader results are recorded
implementation evidence, not fresh runs by this reviewer. No Cargo or Docker
command ran in this reviewer's context. The parent supplied the completed
lint result separately; it is recorded above as a fresh review check.

Unchanged R1 closures (listing, persistence duration, full snapshots, smoke
integrity and approved no-target output) carry forward: this diff changes the
phase owner and cleanup accounting but leaves their producer/consumer contracts
intact. Historical readers and L2 policy remain unchanged. No old performance
target is restored.

## Remaining formal and verification limits

There is no unambiguous candidate commit for formal completion. Once a
candidate exists, bind it to these source/evidence hashes and inspect any
intervening changes; do not automatically reuse this result for changed content.
That identity gap creates no production correction or new remediation finding.

Historical TDD disclosures remain separate and nonblocking per maintainer
direction; no historical RED reconstruction or code churn is requested.
Full-size feasibility remains UNVERIFIED under approved AC-005's execution
hold. No full-size performance or live profiler run is implied by this review.
