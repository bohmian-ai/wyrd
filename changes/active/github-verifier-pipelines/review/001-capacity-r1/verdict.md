# Task 001 verdict

**FIX_REQUIRED** — FIND-001-1, FIND-001-2, FIND-001-3, FIND-001-4, FIND-001-5.

## Immutable subject and round

Repository: `/home/thorrester/Documents/GitHub/wyrd-factory-worktrees/code-review-workflow-github-verifier-pipelines`.
Base: `395274d58bf76a2075f245ca3a5c1bd044925027`.
Candidate: `34fb45bb387582d2aedfee77fb4e09fa2753b80d`.
First review, complete `git diff 395274d58...34fb45bb3`; four implementation commits, 34 changed files. No prior verdict, remediation, acceptance or closure evidence carried forward. Source and candidate stayed immutable; only review reports were written. No implementation, commit, push or deployment occurred.

Original task: `changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md`.
Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 3, REQ-015/016, INV-001/005/007 and AC-010. Authority includes approved runtime design, architecture Verifier resource-composition section, root/nested repository rules, applicable topic guides and TESTING. The navigation/authority/applicability record is `scope.md`.

## Independent roles, applicability and coverage

| Role | Report | Result / coverage |
|---|---|---|
| behavior-rev | task-review-behavior.md | FAIL; full task, all O1–O19 plus omitted obligations, caller-to-result and existing-owner comparison |
| invariant-rev | task-review-invariants.md | PASS; shared state, resource ownership, queued claims/reclaims and persisted authority/deadline |
| domain-rev concurrency | domain-review-concurrency.md | PASS; sensitive permit/wait/lifetime synchronization and no borrowing |
| maintainer-rev | maintainer-review.md | PASS; changed ownership/abstractions, full existing workflows and harness reuse |
| repo-rev | standards-review.md | FAIL; all changed server/harness/SDK/fixture/architecture/docs/generated surfaces and rustdoc |
| system-rev | system-review.md | FAIL; shared-server startup/shutdown, saturation, cancellation, recovery and inner-call deadline |
| followup-rev | followup-review.md | RESOLVED; SDK proof ownership conflict and source-backed correction feasibility |
| fresh ponytail-rev | findings-validation.md | FIX_REQUIRED; independently validated all proposals, corrections, acceptance and specialist applicability |

Specialists were required by materially changed shared lifecycle/concurrency, failure propagation, substantial resource composition, and broad server/harness/SDK-test/docs rules. The validator confirms all required coverage is present. Separate security/persistence specialists are not applicable: no identity, authorization, SQL/schema or durable handoff contract changed; accepted Caller/evidence, gateway permission/accounting and original checkpoint/deadline consumers were traced by selected reviewers. Python/TypeScript/UI specialist omissions are supported by no changed production surface, projection or language lifetime. No missing specialist or new validator-discovered blocker remains. Full rationale and producers/consumers are in scope.md and findings-validation.md.

Follow-up was necessary for the material disagreement over SDK story placement. It found the explicitly prescribed server target already supports real client declaration, hydration, verify/submit/readback using the exact client types reexported by the SDK. The validator confirms relocation preserves AC-010 without new machinery or a spec revision. Unique arithmetic and timeout proposals needed independent source validation, not repeat discovery merely to obtain agreement. Apparent timeout disagreement is resolved by distinct contracts: whole-execution cancellation passes; the shorter per-call duration does not include lane wait.

## Validated finding ledger

Only independently CONFIRMED or REVISED findings are retained. Exact traces, materiality, correction burden, producer/sibling-consumer coverage and focused proof are in findings-validation.md; recommendations below preserve that ledger without expanding it.

| ID | Discovery sources | Status / class | Exact correction owner and consequence | Minimal correction |
|---|---|---|---|---|
| FIND-001-1 | BEH-LANE-WORKER-OVERFLOW | REVISED / INCORRECT | hosted/mod.rs:140–144; worker-count addition panics or wraps before checked memory validation | Checked aggregate addition in HostedLimits::lanes_fit, use existing false/error paths. No extra downstream sandbox guard for this witnessed state. |
| FIND-001-2 | SYS-001 | CONFIRMED / REGRESSION, INCORRECT | gateway/caller.rs:128–170; lane wait spends no supplied call budget, permitting overdue dispatch under supported long execution ceilings | Enforce one original call deadline around admission and existing governed invocation; preserve GatewayInvocation cancellation/settlement/accounting. |
| FIND-001-3 | BEH-UNRESOLVED-RECOVERY-GATE, STD-001 | REVISED / VIOLATION | verifier_orchestration.rs:2065–2194; late-decision test does not order comparison after extra milestone and required affected gate is red | Existing HeldUpstream/reached ordering in that test, retain takeover/content/attempt assertions; no sleeps or engine changes. Fresh exact failure assertion remains unavailable in bounded output. |
| FIND-001-4 | STD-002, FOLLOWUP-001 | CONFIRMED / VIOLATION | SDK isolated_capacity.rs:153–160,240–249,405–412 and main.rs:19–20; runtime controls/log polling cross explicit SDK test contract | Relocate real-client composition proof to existing wyrd-testing server/verification-runtime owner, preserve every resource/result/accounting assertion and shared YAML; no new harness or SDK control. |
| FIND-001-5 | STD-003 | CONFIRMED / VIOLATION | verification/mod.rs:626–665; changed defaults test lacks mandatory panic contract | Add meaningful # Panics, retain assertions; bundle with substantive fixes. |

Five retained findings, no rejected proposals creating optional work. One bounded remediation: `001-R1-close-capacity-review.md`, directly for `$wyrd-implement`. No behavior/expensive-boundary decision or task/spec revision is necessary. No prior-finding closure or convergence assessment applies to this first review.

## Reconciled acceptance matrix


`J` denotes the full new capacity story inspected above. Fresh J and four exact server units pass in verification.md; other supplied checks remain implementation-reported evidence, not newly rerun here. A behavior PASS below does not waive the separate mandatory proof-owner/gate findings.

| Obligation | Source and observable proof | Result |
|---|---|---|
| O1 execution isolation | Direct request owns `admit_direct`; queued owner remains runtime ClaimLoop. J seats the only queued slot on the held model call, then direct assert succeeds while second queued result is Pending. | PASS |
| O2 workspace isolation | HostedBlocks selects distinct Workspaces. J holds queued workspaces while direct writer's `written` requirement passes. | PASS |
| O3 Python isolation | Selected sandbox used by CodeExecutor and HostedTools. J holds queued Python while direct `measured` passes. | PASS |
| O4 model isolation | Distinct model semaphore before existing GatewayInvocation. J observes queued wait and direct `answered` passes. | PASS |
| O5 queued execution wait/progress | Existing runtime slot stays owned across process execution. J asserts Pending before release and both queued requirements pass after release. | PASS |
| O6 workspace defer/progress | Existing Busy→Defer owner retained. J requires exact execution deferral then `written` passes after release. | PASS |
| O7 Python defer/progress | Same existing deferral path. J requires exact execution deferral then `measured` passes after release. | PASS |
| O8 queued model wait/progress | Owned lane acquisition. J witnesses MODEL_WAIT while held and `answered` passes after release. | PASS |
| O9 no borrowing | Exhaustive lane selection, independent direct permits. J asserts direct workspace/Python/execution 503 and direct model waits while queued lane remains free. Queued twins complete. | PASS |
| O10 mode/reclaim routing | Sole runner engine initializer sets Queued; direct sets Direct. J resumed Python/workspace runs pass while corresponding direct admission remains held. | PASS |
| O11 typed/env limits and original ceiling | Normal values reach retained owners and boot runtime builder; fresh config unit checks both full lanes and two-hour deadline plus malformed/scaled/inconsistent inputs. Combined worker overflow remains unrefused: FIND-001-1. | FAIL |
| O12 defaults | Fresh memory/default units assert total workers/workspaces/waiters/prewarm `(5,16,16,2)` and queued `(64,five minutes)`. Mandatory panic documentation is separately FIND-001-5. | PASS behavior |
| O13 pre-step eligibility | `resolve` calls direct_eligibility before admission/engine; fresh pure test refuses Window and accepts ordinary graph; supplied Window journey asserts stable refusal. Future queued-only forms belong to their introducing tasks. | PASS |
| O14 ordinary tenant/subject/accounting | Selected caller passes unchanged captured identity and subject into GatewayInvocation. J reads six gateway rows under the tenant and asserts every subject is the declared Service UID. | PASS |
| O15 owner/operator documentation | Design and both operator pages describe separate resources/no borrowing/shared downstream capacity/original ceiling including approvals; actual settings match config. Supplied docs:check includes generated parity. | PASS |
| O16 explicit fmt/lints/deps/diff checks | Supplied fmt/lints/check:deps pass; fresh cumulative diff check passes. Broader affected red verification remains separately mandatory. | PASS for named checks |
| O17 one engine | Both callers use HostedBlocks::engine; Lane composes existing resource owners; no alternate engine introduced. | PASS |
| O18 execution cancellation | Owned model permit and enclosing execution deadline cancel pending future; J asserts exact ExecutionTimedOut while held, then successful direct Agent after release. This does not prove per-call timeout preservation. | PASS |
| O19 ordinary direct graphs | Existing engine/executors retained; supplied hosted_agent_tools and Window owner journeys pass, with J exercising actual Agent/tool/code flows. Separate recovery gate remains red. | PASS within direct-graph scope |
| Additional task requirement: existing per-call deadline/budget behavior | Skald supplies finite timeout but new wait does not spend it; FIND-001-2. | FAIL |
| Additional mandatory affected test gate | Fresh exact recovery selector remains red and setup lacks required ordering; FIND-001-3. | FAIL |
| Additional proof ownership/public SDK test boundary | Client proof exists and passes, but runtime access/log polling is registered in the SDK tier; FIND-001-4. | FAIL |
| Additional changed-item rustdoc gate | Defaults test's panic contract missing; FIND-001-5. | FAIL |
| INV-001 accepted authority, evidence and tenancy | Existing accepted Caller, tenant evidence/registry, route and GatewayInvocation permission/accounting owners retained; no identity/SQL contract change. J attribution and traced producers/consumers corroborate. | PASS |
| INV-005 original persisted execution state | Runner's functional composition diff is Queued mode; accepted authority/checkpoint restoration and original remaining-age deadline remain with existing owners. No retry budget replenishment introduced. Separate per-call duration regression above remains unresolved. | PASS for persisted execution state |
| INV-007 and task non-goals | No new durable queue, table, generic scheduler, process service, platform enforcement, host-load test or completion SLA; no future extraction/deferred/approval action introduced. | PASS |

The obligation ledger omits the mandatory test boundary, rustdoc and affected-red-gate obligations and does not distinguish inner model-call timeout from whole-execution cancellation. These gaps are reconciled explicitly above rather than silently inheriting its all-PASS table. The historical production-before-RED process exception is accurately recorded: later mutation RED proves sensitivity, not historical test-first order. No current source correction can reverse that chronology; it creates no separate remediation machinery.


## Verification and limits

Fresh orchestrator checks in verification.md:

- Cumulative `git diff --check`: exit 0.
- Four exact owning server unit tests: 4 selected, 4 pass.
- Repository-managed Postgres migrations and exact isolated-capacity journey: pass; journey 26.801 s, nextest run `95aa3179-7e79-4cdb-8278-47a6dbce95f4`.
- Repository-managed Postgres migrations and exact speculative recovery test: migrations pass, test FAIL, exit 100 in 16.857 s; run `a1ff9224-6e3a-4d3e-843d-c1328da801ca`.

Implementation-recorded fmt/lints/dependency/docs and 207 affected server checks were inspected against source but not all rerun during review. The new isolation journey proves important behavior; it cannot prove overflow refusal or preservation of an inner call budget it never exercises, nor waive test-boundary/rustdoc/red-gate rules. The fresh recovery failure output exceeded the tool budget; reviewers independently traced the missing precondition without claiming that output proves a specific branch. No broad aggregate, full journey suite, production provider, platform matrix or mutation rerun was performed. Historical production-before-RED remains an accurate process exception; it requires no retrospective source churn.

Non-goals remain excluded. Source ownership and the one-engine reservation approach are accepted, subject to the five bounded corrections. PASS is withheld until a fresh remediation review validates closure and affected paths against this original cumulative subject.
