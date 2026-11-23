# TASK-001-R1 independent remediation validation

Recommendation: **FIX_REQUIRED**. Four prior findings close; two existing
lifecycle findings remain incomplete: **FIND-TASK-001-2 and FIND-TASK-001-3**.
The approved removal of old targets is implemented. No new finding or product
requirement is introduced. TDD history is recorded separately and does not
cause this verdict.

## Immutable subject and review method

- Original base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Prior reviewed candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
- Current candidate: `98a58443afac1f74cfefd607f3cd429c3b1cdee1`.
- Implementation starts `be099fd8e12a5054fc38e916817bcb52d842ecec`, based on
  `19fd9cfcf8a112c6d92440c9168bb03c8f838229`, ten commits through candidate.
- Direct remediation diff: `git diff 1bcc403b 98a58443a`; implementation-only
  diff: `git diff 19fd9cfcf 98a58443a`; cumulative original range remains
  `git diff 2418dc78...98a58443a`.
- Latest change since the blocked attempt: `git diff f436ada92 98a58443a`.
  It adds committed task evidence and a `match` to `if let` Clippy correction
  at the stop path, without changing its timing semantics. Ancestry is retained.
- Authority: [approved spec revision 2](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md), original
  [validated findings](../task-001-1bcc403b-review-1/findings-validation.md),
  and [TASK-001-R1 with committed execution evidence](../task-001-1bcc403b-review-1/TASK-001-R1-l1-protocol-boundaries.md).

This fresh remediation reviewer independently checked prior closure, the actual
changed owners and callers, affected assertions and the now-committed execution
claims. The previous blocked review's observations were provisional; they were
not treated as established findings. Current source was traced directly against
the pinned commit, including the entire supervisor deadline/stop/confirmation
path and the child runner's record-write ordering.

Read the invoked skill, root/benchmark rules, applicable spec-driven workflow,
maintainer-style/Rust/agent-surface/testing guidance and Bifrost durability
authority. CodeGraph preceded code tracing; missing focused bodies were read
from the pinned commit. No source or implementation evidence was modified.

## Closure and coverage handoff

Locations are relative to `crates/wyrd/wyrd-testing/src/` unless qualified.
The original requirements and scenarios remain the coverage identifiers;
this table does not add obligations. Execution results below are recorded
implementation evidence, not fresh runtime execution by this reviewer.

| Existing finding / scenario / authority | Independently inspected implementation and proof | Result |
|---|---|---|
| FIND-TASK-001-1 / 1 / REQ-001, AC-001 | `cases.rs:46–73` and `scripts/benchmarks/l1.sh:21–47` consume one TSV registry. `main.rs:1015–1068` copies only script/registry into an isolated checkout without driver, compares all category output to the Rust registry, and proves Cargo/mise stubs were never called. Exact focused command and GREEN recorded in task. | PASS — closed |
| FIND-TASK-001-2 / 2 / REQ-002/003, AC-003 | Runner stop now uses `Lifecycle::teardown_left` and charges teardown; `experiment.rs:7509` covers normal/overlong stop and slow drain. Supervisor `sweep.rs:303–336` still permits post-510 teardown only if its final engine record already exists. Tests at `sweep.rs:1047,1237` prewrite engine evidence and omit the valid unrecorded-teardown path below. | FAIL — incomplete closure |
| FIND-TASK-001-3 / 3 / REQ-002/003, AC-002 | Docker calls use absolute timeout and kill-on-drop; `confirm` returns at exhaustion, and `sweep.rs:1184,1284` exercises hung and late Docker responses. But admitted wrapper exhaustion renews stop/confirmation at `sweep.rs:347–349,382–388`, so later absence can still authorize continuation. | FAIL — incomplete closure |
| FIND-TASK-001-4 / 4 / REQ-003/005, AC-004 | Measured write duration survives in `SweepEntry.persist_seconds`; `CaseRecord::persisted` at `summary.rs:171–187` hydrates the full reader from the saved adjacent sweep. `sweep.rs:1089,1352` asserts retained duration and unsaved/late-save reference outcomes. The timing is honestly measured outside its own case write. | PASS — closed |
| FIND-TASK-001-6 / 4 / REQ-005/006, AC-004 | `Sweep::attempt` reads retained artifacts via `Record::read_with_timings`; `CaseRecord::timings_of/hydrate` preserves complete snapshots through case JSON. `sweep.rs:1089–1174` records actual timing/scan/counter/gauge series, crosses the handover/persistence boundary, removes artifacts, then asserts full report labels, values, hydrated snapshot equality and identical sweep output. | PASS — closed |
| Human-approved target removal / 5 / revised REQ-004/006, AC-003/004 | `judge_criterion` at `experiment.rs:3325–3336` makes case performance targets inapplicable without changing historical/L2 judgment; full-mode sample/integrity guards remain. `experiment.rs:7607` and `main.rs:877` cover aggregate readings, missing samples/errors and ordinary/profiled `none` target output. Mise/README migration inspected. FIND-TASK-001-5 remains withdrawn. | PASS |
| FIND-TASK-001-7 / 6 / REQ-003/007, AC-002 | Case smoke retains `Over::Every` integrity judgment; only legacy non-case smoke skips it. `experiment.rs:7666` checks detected errors become INVALID. Connected `pg_tests.rs:893–938` drives two real Gateway cases through normal supervision, checks saved engine reasons by `criterion == no-errors`, cleanup confirmation and INVALID regenerated rows. Task records exact focused and connected GREEN commands. | PASS — closed |
| Unchanged assigned requirements | Prior source-backed membership/order, declaration independence, ratio-of-sums/pooling, window-stop/late populations, fixed report units/order and historical compare/calibration guards remain valid; changed criterion/record seams were rechecked above. No other workload is reopened. | PASS carried forward, subject to the two lifecycle failures |
| AC-005 | Full-size feasibility remains explicitly unverified under the approved execution hold. No full-size runtime result is required during that hold. | PASS for disclosure; feasibility deferred |

The connected task result is recorded as one passed lane in 284 seconds;
capacity is 110 passed/3 skipped and runner is 26 passed. Assertions support
the four closed findings and target-removal scope. Their success cannot prove
the two paths the tests do not drive. No extra runtime run is needed to
establish the direct control-flow violations below.

## Retained findings

### FIND-TASK-001-2 — INCORRECT: valid unrecorded teardown is still cut off at 510 seconds

Disposition: **CONFIRMED incomplete closure of the existing finding**, not a
new requirement. REQ-003 separately grants setup 240 s, warmup 30 s,
measurement 90 s, drain/capture 150 s, teardown 60 s and persistence 20 s.
Neither the spec nor the linked lifecycle design requires process teardown to
finish and the engine record to exist by the 510-second pre-teardown boundary.

Location: `bin/capacity/sweep.rs:303–336`, specifically the record-dependent
extension at `:320–323`; `run_case` writes its engine record at `:772–784`
only after `experiment::run` awaits `workload.finish()` at
`experiment.rs:2673–2679`.

Starting conditions and trigger: a valid full case spends 240 s setup,
30 s warmup, 90 s measurement and 140 s drain/capture, then starts process
teardown at 500 s. Its SDK/server processes need 20 s to stop, and its
PostgreSQL wrapper needs a further 10 s. All individual phases and the total
permit completion at 530 s, with persistence still within its own allowance.

Expected: allow the legitimate process teardown to complete and write its
engine record at 520 s, dispose PostgreSQL by 530 s, then persist the result.
Actual: at 510 s the final engine record does not exist, because the child is
still inside its permitted stop. `teardown_left` returns `None`, so the
supervisor records a missing-record deadline failure and stops the process
group. The new extension only fixes cases that already wrote their record.

Proof/materiality: this is the actual production call order for a slow owned
process stop, a phase the contract explicitly allows. It can falsely reject
full fixtures approaching their accepted envelope. The task's 490 + 10 + 20
example and both new supervisor tests use a prewritten record; neither tests
a child entering allowed teardown before 510 s and producing its record after
510 s. Passing them does not establish the missing boundary.

Smallest correction: make the existing child/supervisor phase ownership permit
allowed process teardown independently of a final record written after it.
Keep every approved allowance and the total; do not add renewed grace or shift
teardown back into drain. Add controlled proof where the record appears after
510 s because a valid stop is still running, plus a genuine phase overrun that
remains invalid. No new general lifecycle framework or production behavior is
required.

### FIND-TASK-001-3 — INCORRECT: admitted teardown expiry renews cleanup confirmation

Disposition: **CONFIRMED incomplete closure of the existing finding**.
REQ-002 forbids later launches when cleanup is unconfirmed within the teardown
allowance; REQ-003 retains a shared 60-second teardown allowance, records
external-service stop failures, and retains the total envelope. Bounding each
Docker call fixes one original path but does not waive the deadline that
applies to a previously admitted wrapper.

Location: `bin/capacity/sweep.rs:317–349,371–388`; next-case admission depends
on `record.cleanup.confirmed` at `sweep.rs:181–185`.

Starting conditions and trigger: a case completes pre-teardown work at 490 s,
spends 10 s stopping its processes and writes its engine record at 500 s.
Its PostgreSQL wrapper remains alive. At 510 s the record accounts for 20 s
of teardown already spent, so the supervisor admits the remaining 40 s,
until 550 s. The wrapper is still alive at 550 s, but exits five seconds
after TERM; immediate Docker checks then report no surviving resources.

Expected: the shared teardown allowance expired at 550 s; retain INVALID with
timely cleanup unconfirmed and mark subsequent selected cases NOT RUN. Later
physical absence can be recorded without authorizing a next launch.
Actual: admitted expiry sets a new `stopping` instant and computes stop time
as `(now + teardown).min(envelope.max(now))`, granting up to 570 s rather
than retaining the exhausted 550 s deadline. Confirmation reuses that renewed
deadline. Absence observed at 555 s can therefore yield an empty survivor list
and `cleanup.confirmed = true`. Lifecycle overrun can invalidate this row,
but supervision still admits the next case because it checks that true flag.

Proof/materiality: both the supervisor deadline extension and renewed
stop/confirmation calculations were followed against the pinned commit.
The existing hung-Docker test runs out the overall envelope; the late-answer
test uses normal wrapper exit with 0.5 s left. Neither drives an admitted
wrapper past its own teardown deadline and then lets it finish during the
renewed stop allowance. This is the original explicit late-confirmation and
continuation defect, not speculative cleanup hardening.

Smallest correction: carry the actual already-started teardown deadline through
timeout, stop and confirmation, charging process and wrapper time already
spent. Distinguish later physical cleanup from confirmation within allowance.
Add a controlled admitted-wrapper timeout that cleans up during the would-be
renewed interval, asserting saved INVALID/unconfirmed cleanup and NOT RUN
later rows; retain in-budget continuation. Reuse the existing supervisor and
cleanup owners without introducing a global outage mechanism.

## Evidence, diagnostics and limits

The committed task now contains in-place proof, exact focused commands,
reported RED/GREEN results and final formatting/lint/diff/capacity/runner/
connected results. These are valid recorded execution inputs and were checked
against actual assertions and producer-to-consumer paths; this reviewer did
not independently rerun Cargo or Docker. Parent coordinates execution while
external Cargo work is active. Do not describe recorded results as fresh
review passes.

The three diagnoses were also checked as claims. `confirm` now returns at
`now >= deadline`, explaining and correcting the zero-wait loop. Reserving
stop time inside the total closes the cited total-envelope issue but leaves
the two lifecycle paths above. The connected assertion correctly reads the
typed criterion field, since reason messages omit the criterion ID; no runtime
change was required for that assertion repair. Diagnostician agreement is not
used as acceptance proof.

One fresh remediation reviewer is sufficient for the bounded previously
assigned boundaries; no new auth, tenant SQL capability, domain durability or
SDK/wire seam is changed. No material missing specialist expertise was found.
No new finding requires another validator. The prior aborted attempt was
BLOCKED for concurrent source mutation, not a completed failed remediation
round; this is the first completed remediation audit, so no two-failure
convergence assessment applies.

TDD remains historically **VIOLATED** as disclosed. Per maintainer direction it
does not block acceptance or create repair work; no historical RED or
production churn is requested. Full-size feasibility remains **UNVERIFIED**
under approved AC-005. Only this report was written by this reviewer, and the
candidate source stayed unchanged during this audit.
