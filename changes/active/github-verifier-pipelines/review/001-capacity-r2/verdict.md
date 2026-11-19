# Task 001 first remediation verdict

**FIX_REQUIRED** — FIND-001-2 remains open; FIND-001-6 and FIND-001-7 are new. FIND-001-1, FIND-001-3, FIND-001-4 and FIND-001-5 are closed.

## Immutable subject and review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-factory-worktrees/code-review-workflow-github-verifier-pipelines`.
Original base `395274d58bf76a2075f245ca3a5c1bd044925027`; prior candidate `34fb45bb387582d2aedfee77fb4e09fa2753b80d`; candidate `ea4460c3c97adb55395acd50f062658bc8d0d4cd`. The actual incremental review is `git diff 34fb45bb3 ea4460c3c`, with cumulative `git diff 395274d58...ea4460c3c` retained as context. Ancestry is linear; HEAD and tracked source remained immutable.

Original task `changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md`, spec revision 3 `changes/active/github-verifier-pipelines/spec.md`, prior `../001-capacity-r1/verdict.md` and `findings-validation.md`, and remediation `../001-capacity-r1/001-R1-close-capacity-review.md` including both human-approved revisions remain the acceptance authority. The revision blocks supersede prior default/config restrictions only for their named settings. No review finding ID is retroactively assigned to a human revision.

This is the first remediation review, limited to prior-finding closure, changed source/consumers and the R/H obligations. `scope.md` records the common navigation and selection. Prior r1 source-backed lane/engine ownership, mode/reclaim routing, accepted identity/evidence/tenant/subject, persisted claim/checkpoint/deadline, eligibility and excluded scope carry forward where unchanged. Changed config projections and duration/cleanup were reviewed against those seams. The relocated proof and changed recovery ordering were rechecked, not merely carried. No full discovery restart or two-unsuccessful-round convergence assessment applies.

## Independent coverage and follow-ups

| Reviewer | Report | Result and coverage |
|---|---|---|
| Targeted repo-rev | standards-review.md | FAIL: changed server/config/SDK tests/docs/generated parity and mandatory rustdoc; retained proposals on configuration representation and dispatch panic docs. |
| Targeted maintainer-rev | maintainer-review.md | FAIL: config resolution and existing owner reuse, relocated fixture, gateway/recovery deltas; retained precedence proposal. |
| Targeted system-rev | system-review.md | FAIL: changed admission/timeout/failure/settlement paths; retained incomplete original call bound. Its informational per-worker arithmetic is corrected in the independent validation (384 MiB each; 6144 MiB total). |
| Focused deadline follow-up | followup-review.md | RESOLVED: original absolute deadline and reversible preparation are sufficient to prevent late dispatch without blanket cancellation, abandoning durable admission, or changing settlement-before-answer. |
| Focused config-timeout follow-up | followup-config-timeout.md | RESOLVED/CONFIRMED: validator-originated deadline representation path independently source-validated and SQL failure reproduced. |
| Fresh ponytail-rev | findings-validation.md | FIX_REQUIRED: independently traced proposals and corrections, closed prior findings, reconciled R/H/O matrix, verified specialist applicability and carry-forward. |

All required selected reports are complete. The unchanged shared lane/concurrency and persistence/identity boundaries have specific r1 source coverage; the changed call wait/expiry/settlement path has fresh system and validation coverage. No new shared synchronization/permission/SQL schema/durable handoff or SDK/UI language contract required additional domain discovery. The targeted specialists are justified by new operator config, changed cross-tier proof/ownership and material failure paths. The fresh validator confirms these selections and omissions.

`claim-comparison.md` records distinct scenarios and grouping. One focused follow-up resolved a concrete correction conflict (blanket timeout would alter outcome/settlement). A separate focused source validation was required for the validator's new queued timeout representation question; it established the first-claim path and bounded existing-owner correction. No further discovery needed merely for unique findings or scope-limited PASS statements.

## Prior-finding closure


| Prior ID | Source and focused proof | Disposition |
|---|---|---|
| FIND-001-1 | `hosted/mod.rs:141–148` uses checked_add before checked conversion/multiplication. Config still consumes `lanes_fit`; Sandbox startup consumes the same predicate. Memory test asserts false with queued workers usize::MAX and positive direct workers; env test asserts named aggregate refusal for the maximum-width count. Fresh U passes. | CLOSED. The original worker-sum hole is fixed once at its owner. New configurable waiter/execution/model/workspace representation is H4/H8 scope below, not reopening this closed worker diagnosis or retroactively adding requirements to it. |
| FIND-001-2 | `caller.rs:137–158` now bounds/cancels lane acquisition and subtracts elapsed wait, but `:184–186` awaits invocation without the original bound; snapshot precedes the downstream deadline at `invocation.rs:565–568,601`. Fresh G proves fast-admission cases only. | INCOMPLETE; preserve ID. SYS-R2-001 validates the remaining original-bound path. |
| FIND-001-3 | `verifier_orchestration.rs:2118–2156` holds COMPARE. Admitted extra is accepted Succeeded with no disposition before release; excluded extra's turn-1 request carries publication results and is still Started before release. Remaining takeover, no-replay counts, mounted content, original attempts and retained publications are unchanged. Fresh exact recovery/disposition pair passes. | CLOSED; producer ordering is established using existing HeldUpstream/reached controls, without changing production recovery or mock response semantics. |
| FIND-001-4 | Server target registers child module at `verification_runtime.rs:87–88`; SDK registration/source removed. `CapacityDesk::start` uses existing bound server/native client, public credential/Gateway administration, Cards YAML declaration/pull and WyrdState hydration. Full moved test retains real verify/submit/status/wait/Bifrost requests, distinct saturations, deferral/continuations, reverse nonborrowing, cancellation and six subject assertions. Fresh J passes. | CLOSED; runtime holds and log inspection now reside in their prescribed runtime owner. No reverse SDK dependency, new target or alternate engine. |
| FIND-001-5 | `verification/mod.rs:632–636` adds a meaningful panic contract and retains default assertions. Fresh exact defaults test passes. | CLOSED. Changed Scripted dispatch is a separate new mandatory gate, FIND-001-7. |

## Retained findings

| Stable ID | Source IDs / validation | Diagnosis and required owner correction |
|---|---|---|
| FIND-001-2 | SYS-R2-001; REVISED correction with focused follow-up | Caller forwards a relative remainder before snapshot admission, then gateway constructs a later absolute deadline; supported contention permits overdue dispatch. Carry original absolute deadline through reversible preparation and existing engine deadline. Preserve durable admission/settlement and unbound behavior. |
| FIND-001-6 | MR2-1, STD-R2-002, independently confirmed validator-raised deadline representation; CONFIRMED | Early file checks block valid overrides; active counts overflow runtime semaphore representation; accepted deadline can fail first claim or silently fall back. Resolve winning values and validate their existing runtime representation once at the normal startup configuration owner. Three witnessed cases, one bounded correction. |
| FIND-001-7 | STD-R2-001; CONFIRMED | Materially changed scripted-dispatch method has poisoned-lock panic paths without mandatory rustdoc. Add its panic contract alongside substantive fixes. |

Exact locations, violated obligations, producer/consumer traces, materiality, correction burden and focused closure proof are in `findings-validation.md`. No rejected correction becomes optional work. The validated correction does not require a specification revision or new product decision. One bounded task, `001-R2-finish-capacity-boundaries.md`, goes directly to `$wyrd-implement`.

## Reconciled acceptance


U/G/O/J refer to the exact fresh selectors and assertion evidence in this round's verification.md: four units, two gateway tests, two orchestration tests, one relocated journey. All nine pass. A FAIL below identifies a separate source-proven path not asserted by those passing selections.

| Requirement | Implementation and observable verification evidence | Result |
|---|---|---|
| R1 checked combined workers | Shared checked_add/conversion/multiplication at hosted/mod.rs:141–148; memory test rejects usize::MAX aggregate without panic. U. | PASS |
| R2 max-width env named refusal | Existing env test uses maximum-width workers; shared lanes_fit yields normal aggregate ConfigError. U. | PASS |
| R3 valid/default/memory cases | Memory test retains worker_fits/lanes_fit, pool-below-lanes and per-worker rejection; config keeps valid/internal values. U. | PASS |
| R4 held-lane original timeout/no dispatch | Caller timeout/select; G asserts `[300ms,1s)` and empty dispatch while original permit remains held. | PASS |
| R5 one original admitted call bound | Remainder computed and fast-admission hung-provider deadline/end asserted in G; delayed snapshot restarts remainder after original expiry. FIND-001-2. | FAIL |
| R6 expiry/cancel cleanup and existing accounting | G asserts pre-cancelled 30s waiter ends within 5s with zero dispatch; provider timeout restores permit; drained tracker captures timed_out; following call succeeds. J proves enclosing execution timeout and following success. These pass, but original-bound cleanup during delayed admission is incomplete with R5. | PASS for asserted cleanup; FAIL for complete original-bound path |
| R7 unbound behavior | None branch passes untouched timeout; existing compatible-adapter G retains translation/refusal/response assertions and uses an unbound caller. | PASS |
| R8 three late-decision orderings | COMPARE held until succeeded/no disposition or published second turn/Started milestone at orchestration.rs:2118–2156. O. | PASS |
| R9 recovery pair/assertion preservation | Setup-only production-neutral correction; takeover/content/count/attempt/publication assertions unchanged; exact O 2/2. | PASS |
| R10 relocated real-client composition | Server child module retains YAML/client setup and full journey assertions listed below; J. | PASS |
| R11 SDK story removed/no alternate owner | SDK source/module removed; server registration/fixture README updated; no manifest/dependency or new engine. | PASS |
| R12 original defaults panic docs | verification/mod.rs:632–636 plus exact default U. | PASS |
| R13 checks and bounded scope | Reported fmt/lints/check:deps/docs checks pass; fresh incremental diff check passes. No API/dependency/scheduler; approved config additions supersede only earlier config restriction. | PASS for named checks; changed-item gate separately FIND-001-7 |
| H1 execution/model defaults | hosted default/assertions direct executions/models `(256,256,256)`; runtime remains `(64,5min)`. Earlier Python preservation superseded by H7. U. | PASS |
| H2 typed file startup path, private timings | VerificationConfig/VerificationLaneConfig fields; normal load/override/boot route; file U asserts both LaneLimits, slots, two-hour execution ceiling and unchanged lease/verify timeout. | PASS |
| H3 env > file > defaults | Existing U asserts valid winners `(2,23,13,9,8192MiB)` but invalid shadowed file errors precede override. FIND-001-6(a). | FAIL |
| H4 final source-independent validation | Existing U asserts selected zero, negative, scaled/pool overflow, memory and prewarm failures. Active count and queued-timeout representations are unchecked; shadowed file values are prematurely checked. Independent SQL witness reproduces the latter runtime failure. FIND-001-6. | FAIL |
| H5 existing owners | boot limits → same HostedBlocks; runtime → same builder/drain projection; no parallel loader/resource owner. J corroborates. | PASS |
| H6 operator/generated docs | Both pages document file/env keys/new defaults and remove obsolete env-only claim; generator projections correspond; reported docs:check passes. Corrections must keep effective-precedence wording accurate. | PASS delivery; H3 behavior defect remains |
| H7 Python/workspace/pool defaults | U exact workers `(4,12)`, waiters/workspaces `(16,48)`, pool8192MiB, prewarm1 each; totals `(16,64,64,2)`, memory fit and refusal checks retained. | PASS |
| H8 pool source/validation | U proves file pool4096MiB and env8192MiB, zero/scaled overflow and inconsistent file workers/pool refusal. Shadowed overflowing pool still aborts before env; final count parity gap from H4 remains. | FAIL for final-merged semantics; normal values PASS |
| H9 sizing/docs qualification | Docs show 6144 MiB worker accounting within 8192 MiB, the 32 GiB sizing assumption, no upfront pool allocation or reserved process memory/disk, shared downstream and no measured guarantee. Generated projection/check evidence retained. | PASS |

## Carried original acceptance disposition

| Original row/invariant | Retained or refreshed evidence | Result |
|---|---|---|
| O1 execution isolation | Refreshed J holds sole queued model/runner slot; direct answer passes while second queued status is Pending. | PASS |
| O2 workspace isolation | J holds queued workspaces; direct scratch requirement passes. | PASS |
| O3 Python isolation | J holds queued Python; direct measured requirement passes. | PASS |
| O4 model isolation | J witnesses queued MODEL_WAIT; direct answered requirement passes. | PASS |
| O5 queued execution progress | J pending assertion then both queued answered results after release. | PASS |
| O6 workspace defer/progress | J exact execution DEFERRED then written passes after release. | PASS |
| O7 Python defer/progress | J exact execution DEFERRED then measured passes after release. | PASS |
| O8 model wait/progress | J wait log while held then answered after release. | PASS |
| O9 no borrowing | J direct workspace/Python/execution stable503; direct model waits with free queued lane; queued counterparts complete. | PASS |
| O10 mode/reclaim selection | Carry r1 sole direct/queued Execution producers and exhaustive lane match; J resumes queued workspace/Python while direct equivalent remains held. | PASS |
| O11 typed/env refusal and original execution ceiling | Original worker-overflow finding closed; normal projections and two-hour execution setting pass U. Human-expanded final merged validation has FIND-001-6. | Original closure PASS; expanded acceptance FAIL |
| O12 defaults | Former combined defaults intentionally superseded by human H1/H7; U pins new approved counts and retained queued64/five-minute ceiling. | PASS under current authority |
| O13 eligibility before steps | Carry r1 source/test evidence: service resolve calls direct_eligibility before engine; pure Window refusal/ordinary acceptance plus existing Window journey. No incremental eligibility change. | Carried PASS |
| O14 tenant/subject gateway path | Caller retains captured identity/resolved subject; J reads six ordinary gateway rows under tenant and requires each subject = declared Service UID. | PASS |
| O15 design/operator docs | Carry unchanged architecture isolation/deadline section; refresh both operator pages/defaults/config/generated projections and reported docs check. | PASS |
| O16 named gates | Reported fmt/lints/deps/docs0; fresh diff-check0; all nine focused tests green. No broad gate required for bounded remediation. | PASS named checks |
| O17 one engine | Carry existing HostedBlocks::engine and retained Lane composition; config projects to same owners; no incremental alternate executor. | PASS |
| O18 execution cancellation | J exact ExecutionTimedOut during held direct model lane, release then successful Agent; G pending cancellation and permit cleanup. Inner-call bound remains separate FIND-001-2. | PASS execution contract |
| O19 ordinary graphs/recovery | Carry unchanged engine/executors and r1 hosted Agent/Window evidence; new J exercises real Agent/code/workspace; fresh exact orchestration pair now passes with assertions retained. | PASS |
| INV-001 authority/evidence/tenancy | Prior traced accepted Caller/evidence/registry and gateway permission/accounting unchanged; J attribution refreshed. | Carried PASS |
| INV-005 original persisted state/budgets/deadline | Prior runner claim/reclaim/checkpoint and original claim-age ceiling unchanged; no retry replenishment. Inner call duration is separate retained defect. | Carried PASS for persisted execution state |
| INV-007 and excluded scope | No table/broker/service/API/dependency, future queued-only action, host detection/platform enforcement, synthetic load, SLA or benchmark requirement. Approved settings remain startup projection. | PASS |
| Mandatory proof boundary and recovery gate | FIND-001-4/-3 closed at correct test owner and ordered producer; fresh J/O green. | PASS |
| Mandatory changed-item rustdoc | Original default item fixed, new changed dispatch item incomplete. | FAIL, FIND-001-7 |

Historical production-before-RED and later mutation RED are accurately disclosed in the original task/remediation. Fresh green runs prove current selected behavior; they do not rewrite test-first chronology. No source machinery can repair that historical order, and no new remediation is prescribed for it.

## Verification and limits

Fresh U/G/O/J run sequentially: 4 defaults/config units, 2 real gateway caller tests, 2 orchestration tests and 1 relocated real-client isolation journey all pass. The previously red recovery test now passes with its assertions intact. Both managed Postgres migrations and incremental/cumulative diff whitespace checks pass. Details, exact selectors, timings and run IDs are in `verification.md`.

The independent read-only PostgreSQL expression returned SQLSTATE `22008` for the accepted signed-max file timeout. Source tracing establishes the startup and first-claim wiring; no full failure journey is claimed. The passing gateway test does not exercise a delayed snapshot, and the passing config tests do not cover the retained effective-value failures. The resolver's one-line production loader connection is source-verified; no redundant test is required simply to mirror it. Historical production-before-test and later mutation RED are disclosed rather than rewritten.

Implementation-reported fmt/lints/check:deps/docs:check were not independently rerun. No full suite, platform, performance or mutation rerun. Only review artifacts were written; no source edits, commit or push.
