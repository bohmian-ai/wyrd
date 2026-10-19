# TASK-003 cumulative review verdict

**FIX_REQUIRED**

Two independently validated findings remain: **FIND-TASK-003-3** (final durable replay observation) and **FIND-TASK-003-6** (independent diagnosis evidence for two new R1 HTTP-test corrections). Findings **1, 2, 4 and 5 close**. The original five diagnosis subcases under finding 6 also close against Reports A–E. No new stable finding ID is assigned. Neither retained finding establishes production duplication, starvation, faulty admission or unsafe prior fixes.

One self-contained remediation is issued directly to a fresh `$wyrd-implement` agent: [TASK-003-R2-observe-replay-and-record-diagnosis.md](TASK-003-R2-observe-replay-and-record-diagnosis.md). This review does not implement it.

## Immutable subject and authority

| Field | Identity |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003` |
| Branch | `change/long-horizon-verification-task-003` |
| Original base | `ef3a57b0776dd7bc451273b9176498fcca700550` |
| Cumulative candidate / checked HEAD | `ea317179820b24d8b08ed499454ce2cf4675ce4e` |
| Full review range | `git diff ef3a57b07...ea3171798` |
| Prior candidate / fix locator | `9ffdf753ab11ce9b638c906388d3690a6aede249`; `git diff 9ffdf753a...ea3171798` used only to locate remediation |
| Remediation history | `a162396b5` records prior review; source/evidence commits `8d960f6ff`, `b6de87ffe`, `ba12fe8f1`, `efbb596c6`, `ea3171798` |
| Original task | `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md` |
| Authorized spec | `changes/active/long-horizon-verification/spec.md`, current TASK-003 text explicitly owner-authorized; revision 14's draft pre-release schema addition excluded |
| Prior review / remediation | `review/task-003-9ffdf753a/{verdict.md,findings-validation.md,evidence-reassessment.md,TASK-003-R1-close-admission-review.md}` and supplied `diagnostician-reports.md` / `supplemental-diagnoses.md` |

Steven's prior authorization was expressly reaffirmed for this review. No authority blocker remains and no draft schema addition is approved. The full cumulative candidate, original task, prior findings and remediation were audited; the implementer's conversational summary was not provided to discovery reviewers as evidence. Appended implementation evidence was treated as claims to check.

Every required review report is complete. The caller's requested new review directory was created before discovery; no prior attempt was overwritten. Source, tests, specification, original task, prior review directory, index and HEAD remained immutable. Only this new review directory was written. No commit, merge, push or deployment occurred.

## Reconciled acceptance matrix

The following reconciles independent discovery with [findings-validation.md](findings-validation.md). Source and static proof are distinguished from actual current-review tests and implementation-recorded runs. A green check does not replace missing outcome observations.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Finite whole-pending work, including queued/waiting/retrying/in-flight items and unused reservations | `outbox.rs` Admission/Reservation and writer state retain capacity until terminal consumption; both sinks use finite capacity | Seven exact runtime tests independently passed; full source/consumer traces | PASS |
| No hidden unbounded Scribe-ACK → SubmissionOutbox handoff | Enqueue reserves downstream then source; ExecutionInputs carries the downstream slot through ACK | Behavior/invariant/domain traces; independent HTTP pressure proof 2/2 | PASS |
| Typed retryable refusal before source staging; no partial refused direct two-row call | Control execute reserves two before engine; enqueue reserves both owners; catalog error, retry detail and HTTP Retry-After | Source audit; SDK/MCP/header tests inspected; original recorded runs, not repeated | PASS |
| Successful accepted work stays retained across retry; identical failed slices are not re-admitted or merged with later arrivals | Generic writer owns failed/in-flight pair; Scribe batch identity and SQL replay remain existing owners | Independent retry/in-flight/bound selectors passed | PASS |
| Ordinary active execution tasks bounded at claim owner | RuntimeLimits execution_slots, ClaimLoop JoinSet checks before rounds/claims; Operator retains separate existing permit cap | Source audit and recorded execution-slot integration evidence | PASS |
| Audit/span/capture remain non-blocking/countable with one Scribe writer and no operation error | Unreserved share overflow counts/logs then returns; permission check ordering unchanged | Independent bounded-audit test; sibling/caller tracing | PASS |
| Correct preserved concurrent shutdown/loss semantics and unused slot ownership | `outbox.rs:336–368` nonzero pending-swap winner exclusively settles paired staged-reserved count; unused Reservation Drop independent | Exact new concurrent test plus drain/deadline regressions passed; source proof covers old interleaving not forced by current-thread test | PASS — FIND-1 closed |
| B completes while A continues offered traffic when resources permit; accepted work reconciles/drains | Three SDK stories offer normal A submits until B's wait completes; retain held A/refusals and `3 + offered` reconciliation | Current source inspected; three focused SDK Greens/native rebuilds recorded, not independently rerun | PASS — FIND-2 closed |
| Same-key pressure refusal → drain → retry, no source row from refusal | New HTTP test preserves body/key/authority, waits recovered execution, reconciles inputs `(3,1)` before accepted replay | Exact new HTTP test independently passed with tracing | PASS for recovered identity/source seam |
| Final accepted replay is processed before asserting one durable execution for that identity | Test `pg_verification_routes.rs:532–541` reads queue immediately after replay 202; queue is asynchronous behind Scribe ACK and SubmissionSink commit | Source schedule can pass count before replay processing; older replay test does not order this test's SELECT | FAIL — FIND-3 retained, narrowed |
| Existing lease/fence/tenant/authority and crash-loss semantics preserved | Existing queue/settlement/tenant connections and lifecycle order unchanged; source remains staging-only | System/concurrency/durability source audits; healthy-path tests do not imply new outage/crash guarantees | PASS |
| Listed touched Rust Errors/Panics contracts complete | Execute/enqueue Errors, Scribe constructors, bounded/audit/shutdown and mapper tests, new HTTP helper/test and widened retained reader documented | Independent validation closes prior omissions; navigation-only proposal rejected under materiality | PASS — FIND-4 closed |
| Normative security/recovery prose reflects finite/countable overflow | Security and operations paragraphs replace old unbounded promise while preserving retry/ACK/crash boundaries | Static source-to-authority comparison | PASS — FIND-5 closed |
| Required independent diagnosis for original implementation failures | Reports A–E cover fixture, cap, admission, neighbor and compiler causes; retrospective limits preserved | Reports/source/compiler/trace reconciliation; no historical pre-fix compliance inferred | PASS for original five subcases |
| Required independent diagnosis for R1's new unexpected HTTP-test failures | R1 row 151 describes queue-count timing and physical-view projection fixes, but provides no corresponding independent report | A–E concern original failures; current 2/2 Green is not diagnosis evidence | FAIL — FIND-6 retained for new cohesive HTTP boundary |
| Rust server ownership, pure spec, common SDK projection, typed errors, SQL capabilities/tenancy and portable facilities | Existing Outbox/ClaimLoop/queue/client owners extended; no new producer SQL, transport or platform primitive | Complete standards/caller coverage and cumulative reuse assessment | PASS |
| Non-goals stay excluded: distributed quotas, strict fairness, durable outbox/receipts, new SDK identity API/schema, extra writer/harness | Cumulative source audit and existing-owner comparisons | No prohibited machinery found | PASS |
| Standalone proof independent of TASK-001; grouped J-005 and capacity qualification stay at assigned integration/change owners | Ordinary calls used; no grouped schema or TASK-004 behavior imported | Deferred evidence remains explicitly unrun, not claimed complete | PASS for task-local scope |

## Independent reports and evidence

| Role | Report | Discovery result |
|---|---|---|
| Fresh behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; B-R2-001 proposes residual FIND-6; other prior closures assessed |
| Fresh invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL; INV-R2-001/002 propose residual FIND-3/6; seven runtime selectors passed |
| Fresh repo-rev | [standards-review.md](standards-review.md) | FAIL; STD-R2-01/02 propose residual FIND-4/6; complete all-surface authority/rule coverage |
| Fresh maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS; complete owner/caller/test/reuse coverage, no material proposed finding |
| Fresh system-rev | [system-review.md](system-review.md) | PASS; deployed failure/recovery and shared-capability audit, no material finding |
| Fresh concurrency domain-rev | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS; admission/RAII/task caps/tenant/shutdown, no material finding |
| Fresh durability domain-rev | [domain-review-durability.md](domain-review-durability.md) | PASS; accepted writes/ACK/replay/settlement/loss; exact HTTP pair passed |
| Fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED replay-observation conflict; FOLLOW-R2-001 proposes residual FIND-3 |
| Fresh structured ponytail-rev | [findings-validation.md](findings-validation.md) | Validates every proposal; retains only revised FIND-3/6; explicit prior closure and rejected dispositions |

Concurrency/resource ownership and durability/persistent handoff are the materially changed sensitive domains. Standards/domain reviewers also traced actual unchanged TenantConn/RLS and authority propagation; the testing reader's widened visibility does not create a new production permission/tenant route. No separate unrelated security/tenancy design pass was invented. All seven discovery reviewers received the same starting map and expanded it independently; they did not read other current discovery conclusions before completing reports. They read prior findings as required remediation hypotheses.

The orchestrator compared claims and routed uncertainty; it did not validate its own findings. See [claim-comparison.md](claim-comparison.md). A fresh follow-up was required because invariant review challenged the final asynchronous replay observation while behavior/durability credited the combined tests and existing SQL source. It traced actual source/send/tenant/slice/ACK/commit order, distinguishing correctness of current SQL replay from when this test observes it. The validator independently accepted the narrower missing-proof proposal. No extra follow-up was run simply to obtain agreement.

The validator rejected STD-R2-01 as a blocking FIND-4 continuation under the skill's materiality threshold; prior missing Errors/Panics contracts are repaired and substantive lifecycle documentation remains. It also rejected mechanical-helper calibration as a material finding. Rejected proposals are omitted from the actionable ledger and remediation, not softened into optional advice. Full disposition evidence remains in findings-validation.md.

## Validated ledger and prior-finding closure

| Stable ID | Disposition at this candidate | Validated evidence / correction |
|---|---|---|
| FIND-TASK-003-1 | CLOSED | Sole nonzero pending winner takes/releases staged-reserved after sender fence/writer termination; exact test covers loss once and retained unspent slot; source proof closes historical concurrent interleaving. |
| FIND-TASK-003-2 | CLOSED | Every SDK story now offers A requests until B completion is observed and reconciles them; resource-availability qualifier retained. |
| FIND-TASK-003-3 | REVISED, PARTIALLY CLOSED / RETAINED; MISSING acceptance proof | INV-R2-001, FOLLOW-R2-001; `pg_verification_routes.rs:532–541`. Same-key refusal/retry/source proof closes. Final replay's queue count remains earlier than its required downstream observation. Reuse existing same-tenant distinct sentinel, wait its execution visibility, then count **4**, preserving pre-replay `(3,1)` and current loss/identity contracts. |
| FIND-TASK-003-4 | CLOSED | Previously missing changed-item error/panic documentation is present. Independent validator rejects remaining proposals as material continuations; no style-only task is issued. |
| FIND-TASK-003-5 | CLOSED | Security/recovery prose consistently describes finite staging and logged/counted non-blocking overflow without strengthening durability. |
| FIND-TASK-003-6 | REVISED; original five subcases CLOSED; RETAINED for new R1 HTTP diagnosis evidence | B-R2-001, INV-R2-002, STD-R2-02; prior R1 task `:151`, affected `pg_verification_routes.rs:466–480,518–541`. A–E close original causes. Recover or obtain one honest independent retrospective report for the two new HTTP failures, with available command/trace/diff, causes, owners, siblings and historical limits; preserve existing source-supported fixes. |

Full producer/consumer/sibling traces, realistic schedules, decision-complete minimum corrections and focused closure commands are in the independent validated ledger. No product, public API, concurrency semantics, persistent-data decision or spec revision is required for these corrections. The selected sentinel uses existing per-tenant writer/order and SQL visibility, not a new receipt, test-control API or guarantee. The diagnosis report may cover both related test failures together; no report-per-caller expansion is required.

## Verification actually performed and limits

- Invariant reviewer ran the exact seven named runtime selectors through `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib`: **7 passed, 62 skipped**, default profile, nextest `d7e8d8af-3e40-4461-b703-ce828a8fecca`. Full exact expression appears in its report.
- Durability reviewer ran the exact two named HTTP pressure/replay selectors through the repository-managed Postgres/migration wrapper with tracing: **2 passed, 270 skipped**, default features/profile, nextest `53816fcf-9e79-4907-8373-00c54fdb2f01`. The migration wrapper separately passed one migration check each for wyrd-sql and vala-sql. Full command is in its report; raw run output is `/tmp/task003-r2-durability-http.log`.
- Root/standards/maintainer/validator cumulative `git diff --check ef3a57b07...ea3171798` was **nonzero** for trailing spaces in verbatim prior `supplemental-diagnoses.md:72,209`. The validator records this as a nonblocking evidence-format limit without a behavioral, public-contract or diagnostic-meaning consequence. The prior directory remains immutable. A clean working-tree diff check is a different subject and is not reported as proof the committed cumulative range passed.
- Final HEAD checks matched candidate and tracked/index diffs were empty; only the requested new directory is untracked. No production/test/spec/original-task/prior-review edits occurred.

The exact HTTP pair's Green does not imply the final replay count is correctly ordered; source establishes that missing observation. The shutdown test does not force the old two-thread swap interleaving; single-owner source proof supplies that closure. Rebuilt native bindings, focused Rust/Python/TS stories and original/R1 static/codegen/SDK/MCP/runtime checks remain recorded implementation evidence, not independently repeated current-review results. No broad aggregate, full journey suite, SDK rerun, live provider/cloud storage, fault campaign, grouped J-005 or capacity benchmark was performed. Those limits do not grant new loss-free receipts or fairness claims.

## Handoff

[TASK-003-R2-observe-replay-and-record-diagnosis.md](TASK-003-R2-observe-replay-and-record-diagnosis.md) is the single self-contained ready remediation for FIND-3 and FIND-6. It includes exact locations, violated obligations, diagnoses, selected existing correction/proof owners, preserved behavior, acceptance criteria and exact narrow closure commands. No closed finding or rejected proposal enters its write set. Route directly to `$wyrd-implement`; a subsequent immutable review must retain the original base, original task, prior reviews/remediations and full cumulative new candidate.

Review has not implemented, committed, merged, pushed or deployed the change. Task completion remains withheld until the two validated proof/evidence gaps close.
