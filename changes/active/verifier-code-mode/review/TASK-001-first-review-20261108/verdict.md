# TASK-001 acceptance verdict

**Verdict: FIX_REQUIRED.** Ten material findings
remain. The bridge and substantial shared-runtime/public proof are implemented,
but the candidate does not yet satisfy TASK-001's approved failure, cleanup,
value-preservation and evidence contracts. Green checks include assertions
that permit the wrong timeout outcome; they are not acceptance of that outcome.

## Immutable subject and round

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
- Round: first cumulative review; no prior task-review acceptance/closure
  evidence is carried forward and no convergence-round assessment applies.
- Base: `5234961d51798581a026834cfffd5efaadc226b4`, the parent of first
  implementation commit `d57c54781`.
- Candidate: `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011` (HEAD throughout review).
- Actual range: `git diff 5234961d51798581a026834cfffd5efaadc226b4...0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
- Approved authority: [spec.md, revision 2](../../spec.md), SHA-256
  `cc1b3c37d28201f3207769901776cc34b6a73c2810e9c42c53b50c2033cd4055`.
- Original [TASK-001 with appended implementation evidence](../../tasks/TASK-001-bounded-code-orchestration.md),
  SHA-256 `494bca6fa15325ace56ffe116e45d7d4f745361c56567e5b8351af6cb9fb1521`.
- The spec/task packet was untracked at review start and remains an accepted
  local governing input. No commit/push was requested or performed. The tracked
  source and governing input hashes remained unchanged; only review reports
  and this remediation handoff were written.

This is a TASK-001 audit, not whole-change acceptance. TASK-002's authored
Python/TypeScript/MCP journeys and public guidance remain separately owned;
TASK-001's mechanical generated consumers/buildability are included. Full
navigation, source-backed specialist applicability and omission reasoning
were recorded before discovery in [scope.md](scope.md).

## Independent selection and coverage

The orchestrator did not review or validate its own findings. Each discovery
role used a fresh independent context and wrote only its assigned report;
the final validator was separate from all discovery and focused reviewers.

| Role | Scope and report | Report result / reconciliation |
| --- | --- | --- |
| behavior-rev | Complete task behavior, public flow, requirements/non-goals; [report](task-review-behavior.md) | FAIL; retained causes consolidated in the validated ledger. |
| invariant-rev | Shared state across attempts, records and cancellation/settlement; [report](task-review-invariants.md) | FAIL; overlapping lifecycle/admission causes consolidated. |
| repo-rev | All 36 changed paths, cross-tier rules, generated TS/docs, helpers and mandatory rustdoc; [report](standards-review.md) | FAIL; rustdoc retained, missing-diagnosis report concern resolved by actual focused follow-up. |
| maintainer-rev | Substantial dispatch/injection/record ownership and whole-workflow reuse; [report](maintainer-review.md) | PASS within maintainer scope after reassessment; CodeTool finding withdrawn, with no required trait replacement. |
| system-rev | Shared server/worker, dependencies, queue, failures/recovery and healthy siblings; [report](system-review.md) | FAIL; failure boundary, extraction settlement and setup deadline gaps retained. |
| domain-security-rev | RBAC, accepted identity, tenancy/delegation, worker trust, safe errors/audit; [report](domain-review-security.md) | PASS within scoped protections; no new grant/tenant bypass established. |
| domain-concurrency-rev | Atomic request allowance, shared permits, workspaces and cooperative stop races; [report](domain-review-concurrency.md) | FAIL; material cases independently traced, including unique post-halt waiter path. |
| domain-records-rev | Admission/delivery truth, failed/retried/check records, historical/custom-data and retained consumers; [report](domain-review-records.md) | FAIL; record boundaries retained, existing container preservation accepted with access-evidence limits. |
| followup-rev | Four failure-clearing fixture/assertion diagnoses; [report](followup-review.md) | RESOLVED diagnoses; new FIFO recovery proof defect separately validated. |
| independent integer validation | Validator-discovered wire-range candidate and residual supported publication path; [report](integer-validation.md) | CONFIRMED; no unsupported deep-Card claim retained. |
| independent proof equivalence | Queue/capture and mixed-origin evidence; [report](proof-validation.md) | Capture/mixed-origin extra-test blockers rejected; actual QueueFull projection mismatch sent to a separate source validator. |
| independent capacity validation | Exact downstream process overload producer→stop→queue sink; [report](capacity-validation.md) | CONFIRMED, preserving ordinary domain failures and catalog metadata. |
| fresh ponytail-rev | Every proposal, applicability/coverage, source/materiality/correction burden and acceptance reconciliation; [final ledger](findings-validation.md) | FIX_REQUIRED; ten retained findings after the CodeTool withdrawal amendment; other findings remain confirmed/revised. |

The actual task spans complex lifecycle, broad tiers, substantial shared
ownership and sensitive security/concurrency/persisted-evidence boundaries;
all applicable specialists were selected. No mandatory report is missing.
No external provider/standard change needed a source scout. No implementation
source or test was edited during review.

## Follow-up and validation decisions

[comparison.md](comparison.md) groups discovery by violated invariant, not
line number or vote. Agreement did not establish acceptance. A unique
cooperative-poll race was source-validated rather than discarded by count.
One bounded follow-up supplied independent current diagnoses for four test
adjustments. It does not invent historical logs or claim a prior diagnostic
report existed; REP-002 is closed rather than sent back for paperwork.
The follow-up found a real later FIFO recovery omission, retained as FIND-9.

The final validator's new integer-range and proof questions received separate
fresh source validation before retention. That excluded an unproved deep-JSON
counterexample, rejected standalone extra-test demands where identical-owner
composition is sufficient, and confirmed the exact Oracle QueueFull mismatch.
The validator independently accepted the supported undelivered-future record
path even after unsigned integer conversion is corrected. No reviewer
validated its own newly discovered blocker.

## Validated findings

Detailed producer/consumer traces, materiality, correction burden and exact
proof are in [findings-validation.md](findings-validation.md). Only that
ledger's confirmed/revised findings appear here or in remediation.

| Stable finding ID | Classification and required correction boundary |
| --- | --- |
| FIND-TASK-001-1 | INCORRECT: uncaught generated execution and Python time-stop become repairable model errors; preserve failure/stop through existing session/Agent/engine. |
| FIND-TASK-001-2 | INCORRECT/MISSING proof: interrupted extraction loses writer/admission and detaches from cleanup; retain actual mutation ownership and await bounded settlement. |
| FIND-TASK-001-3 | INCORRECT: initial writer/mount waits exceed the computed operation deadline; bound setup with the existing deadline/cleanup owner. |
| FIND-TASK-001-4 | MISSING: admitted permit waiters disappear on cancellation; allocate existing bounded record before waiting. |
| FIND-TASK-001-5 | INCORRECT: nested result is marked delivered before Python publication; finish at existing resumption boundary and retain honest unpublished cancellation/failure. |
| FIND-TASK-001-6 | INCORRECT: an already-spent waiter can dispatch after exhaustion is raised; enforce existing Halt after capacity suspension, before actual invocation. |
| FIND-TASK-001-8 | VIOLATION: mandatory error/panic/invariant rustdoc missing on changed production/private/test helpers; batch in-place docs with remediation. |
| FIND-TASK-001-9 | MISSING required proof: capacity recovery consumes a leftover empty final answer, not the appended program; correct FIFO script and assert actual execution. |
| FIND-TASK-001-10 | INCORRECT: supported unsigned JSON integers are rejected; preserve i64::MIN..u64::MAX exactly with installed Monty integer transport. |
| FIND-TASK-001-11 | INCORRECT: Oracle QueryQueueFull is catchable and fails to defer queued execution; recognize that exact catalog refusal in existing Halt while preserving the problem. |

These are implementation/gate failures within approved revision 2. None
requires a new public contract, concurrency policy, runtime/store/transport,
dependency, migration, or specification revision. They are packaged into
one [TASK-001-R1 remediation](TASK-001-R1-restore-execution-and-evidence.md)
for direct handoff to `$wyrd-implement`. This review does not start that
implementation or alter the original task's status/evidence.

## Reconciled acceptance matrix

The following matrix is taken from the independently validated source ledger;
PASS rows are scoped evidence, not a blanket task PASS. Implementation and
proof which remain correct are retained as acceptance context for remediation.

| Requirement / AC / constraint | Reconciled implementation evidence | Verification evidence | Recommendation |
| --- | --- | --- | --- |
| REQ-001–004, AC-001–003; step/check orchestration, direct catalog, only declared dotted targets, no recursion | One resolved Dispatch, hosted protected Invocation and Monty callable; authored entrypoint passes no bridge | Independent Rust reduction journey; Vala bridge; inspected actual-worker boundary | PASS except supported conversion failure under FIND-10 |
| REQ-005, AC-003/011; exact supported JSON, bounded pre-dispatch/result conversion | Existing capped encoding/converter retains ordinary values/refusals, but unsigned range is lost | Independent converter test passes wrong u64 refusal; supported Card path and focused independent report contradict it | FAIL FIND-10; delivery consequence FIND-5 |
| REQ-006–007, INV-001/002/004; AC-004/017; accepted authority, grants, tenant, audit and Oracle | Both origins use same Invocation authorization/capability owners and statement-specific caller; no worker identity selector/new audit owner | Independent public denial/recovery; inspected recovered/delegated and existing queue authority tests; unchanged permission/audit owner source | PASS authority reuse; exact QueueFull control failure is FIND-11, not an authorization finding |
| REQ-008; caught half AC-005 | Structured safe owner problem retained, catchable repair inside Python | Existing real worker/domain denial repair and public permission journey | PASS; preserve during host-stop correction |
| REQ-009, AC-005/009/010/018; uncaught versus host stop semantics | Sandbox timeout/program error loses category; exact downstream QueueFull misses Halt; Busy/budget typed paths otherwise work | Independent passing public timeout is a source-proven counterexample; capacity-validation traces real overload producer | FAIL FIND-1/11; misleading recovery proof FIND-9 |
| REQ-010, AC-006/014; atomic retry-spanning ceiling/final allowance/no refund | Engine ToolCalls outside retry loop and atomic bounded spend are sound; post-wait stop boundary absent | Independent Vala budget tests and public budget phase; concrete cooperative-poll ordering | PASS ceiling/final allowance; FAIL no-later-work FIND-6 |
| REQ-011, AC-007; shared cap, cap1, association | All direct and nested operations hold the identical Dispatch semaphore; code holds none; call_id drives futures | Runtime peak/cap1/2 and hosted gather; static one-owner proof, not a claimed simultaneous mixed-origin execution | PASS ordinary shared cap; FAIL detached active extraction under FIND-2 |
| REQ-012, AC-009/018; wall-clock setup/waits/retries and all-exit cleanup | Reservation accounting and async query cancellation/drop are sound; initial writer/setup is unbounded and started extraction detaches | Held-query cancellation/source and worker lifecycle proof; missing extraction barrier is a concrete failing guarantee, not just absent test name | FAIL FIND-1/2/3/6; accounting itself PASS |
| REQ-013, AC-008/011; scratch, awaited visibility, no-overwrite | Same Workspace writer/room/persist_noclobber owner; normal awaits work; interrupted extraction releases guard early | Inspected real-worker mutation proof and unchanged mounted-write regression | PASS normal behavior; FAIL interruption FIND-2 |
| REQ-014, AC-001/012/016; intermediate visibility/reduction | Children bypass model conversation; outer bounded value alone is installed in loop | Independent 20-note real-client/worker comparison, matching leaf evidence, cited judgment/data, model byte assertions | PASS; no raw-result store introduced |
| REQ-015–017, INV-006, AC-005/009/012; once-only honest admitted records, failure/retry/check retention | Engine settlement preserves existing completed/unfinished slots and custom-data calls, but waiter has no slot and child finalizes before publication | Independent failed/inconclusive/retry tests; supported missing-slot and unpublished-future paths | FAIL FIND-4/5; existing result handoff otherwise PASS |
| REQ-016–017/AC-013 historical/additive/default/custom-data contracts | Closed recipient/default model, optional-empty requirement calls, decode_data transfers list; Rust export/generated TS agree | Independent wire round-trip plus real immediate/retained equality and server build | PASS task's mechanical contract scope; authored other-language/MCP delivery remains TASK-002 |
| REQ-017 privacy/no alternate argument surface | Records remain in existing outcome/StoredOutput, summary remains payload-free; no new record-bearing summary/debug surface | Records/proof reviewer source traces. Sensitive metadata is a declaration, not universal proof of elevated enforcement | PASS preservation/no-new-surface within task; no invented outcome-disable mode or unrelated access-control audit claimed |
| REQ-018, AC-001/011/016; original evidence/Verdict integrity | Existing verdict/citation/reachable pinned-evidence owners retained; scratch/live reads gain no pinning authority | Independent reduction judgment/citation/data/leaf equivalence and retained readback; existing malformed/citation seams | PASS except execution failure must not reach judgment under FIND-1/11 |
| AC-014 startup settings | Existing checked scaled parser with named setting refusal; positive defaults 128/64KiB, no Card override | Independent valid/invalid/default config test | PASS |
| INV-003/005, AC-015, non-goals and portable isolation | Existing pool/interpreter/allocator/mounts; authored code has no bridge; no dependency/feature/migration/OS isolation/new runtime | Actual-worker negative tests, source/diff and build; no platform matrix rerun claimed | PASS |
| Required governing authority correction and no unrelated changes | Changed Verifier/security paragraphs reflect approved portable mediated execution; mechanical constructors/generated outputs fit new fields | Cumulative diff and specialist coverage | PASS |
| Repository ownership/rustdoc gates | CodeTool is accepted after reassessment; listed changed items still lack required sections | Amended maintainer report and changed-item rustdoc audit | FAIL FIND-8 only |


## Verification and evidence limits

The orchestrator independently ran **10 selected tests**, all passing, plus
the real server-worker build and cumulative diff whitespace check. Exact
commands, counts and nextest run IDs are in [verification.md](verification.md):
startup/conversion (2), Vala shared records/budget/bridge (3), Rust public
reduction/permission/failure journeys (3), wire round-trip (1), Busy sibling (1).
The public timeout test currently asserts the wrong successful outcome; its
GREEN result therefore corroborates FIND-1 rather than closing it.

The original task reports sandbox, broader Vala/wire/story, codegen,
dependency/tenant, format/lint/docs checks passing. Those unrerun lanes remain
reported evidence, not newly claimed executions. No broad gate, platform
matrix, new extraction barrier, mixed-origin experiment, live overload or
new failing-path test was run during this immutable review. Missing tests
were not automatically findings: source-proven required behavior and
non-weaker proof were evaluated independently.

Two proof assumptions do not add remediation work. There is no existing
verification outcome-disable selector; ContextCapture governs requirement
actual values, so inventing a new toggle would exceed scope. The original
private proof wording should be corrected through `$wyrd-plan` to use the
actual existing-container/no-alternate-surface proof. Sensitive table metadata
alone does not establish universal elevated access enforcement; this audit
claims preservation of existing owners, not a generic Oracle authorization
verdict. Likewise, the literal simultaneous direct+nested cap test is absent,
but the identical Arc<Dispatch>/semaphore plus cap1/2 runtime proof is
non-weaker structural evidence. Queue composition is valid for typed Busy;
it does not close QueueFull's different structured projection.

## Next review boundary

Implement the single remediation task, retaining the original task, this
verdict/ledger and candidate identity. The next task review checks each stable
finding's closure, the diff from this candidate to the new immutable candidate,
and affected source/callers/consumers/tests. Carry forward unaffected passing
rows unless new dependency or source evidence invalidates them. Do not
restart the whole discovery by default.

Source, spec and original task were unchanged at closeout. No commit, push,
merge, deployment, task PASS or completion has been performed.

## CodeTool finding withdrawn

FIND-TASK-001-7 (MNT-001) is withdrawn at the maintainer's request following
source reassessment. `CodeTool` is accepted as the cross-tier execution seam;
no callback replacement is required. The original report's production-only
interpretation was not stated by the repository rule, and no technical benefit
from removal was established. See the amended [maintainer report](maintainer-review.md).

The active finding IDs are 1–6 and 8–11, with their original meaning and
numbering preserved; ID 7 is reserved as withdrawn. Ten findings remain and
the verdict stays FIX_REQUIRED. The reviewed source and approved spec did not
change, and no new test result is claimed by this amendment.
