# TASK-003 verdict

**FIX_REQUIRED**

Steven, the specification owner, explicitly authorized the current `changes/active/long-horizon-verification/spec.md` TASK-003 text as this review's authority on 2026-10-17. Revision 14's draft pre-release schema addition is outside TASK-003 and excluded. This resolves and supersedes the earlier authority blocker without editing or approving the excluded draft addition.

Six validated findings remain. Supplied independent diagnostician Reports A/B close FIND-TASK-003-6 for diagnoses 3 and 5; only diagnoses 1, 2 and 4 remain open under that ID. Findings 1–5 are unchanged. One self-contained remediation is issued directly to a fresh `$wyrd-implement` agent: [TASK-003-R1-close-admission-review.md](TASK-003-R1-close-admission-review.md).

## Immutable subject

| Field | Identity |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003` |
| Branch | `change/long-horizon-verification-task-003` |
| Task | `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md` |
| Requested approved spec | `changes/active/long-horizon-verification/spec.md`, revision 13 |
| Authorized review authority | Current file TASK-003 text, explicitly authorized by Steven on 2026-10-17; revision-14 draft schema addition excluded |
| Base | `ef3a57b0776dd7bc451273b9176498fcca700550` |
| Candidate | `9ffdf753ab11ce9b638c906388d3690a6aede249` |
| Cumulative range | `git diff ef3a57b07...9ffdf753a`, all 32 changed paths |
| Candidate commits | `2348d34d8` through `9ffdf753a` |

This updates the earlier BLOCKED verdict in this directory after owner authorization and new diagnosis evidence. No production remediation has been implemented; stable finding IDs are preserved. There is no earlier TASK-003 remediation round. The pre-existing `plan-readiness` directory is not a prior task implementation audit. HEAD remained the candidate throughout review. Tracked production source, tests, configuration, architecture, task and spec were not edited. Only review reports, copied diagnostic evidence and the remediation packet in this directory were written; nothing was staged, committed, merged, pushed or deployed.

## Reconciled acceptance matrix

PASS/FAIL below assesses TASK-003 under the owner-authorized current specification text, excluding revision 14's draft schema addition. Verification distinguishes source inspection, independently executed checks and implementation-recorded claims. Finding dispositions come from the independent validator, not orchestrator source validation.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Finite whole-pending staging, including queued, waiting, retrying and in-flight items | `outbox.rs` Admission/Reservation and writer backlog/in-flight accounting retain charges until terminal consumption | Six focused runtime selectors independently passed; complete source traces in task and concurrency reports | PASS |
| No hidden unbounded Scribe-ACK → submission handoff | `VerificationControl::enqueue` reserves SubmissionOutbox before Scribe; ExecutionInputs carries the slot until ACK | Independent behavior/invariant/durability/concurrency source tracing; SDK execution claims recorded in task | PASS |
| Finite ordinary active execution tasks | `RuntimeLimits::execution_slots`, ClaimLoop run/claim_round checks, runner constructor; Operator retains its separate existing permits | New execution-slot test source inspected; server integration run recorded, not reproduced | PASS |
| Retry slices retain identity and do not double-charge | Existing failed backlog retains slice/count; admission stays producer-owned | Exact identical-slice and bounded in-flight/retry tests independently passed | PASS |
| Typed retryable pressure before any source row stages; direct two-row refusal cannot be partial | Control execute reserves two slots; enqueue reserves both owners; derive-backed error, detail and Retry-After mapper | SDK/MCP pressure and HTTP header assertions inspected; claimed green results not independently rerun | PASS |
| Successful source staging is retained through normal sink failure | Reservation ownership continues through retry and handoff; no new capacity drop of admitted source rows | Durability source audit; exact retry/drain tests passed | PASS |
| Audit remains non-blocking, logged/counted on overflow, one Scribe write path | Unreserved stage does not return an audit failure; authorization remains before producer reservation; spans/capture share existing owner | Exact bounded-audit test independently passed; caller/sibling source tracing | PASS for implementation |
| Preserve existing shutdown/loss accounting contract | Shutdown independently swaps pending and staged-reserved before shared release | Validated supported concurrent-call interleaving; existing single-caller tests do not cover it | FAIL — FIND-TASK-003-1 |
| B completes while A continues offering calls, when resources permit | Three new SDK stories await B before A2/pressure offers | Held-A activity proved; continued-offering seam absent | FAIL — FIND-TASK-003-2 |
| Retry same refused enqueue identity after drain and replay once | Shared client generates a new key on every repeated submit invocation; HTTP supports a stable supplied key | Fresh-key recovery and existing separate replay proof do not exercise composed pressure/drain/same-key recovery | FAIL — FIND-TASK-003-3 |
| Accepted work drains and existing recovery remains truthful | Existing outbox/claim/lease/settlement lifecycle retained | Exact successful-drain/deadline-loss tests passed; concurrent shutdown exception above | PASS for existing single-coordinator paths; FAIL for preserved concurrent contract |
| Standalone proof independent of TASK-001; grouped J-005 recorded after integration | No group_id dependency introduced | Task explicitly records grouped proof as open; not misrepresented as completed | PASS for standalone task scope; grouped seam remains deferred |
| Rust server ownership, IO-free foundational spec, SDK projections, tenant boundaries, portability | Existing Outbox/ClaimLoop/queue/error owners extended; no new producer SQL or platform mechanism | All changed layers independently covered; actual SQL capability source inspected | PASS |
| No distributed quota, strict fairness, durable outbox, receipt, alternate transport/writer or unrelated owner | Cumulative source and owner-reuse audit | No prohibited machinery found by independent reviews | PASS |
| Complete changed Rust error/panic documentation | Listed execute/enqueue, Scribe constructors and tests omit required contracts | Independent standards finding confirmed under AGENTS §16 | FAIL — FIND-TASK-003-4 |
| Normative operational/security contract reflects bounded audit overflow loss | Two governing paragraphs still promise unbounded outbox retention | Validator read implementation plus security/operations authorities | FAIL — FIND-TASK-003-5 |
| Failure-driven adaptations have inspectable independent diagnosis evidence | Copied Reports A/B close diagnoses 3/5; reports for 1/2/4 remain unavailable | Independent evidence reassessment validates supplied causes and narrows remaining scope | FAIL — FIND-TASK-003-6 (partial closure) |

## Independent review results

Initial discovery results below are historical reports preserved unchanged. Their authority-only BLOCKED qualifications are superseded by the owner decision above; findings are governed by independent validation and the new evidence reassessment.

| Role | Report | Result and limits |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | BLOCKED on authority; B-001/B-002 proposed; 2 focused tests passed |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL; INV-REV-001/002 proposed; 6 focused tests passed |
| repo-rev | [standards-review.md](standards-review.md) | FAIL; complete all-language authority coverage; STD-003-01/02/03 proposed |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL; complete changed-owner/reuse coverage; MAINT-001 proposed |
| system-rev | [system-review.md](system-review.md) | BLOCKED on authority; runtime source audit PASS; 6 focused tests passed |
| domain-rev concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS under common task obligations; 6 focused tests passed |
| domain-rev durability | [domain-review-durability.md](domain-review-durability.md) | PASS under common task obligations; 4 focused tests passed |
| followup-rev | [followup-review.md](followup-review.md) | Both assigned conflicts RESOLVED; authority separate/unresolved |
| ponytail-rev | [findings-validation.md](findings-validation.md), [evidence-reassessment.md](evidence-reassessment.md) | Complete validation; authority resolved; six findings retained, FIND-6 narrowed to diagnoses 1/2/4 |

Sensitive domain selection: concurrency/resource ownership and durability/persistent handoff materially changed. No SQL query, identity, authorization policy or tenant routing was changed; domain and standards reviewers traced unchanged TenantConn/RLS boundaries rather than adding an unrelated security/tenancy review role. Every discovery reviewer received the same subject/navigation map and expanded it independently. No discovery reviewer received other reviewers' conclusions before completing its report.

## Follow-up decision and reconciliation

A fresh focused follow-up was required because discovery materially differed on (a) the supported generic concurrent-shutdown contract versus the sequential deployed coordinator, and (b) active-held-A versus continuing-offered-A tenant progress. It traced the base contract, current production callers, all three journeys, shared client key generation and existing HTTP replay owner. The validator then independently resolved every proposal, including unique standards/evidence proposals. Agreement was not used as proof. See [claim comparison](claim-comparison.md).

The accepted shutdown finding preserves an existing explicit API/lifecycle guarantee; it does not allege that current server teardown concurrently invokes that method. Continued-offering and fixed-key recovery are missing required proofs, not demonstrated production starvation or duplication. No new SDK key API, scheduler, strict fairness or stronger durability is prescribed. Fresh-key recovery and held-A activity retain their narrower credit. Missing rustdoc is blocking under the explicit repository gate, irrespective of its nonblocking characterization in a maintainer calibration note. Rejected extensions are preserved only as disposition evidence in findings-validation.md, not included as optional advice.

## Validated finding ledger

The following is the complete retained ledger. Full producer/sibling traces, disposition evidence, decision-complete minimal recommendations and focused closure commands are in [findings-validation.md](findings-validation.md). Authority is resolved and those recommendations are packaged in the single issued remediation task linked above. The independent evidence reassessment supersedes the original blanket missing-report scope of FIND-6.

| Stable ID | Status / classification | Discovery IDs | Exact location and consequence | Minimal correction boundary / closure |
|---|---|---|---|---|
| FIND-TASK-003-1 | REVISED; REGRESSION / INCORRECT | B-001, INV-REV-001, MAINT-001, FOLLOW-001 | `crates/shared/wyrd-runtime/src/outbox.rs:348–350`, release at 438–441: concurrent callers can divide paired counts and call release(0,1), panicking with checked arithmetic | Existing pending-swap winner must own paired final settlement; focused reserved-work concurrent-shutdown proof, preserve unused reservations and normal drain/loss |
| FIND-TASK-003-2 | CONFIRMED; MISSING acceptance proof | B-002 progress, INV-REV-002, FOLLOW-002a | Rust journey 1228–1261; Python 478–503; TS 449–467: B finishes before continuing A traffic/pressure | Extend existing three focused stories with finite overlapping offered traffic, B completion before offers stop, then reconcile drain; no new load harness |
| FIND-TASK-003-3 | REVISED; MISSING acceptance proof | B-002 identity, FOLLOW-002b | Rust recovery 1295–1308 and Python/TS mirrors; shared client verify/mod.rs:444–450 mints fresh keys | Existing HTTP explicit-key enqueue/replay owner proves refusal → drain → identical-tuple retry/replay → one durable execution; no SDK API addition |
| FIND-TASK-003-4 | CONFIRMED; VIOLATION | STD-003-01, maintainer calibration | Service Errors docs 453/604, Scribe constructors 652/663, runtime tests 1069/1129, HTTP mapper test 410 | Complete touched Errors/Panics docs together; source inspection and Rust formatting/lints; batch with substantive correction |
| FIND-TASK-003-5 | CONFIRMED; VIOLATION | STD-003-02 | `architecture/wyrd-security-posture.md:469`, `architecture/operations/reliability-and-recovery.md:73` promise retention implementation now intentionally sheds | Synchronize existing normative paragraphs to finite staging/countable non-blocking overflow loss; preserve ACK/retry/crash boundaries; static consistency proof |
| FIND-TASK-003-6 | REVISED, PARTIALLY CLOSED; VIOLATION of evidence requirement | STD-003-03; independent reassessment | Task diagnoses 235–239: reports for diagnoses 1 (admission), 2 (neighbor baseline/key), 4 (large_futures) remain missing; Reports A/B close 3 (TS fixture) and 5 (runtime cap) | Preserve copied reports; recover or honestly obtain independent retrospective cause validation only for 1/2/4, preserving historical limits and source-supported fixes |

## Evidence closure and verification limits

The owner supplied `/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-long-horizon-verification-task-003/3d9ed8e8-ef54-480b-a2fe-2598087f67c5/scratchpad/diagnostician-reports.md`. It was copied verbatim to [diagnostician-reports.md](diagnostician-reports.md), with byte equality verified by `cmp`. The independent validator reassessed all five diagnoses against those reports and source in [evidence-reassessment.md](evidence-reassessment.md). Report A explains the file-scoped fixture cache/lifetime correction; Report B explains 64-slot incompatibility with 70/200 held runs. These close diagnoses 3/5 without reruns solely for reattaching evidence. The admission report remains unavailable and neighbor/compiler reports absent, leaving 1/2/4 open. Lack of timing/assertion changes does not exempt fixes touching tests. Fresh reports must honestly label retrospective validation. No finding ID was renumbered or wholly closed.

Independent reviewers actually executed the exact runtime bound/audit/retry/tenant/drain/deadline selectors; all six distinct selected tests passed. Some reviewers repeated subsets; those repeats do not increase coverage. Exact commands, selections and individual limits are in their reports. `git diff --check` passed in independent review. No reviewer reproduced the shutdown race; its retained evidence is a validated source interleaving against the documented required contract.

The implementation's SDK/MCP/Postgres, full-file, format/lint, dependency, codegen, docs and language-typecheck results remain recorded claims without supplied raw execution logs. They were inspected for meaning and coverage, not silently promoted to reviewer-run results. The passing pressure stories do not close the identified additional progress/replay seams. Missing diagnostician reports are separately tracked. No broad gates, capacity benchmark, live storage/provider checks or grouped J-005 integration rerun were performed. The claimed TS clean-checkout worker-build issue was not independently reproduced; this review does not clear that lane or prescribe an unrelated lane change.

## Remediation handoff

[TASK-003-R1-close-admission-review.md](TASK-003-R1-close-admission-review.md) is the single ready remediation for all six IDs. It contains exact candidate locations, diagnoses, existing correction owners, preserved behavior/non-goals, mapped acceptance criteria and focused closure commands. FIND-6 includes only missing diagnosis work (1/2/4), preserving supplied closures (3/5). Route directly to a fresh `$wyrd-implement` agent; this review does not execute remediation. A later review reassesses the complete original-base-to-new-candidate range and preserves stable IDs.

No source change, test rerun, commit, merge, push or deployment occurred in this authority/evidence update. Final checks confirmed HEAD remains the candidate and tracked source, tests, specification and original task are unchanged. Remaining risks are the five source/documentation/proof findings and narrowed diagnosis-evidence gap, not an unresolved approval request.
