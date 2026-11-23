# TASK-001-R1 remediation validation

Recommendation: **BLOCKED — reviewed source changed during validation**.
The checkout initially matched candidate `f436ada92`, but a later identity check
reported a tracked modification to
`crates/wyrd/wyrd-testing/src/bin/capacity/sweep.rs` while HEAD remained unchanged.
The parent independently observed the same mutation and stopped source review.
The invoked skill requires BLOCKED when the reviewed source changes; no final
closure verdict or remediation may rely on this mixed checkout. Resume with a
stable, explicitly identified candidate after the concurrent writer finishes.

The provisional analysis below records the observations reached before the
mutation was detected. Direct commit-to-commit diff observations are pinned
to the stated commits; working-tree source locations/assertion observations
must be rechecked against the resumed candidate. They are not a completed
finding validation or an acceptance result. In particular, the suspected
incomplete closure of FIND-TASK-001-2 and -3 remains provisional. Finding 5
remains withdrawn under approved revision 2. No new product requirement or
finding ID is introduced.

## Subject and authority

- Original base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Prior reviewed candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
- Current candidate: `f436ada92e367f5c279b9485250b0c61d7f065fd`.
- Implementation range: `19fd9cfcf8a112c6d92440c9168bb03c8f838229..f436ada92e367f5c279b9485250b0c61d7f065fd`
  (eight commits beginning `be099fd8e12a5054fc38e916817bcb52d842ecec`).
- Reviewed direct remediation diff: `git diff 1bcc403b f436ada92`;
  implementation-only diff: `git diff 19fd9cfcf f436ada92`. Ancestry is retained;
  no patch correspondence inference is needed.
- Approved [spec revision 2](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-standardize-l1-benchmark-sweeps.md), prior
  [verdict](../task-001-1bcc403b-review-1/verdict.md), prior
  [validated findings](../task-001-1bcc403b-review-1/findings-validation.md) and
  [TASK-001-R1](../task-001-1bcc403b-review-1/TASK-001-R1-l1-protocol-boundaries.md).

Read the invoked skill, root and benchmark AGENTS, relevant workflow,
maintainer-style/Rust/testing references and Bifrost durability authority.
CodeGraph preceded source tracing. Relevant owners remain the category registry,
shell entry point, sweep supervisor, experiment lifecycle/judge, saved-record
reader and connected benchmark harness. No reviewed source or task was edited.

The fresh remediation validator covers changed declarations, consumers,
serialization, criterion semantics, deadline/cleanup paths, relevant sibling
callers and proof. Separate specialists are unnecessary for this bounded
continuation of already validated owner boundaries: no production auth,
tenant SQL, domain durability or SDK wire behavior changes. Unchanged
measurement/aggregation and historical reader evidence is carried forward
only where the changed criterion and record seams preserve it.

## Provisional prior finding closure observations

Locations below are relative to `crates/wyrd/wyrd-testing/src/`, except where
another path is given. Runtime evidence limits are stated separately.

| Prior finding / scenario / existing authority | Source and inspected closure assertions | Closure |
|---|---|---|
| FIND-TASK-001-1 / scenario 1 / REQ-001, AC-001 | `bin/capacity/cases.rs:46–73` compiles the same `cases.tsv` that `scripts/benchmarks/l1.sh:20–49` lists with awk. `main.rs:1015` tests all categories in a checkout containing only script/registry with Cargo/mise call-recording stubs and exact parity; parent independently ran all three actual mise listing entry points successfully with 5/5/18 routine counts. | PASS |
| FIND-TASK-001-2 / scenario 2 / REQ-002/003, AC-003 | `experiment.rs:2215,2673–2695` now charges and bounds process stop against teardown; `experiment.rs:7509` distinguishes stop, slow stop and real drain. `sweep.rs:300–337` admits extra wrapper time only when an engine record already exists at the 510 s cutoff; `sweep.rs:1048,1238` tests prewritten records, leaving the legitimate unrecorded teardown path below. | FAIL — incomplete closure |
| FIND-TASK-001-3 / scenario 3 / REQ-002/003, AC-002 | Docker output/removal calls now have shared absolute deadlines and kill-on-drop at `sweep.rs:532–650`; `sweep.rs:1185,1285` exercises hung and late checks with no next launch. But the admitted-timeout stop path at `:347–349,383–389` starts another allowance beyond the already consumed teardown and can return cleanup confirmed. | FAIL — incomplete closure |
| FIND-TASK-001-4 / scenario 4 / REQ-003/005, AC-004 | `SweepEntry.persist_seconds` and `summary::entry` retain the measured write duration. `CaseRecord::persisted` recovers it from the neighboring saved sweep for `report/read_case`; this honestly avoids claiming self-inclusive write timing. `sweep.rs:1090,1352` asserts rendered duration, late saved reference/duration and unsaved missing reference/duration. | PASS |
| FIND-TASK-001-6 / scenario 4 / REQ-005/006, AC-004 | Supervisor uses `Record::read_with_timings` at `sweep.rs:360`; `CaseRecord::timings_of/hydrate` saves and restores all measured repetition snapshots in the new case protocol. `sweep.rs:1090` records timing/scan/counter/gauge evidence through the recorder, child JSON/artifact-reader/saved-case path, deletes artifacts and asserts retained windows/values/full report. | PASS |
| FIND-TASK-001-5 / revised scenario 5 / revised REQ-004/006 | WITHDRAWN by maintainer decision, not restored. `experiment.rs:3325–3330` makes L1 performance criteria inapplicable after evidence/sample checks; ordinary/profiled saved-row assertions at `main.rs:877` and runner assertions at `experiment.rs:7607` retain aggregate readings, invalidate bad evidence and display Target `none`. Mise descriptions and README match. Historical/L2 criterion interpretation is unchanged because this branch requires a case aggregate. | WITHDRAWN; revised scenario supported |
| FIND-TASK-001-7 / scenario 6 / REQ-003/007, AC-002 | `experiment.rs:3325–3338` exempts only legacy non-case smoke, allowing L1 `Over::Every` checks. `experiment.rs:7666` proves healthy versus numeric-error smoke judgment; `tests/pg_tests.rs:893–938` drives two actual Gateway cases through the normal supervisor under overload, reads saved records and asserts INVALID specifically with a `no-errors` criterion reason, confirmed cleanup and both rows retained. | PASS |

## Provisional incomplete-closure observations

### FIND-TASK-001-2 — INCORRECT: a legitimate process teardown without a record is still cut off

**Authority:** REQ-003 grants separate phase allowances (setup 240 s, warmup
30 s, measurement 90 s, drain/capture 150 s and teardown 60 s), not a requirement
to finish process teardown and write the engine record by 510 s. REQ-002 and
the linked lifecycle design place case-process disposal in teardown.

**Location:** `bin/capacity/sweep.rs:300–337`, especially its conditional
`teardown_left(&record_path, allowances)` at `:322`; record creation occurs
only after `run(...).await` in `sweep.rs:773–785`, while `experiment::run`
awaits `workload.finish()` at `experiment.rs:2677` before returning.

**Starting conditions and trigger:** a full case spends 240 s in setup,
30 s in warmup, 90 s in measurement and 140 s in drain/capture, totaling
500 s before process teardown. Its SDK/server processes take 20 s to stop,
then PostgreSQL cleanup takes 10 s. The planned lifecycle is 530 s, every phase
is within allowance, and persistence can still fit its own 20 s.

**Expected:** let the valid process teardown complete, write the engine record
at 520 s, complete PostgreSQL cleanup by 530 s and retain a valid attempt.

**Actual:** at 510 s the child is still inside its permitted process stop and
has not written the engine record. `teardown_left` returns `None`; the
supervisor records a missing-record deadline error and stops the process group.
The newly added grace only applies when a record is already available. It
therefore fixes the prior review's early-record example without fixing the
underlying premature-cutoff boundary.

**Proof and materiality:** this is the production call sequence, not a
conceivable malformed record. The new runner test proves process stop can use
up to 60 s, and the supervisor's test at `sweep.rs:1238` prewrites a completed
record before starting its virtual clock, so it cannot catch this valid
unrecorded stop. Full fixtures approaching the allowed pre-teardown budget can
still be rejected despite fitting the approved envelope.

**Smallest correction:** coordinate existing child/supervisor phase knowledge
so allowed process teardown does not depend on a record written after it.
Preserve bounded admission, each phase's allowance and total envelope; do not
invent a new grace or extend drain. Add controlled proof where the child
reaches teardown before 510 s but produces its record after 510 s and remains
valid when all phases fit, alongside a genuine overrun that remains invalid.

### FIND-TASK-001-3 — INCORRECT: exhausted wrapper teardown renews stop/confirmation time

**Authority:** REQ-002 stops the sweep if cleanup is unconfirmed within the
teardown allowance. REQ-003 keeps teardown bounded to 60 s and records
overruns/external-service failures. This is the original finding's shared
deadline/late-confirmation requirement.

**Location:** `bin/capacity/sweep.rs:317–349,371–395`, with next-case admission
at `sweep.rs:179–184`.

**Starting conditions and trigger:** a case finishes 490 s of pre-teardown
phases and 10 s process stop, keeping its record at 500 s. Its PostgreSQL
wrapper does not exit normally. At the 510 s checkpoint, the record accounts
for 20 s total teardown so far; the code admits the remaining 40 s, until 550 s.
The wrapper still has not exited at 550 s, but after TERM takes another 5 s
to stop, and immediate Docker checks report all resources removed.

**Expected:** teardown's shared 60 s expired at 550 s. Keep the attempt INVALID
and cleanup unconfirmed within allowance, then mark later cases NOT RUN even
if resources are subsequently observed absent.

**Actual:** on the admitted timeout the code sets `stopping = now` and calls
`stop` with `(now + teardown).min(envelope.max(now))`. At 550 s this grants a
new deadline of 570 s. Confirmation uses the same renewed deadline, rather
than the consumed lifecycle deadline. At 555 s `confirm` can return empty,
setting `cleanup.confirmed = true`. The lifecycle overrun makes the current
row INVALID, but `supervise` checks only the true cleanup flag before admitting
the next case. Thus this still permits continuation after timely cleanup was
not confirmed.

**Proof and materiality:** the newly bounded Docker calls close the original
indefinite-await path, and missing-engine termination shares one deadline, but
the admitted-record timeout path still renews it. Existing tests use a hung
Docker client through the end of the envelope or test remaining time after a
normally exited wrapper; neither lets teardown exhaust, then complete during
the renewed grace. This supported external-service stop failure remains an
explicit recovery/isolation boundary, not a request for harder cleanup policy.

**Smallest correction:** carry the actual already-started teardown deadline
through timeout, stop and confirmation, subtracting time already spent in
engine and wrapper teardown. Preserve a reason distinguishing later physical
cleanup from confirmation within allowance. Add a controlled case with an
admitted record whose wrapper responds only after its teardown deadline and
assert saved INVALID/unconfirmed cleanup and NOT RUN subsequent rows, while
in-budget cleanup still permits continuation.

## Evidence, process and carry-forward limits

The handoff reports 110 capacity tests, 26 experiment tests and the connected
`mise run test:benchmark:l1` lane passing at these commits. I inspected the
actual assertions and connected producers/consumers; these are user-reported
execution claims rather than fresh passes by this validator. The parent
independently ran each actual category listing task successfully, obtaining
Verification 5 routine/9 targeted, Gateway 5/3 and Bifrost 18/25, without a
driver build. Parent also reports `mise run fmt:check` and diff checks passed.
At report creation, other Cargo processes are active, so the parent is
coordinating remaining focused execution and lint evidence sequentially.
No Cargo, Docker or full-size performance command ran in this validator.

Final identity check detected the source mutation described above. Source
tracing stopped immediately; the report was not reconciled with the writer's
changes, and no runtime result is claimed for a final immutable candidate.

The implementation task's scenario entries have not yet been populated with
the new evidence, as disclosed by the user. This review does not invent missing
execution history or infer another code defect from that omission. Final
reports must retain exact execution limits and pending required checks; runtime
proof for the remaining broken timing paths is still absent.

Unchanged exact category membership/order, selector ambiguity handling,
declaration independence, ratio-of-sums/pooling, stop/late population evidence,
full sweep ordering/units and protocol comparison/calibration guards retain
their prior source-backed evidence except where the timing and snapshot seams
above were rechecked. This review does not reopen unrelated workloads or
introduce old performance targets.

TDD compliance is **VIOLATED**. Preserve the original scenarios 1–4 violation
and the two disclosed remediation gaps: recorded-case extra teardown was coded
before its regression test, and the connected negative smoke proof was added
after the smoke-judging fix. No historical RED reconstruction or production
churn is requested. Future closure should proceed test-first.

Full-size feasibility remains **UNVERIFIED** under the approved AC-005 hold.
No full-size qualification or real profiler run is implied by short harness
proof. This is the first remediation round, so no two-round convergence
assessment applies. Corrective scope remains the two original lifecycle
findings; no material specification ambiguity has been found.
