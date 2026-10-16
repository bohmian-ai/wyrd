# Independent structured Ponytail validation — TASK-002 R1

Recommendation: **FIX_REQUIRED**. FIND-TASK-002-1 through FIND-TASK-002-4 are closed. **FIND-TASK-002-5 remains open**, narrowed to three undocumented items in `executor.rs`. No new finding ID or product decision is needed.

## Subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002`.
Original base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Prior candidate: `d342c54749a871dcacf5b0c7ec29e3b458f2d9e2`.
Candidate: `99cf2f18a8cadfe3c500cf7157934047112d156a`.

This fresh independent validation covers the complete cumulative candidate against approved specification revision 6 and original TASK-002, with remediation changes used to locate closure. Inputs include subject, frozen spec/original task, prior ledger/verdict, remediation task and recorded evidence, cumulative/remediation changes, all seven discovery reports, claim comparison, follow-up, root verification and its logs. I inspected source rather than treating report agreement, implementation claims or successful checks as acceptance proof. CodeGraph preceded source navigation; focused reads and caller searches expanded it.

Governing authority is AGENTS.md, agent-rules, applicable Wyrd design/doctrine, the reference router, spec-driven-development, maintainer-style, Rust-core, ownership patterns, Vala/evaluation/drift and testing guidance. Approved r6 replaces the former authored implementation discriminator; unrelated ownership, honest-evidence, resource and documentation obligations still apply. No Bifrost, SQL, authorization, persistent-data, SDK or foreign-runtime implementation is introduced in this diff.

Read the complete changed engine/scope/drift bodies and engine tests, shared walker and removed bucket lifecycle, assertion/comparison, trace/projection, capture and export/dependency changes. Expanded to ContextSnapshot/TaskRegistry, retained scenario and record-scoring callers, prerequisite planner/reference validation, statistical dispatch/Custom scoring and direct/scheduled completeness owners. Full bodies of every proposed correction item were inspected. No source, Cargo, index or commit mutation was performed; only this report was written.

## Complete cumulative acceptance

The proof column refers to source-inspected tests and the actual recorded commands in verification-evidence.md/review-verification.md. Root independently ran the three exact new selectors and the two-library lane; this validator did not launch competing Cargo processes.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001/002: same no-step/multi-step engine, original named evidence, omitted required versus optional null, absent selection | `VerificationEngine::run/drive`; `Scope::resolve/select/resolve_all`; supplied resolved/pinned declarations remain caller input | No-step, missing/optional, each and empty/all-skipped cases; eval library lane | PASS |
| REQ-003: reuse planner, action then step checks, dependent barrier, final checks once, deterministic output order | Existing `VerifierSpec::plan` produces stages; `drive` adapts positions; `walk` alone fans out, joins and appends ordered settlements | Barrier overlap and action/check ordering bodies; retained executor/scenario tests | PASS |
| REQ-003: references expose original inputs, status/output/upstream verdicts and explicit requirement identity/statement | Scope reads original declarations and one common ledger; status/verdict values do not require successful output; metadata requires current requirement | Scope selectors, skip/status tests, ordering/citation bindings | PASS |
| REQ-004: bounded concurrency, retries/timeouts and visible attempts/duration/final failure | Engine retains one shared semaphore; `attempt` bounds retryable failures/per-attempt timeout; `Attempted::settle` retains final state | Retry/exhaustion/terminal and paused two-attempt timeout cases; overlap test | PASS |
| REQ-005: deterministic assertions and trace semantics reused, comparison failure is data, isolated execution failure inconclusive | Shared `evaluate_single/evaluate_item_mode`, `project_spans`, injected Action/Check seam and `Judged` conversion | No-step incomparable value/missing trace, each, injected check error/failed verdict and existing deterministic regressions | PASS |
| REQ-005/INV-007: expired absolute deadline is an execution error; cancellation stops descendants | `run:146-149,165-167` checks eligibility/publication around native timeout; `walk` owns cancellable JoinSet | Exact `elapsed_deadline_refuses_ready_work`; pending-action drop proof and future-deadline empty control | PASS |
| REQ-006/INV-006: failure wins over intentional skip, including skipped owner plus failed explicit binding; blocked checks never run | `requirement:287-313` resolves explicit bindings even with owner gap and combines `Missing::strongest`; Scope and prepare propagate causes | Exact `skipped_owner_keeps_failed_check_evidence`, including omitted required slot and pure skip control; transitive/mixed/status cases | PASS |
| REQ-006: empty/all-N/A inconclusive; all/pass-rate retain inconclusive denominator, inclusive threshold and once-only counting | Common ledger plus one final `decide` fold at `engine.rs:803-830` | Aggregation table, empty/all-skipped and ordered unique results | PASS |
| REQ-014: existing PSI/SPC/Custom algorithms, per-requirement fitted state, per-slot samples/readiness, insufficient data | `DriftChecks::judge` selects by requirement/slot and delegates unchanged `score_drift`; configured Custom column null check precedes null-filtering math | Drift composition, exact Custom completeness case and unchanged drift library tests | PASS |
| REQ-014/015: full report/window provenance retained; no scoring/baseline/source policy invented | `DriftChecks::scored/inconclusive/provenance`, `Judged::checked/report` retain supplied window/detail/configuration | PSI/Custom detail and non-None window assertions; field projection inspection | PASS |
| REQ-015: attributable breakdown, original evidence and lineage, valid reachable cited passes, actual capture | Bindings' source union survives derived step output; `Judged::cited` filters unreachable slots and downgrades uncited pass; shared `capture_actual` | Ordering/citation, error/failed verdict, capture and drift tests | PASS |
| INV-001: one existing execution/state workflow, not leaf-only reuse | Both `execute_plan` and `drive` call `executor::walk`; `RunLedger<O>` owns ordered settlements; Scope borrows it; legacy snapshot projects it | Complete replacement diff and live caller tracing; both consumer families in library proof | PASS |
| INV-005/007/008: bounded library composition; preserve scenarios; no new runtime/store/framework, external effects or retired judge dependency | Existing TaskRegistry remains immutable legacy lookup; new Block/Key index immutable declarations; no independent new stage walker; new check dispatch uses shared injected seam | Source, manifest/lock and dependency check; retained old error/skip/context consumers | PASS |
| TASK-001 untouched and TASK-003/005 boundaries respected | Changed files limited to Vala engine/adapters/tests, narrow existing drift dependency and task evidence; no shared seam rewrite | Full cumulative changed-file/diff inspection | PASS |
| AC-002/003/007/010 task-local proof | Same public engine used by scripted tests; no production test-only engine or provider/interpreter dependency | 198 library tests, three exact new regressions, all-features lib/tests Clippy, format/deps/whitespace pass | PASS within original isolated task scope |
| Mandatory final touched-item rustdoc | Original retained omissions corrected/removed, but new associated types and changed regression lack rustdoc | Direct source/body comparison; green checks do not enforce the complete rule | FAIL: FIND-TASK-002-5 |

Full hosted admission, authorization, sandbox/extraction/Agent implementations, persistence, schedules/reactions, SDK/HTTP/MCP journeys and architecture replacement are assigned to other tasks. Their absence is not a new requirement for TASK-002. No changed security/tenancy/persistence boundary requires another domain decision here.

## Prior finding closure and root-cause tracing

| Stable ID | Source-based closure assessment | Status |
| --- | --- | --- |
| FIND-TASK-002-1 | `executor.rs:232-268` is the sole stage iteration/fanout/join/settlement writer. `execute_plan:328-363` and `drive:188-235` both call it; old classify/bucket/order/stage machinery is deleted. Scope holds only references to spec/evidence/common ledger. `scenario.rs:212` and `orchestrator/scoring.rs:200` still call execute_plan. Legacy snapshot extends initial context with successful common-ledger outputs; legacy executor/condition errors still abort. | CLOSED |
| FIND-TASK-002-2 | Failed action settles `NoOutput` with failed cause; explicit reference resolution retains it. Requirement preparation resolves every explicit gap before merging with the implicit owner gap. The failed ancestor and missing required slot therefore dominate false-own-when skip; no check is scheduled. The exact new case also proves a pure-skip control and aggregate inconclusive. | CLOSED |
| FIND-TASK-002-3 | Public run refuses an elapsed deadline before planning and after projection/aggregation; timeout still drops pending drive/walk. Native JoinSet cancellation and semaphore RAII remain. Ready empty/missing-binding paths now return DeadlineExceeded; future-budget empty remains inconclusive. | CLOSED |
| FIND-TASK-002-4 | `drift.rs:64-70` checks the configured metric by name before Custom's `collect_f64_non_null` loses row completeness. Null selected measurements use existing unscored report and ordinary detail/window projection. Direct host completeness and scheduled `fold_custom` count checks remain unchanged. Missing metric/type/scorer refusals and unrelated-null behavior remain intact. | CLOSED |
| FIND-TASK-002-5 | Original assertion/helper/module/constructor/private invariant omissions are corrected or removed. Fixed sample-schema/timestamp claims remain invalid and excluded. Final shared-owner write set adds two undocumented associated types and materially changes one undocumented regression. | OPEN, REVISED below |

The shared abstraction earns its retained code: RunLedger has two actual settlement contracts; Running/Prepared let the same lifecycle execute heterogeneous actions/checks and existing Eval tasks. Neither a second intact scheduler nor a mutable competing state map survives. Immutable contract-specific preparation cannot be deleted without losing old scenario behavior or coupling new blocks to the retired judge. No additional trait, framework, dependency, compatibility alias, runtime guard or test harness is warranted.

## Current discovery and follow-up adjudication

| Report/claim | Disposition | Independent evidence and reason |
| --- | --- | --- |
| behavior-rev: no new findings, FIND-1..4 closed | CONFIRMED | Callers/owners and negative paths above support closure; passing tests alone were insufficient. |
| behavior-rev: FIND-5 completely closed | REVISED | Its original-item closure is correct; its cumulative closure omits newly added associated types and materially changed skip-order test. |
| invariant-rev: behavioral/ownership acceptance and FIND-1..4 closure | CONFIRMED | Producer-to-sink gap, deadline, common-ledger and statistical traces agree with actual source. |
| invariant-rev: FIND-5 completely closed | REVISED | Same final write-set omissions, independent of its original-item table. |
| maintainer-rev: no material owner/type/workflow regression | CONFIRMED | Existing generic ledger has two real consumers; boxing large Done values and keeping different contract adapters do not duplicate lifecycle ownership. Public reexport has no repository consumer of old fields outside executor; no invented compatibility requirement is supported. |
| maintainer-rev: all mandatory rustdoc is closed | REVISED | Original listed helpers are documented; the exact three surviving items below are not. |
| system-rev: no material runtime/recovery finding | CONFIRMED | Shared JoinSet is invocation-local. Errors discard partial run output and abort siblings; existing queued/direct Eval callers still propagate through their existing owner. No server/process-wide failure or new durability promise is added. |
| concurrency domain: no new finding, FIND-1..3 closed | CONFIRMED | One JoinSet per stage, preparation before spawning, common ordered commits, permits held by child futures, bounded retries/deadlines; native abort releases resources when cancellation is processed. No synchronous preemption promise is required. |
| drift domain: no new finding, FIND-4 closed | CONFIRMED | Configured-column completeness, unchanged scoring dispatch and ordinary detail/window conversion close the source defect. Limited direct SPC-through-adapter/full-report-equality tests do not establish a new defect. |
| REPO-R1-001: missing `IntoIterator::Item` and `IntoIter` docs | CONFIRMED under FIND-5 | New explicit associated types at executor.rs:194-195 have no attached docs. AGENTS §16 expressly includes associated types, without a trait-implementation exception. Trait meaning and surrounding order docs make the correction tiny; they do not satisfy this explicit item requirement. |
| REPO-R1-001: undocumented changed skip-order regression | CONFIRMED under FIND-5 | Full body at executor.rs:547-567 now constructs the redesigned generic ordered ledger, inserting b before a while dependencies declare a before b. The representation-dependent setup materially changed; this is not an untouched sibling or whitespace-only edit. No rustdoc/Errors/Panics exists. |
| REPO-R1-001: repeat cancellation/partial-progress docs on `execute_plan` | REJECTED | Its docs explicitly link walk, whose complete abort/no-partial-stage/nonrollback contract is at :214-231. execute_plan directly awaits it; after success it only projects completed ledger entries. No separate lifecycle fact is missing. |
| REPO-R1-001: repeat cancellation docs on `drive` | REJECTED | Docs identify the shared stage walk and same-spec invariant; run documents whole-run cancellation; drive constructs stages and directly awaits the same walker. It introduces no independent cancellation owner. |
| REPO-R1-001: repeat action-drop/permit-release docs on `attempt` | REJECTED | It documents retries and per-attempt versus execution deadlines. Sole caller is the step future under walk; the block contract requires resource release on drop and run/walk document cancellation. The permit and in-flight action remain owned by that future, with no detached work. A heading is not an additional required behavior. |
| REPO-R1-001: expand `judge` synopsis/cancellation/errors | REJECTED | Its check role is documented in its summary and engine/module workflow; full body dispatches built-in or injected checks and converts errors to inconclusive. It returns Judged, not Result, and owns no independent lifecycle beyond the shared child future. Additional restatement is optional prose. |
| REPO-R1-001: repeat awaited-fetch cancellation docs on `trace` | REJECTED | Docs describe missing/unreadable evidence and null projection. Only judge calls it under run/walk lifetime, directly awaiting TraceSource with remaining budget. No detached IO, rollback or new cleanup protocol exists to document. |
| standards non-blocking import/qualified-name layout observations | REJECTED as findings | No concrete behavioral/public-contract consequence or deletable unearned code established. Do not package them as optional remediation. |
| followup-rev narrowed associated-type/test claims | CONFIRMED | Independently verified exact source and authority rather than using its vote. Its rejection of repeated async prose is supported by full caller/lifetime trace above. No unresolved contradiction remains. |

Historical TDD ordering is candidly recorded. No weakened assertion, timeout inflation, suppression or fabricated RED is required or supported. Packet tracking is not a runtime defect and this review does not request a parent-packet commit.

## Final deduplicated ledger

Exactly one retained finding; the ledger is **not empty**.

### FIND-TASK-002-5 — mandatory final touched-item rustdoc remains incomplete

**Status:** REVISED. **Classification:** VIOLATION. **Discovery sources:** REPO-R1-001 and focused follow-up; retains prior REPO-001/M-002 root-cause identity. No new FIND-TASK-002-6 is assigned.

**Authority:** AGENTS §16:723-737 explicitly requires rustdoc on every new/materially modified Rust item including associated types and test functions, Errors for fallible functions and relevant actual panic contracts. Agent-rules makes missing touched-item rustdoc BLOCK_BEFORE_MERGE. The task-review skill explicitly says missing rustdoc stays blocking.

**Exact surviving locations:**

- `crates/vala/vala-eval/src/executor.rs:194` — new `IntoIterator::Item` has no rustdoc.
- `crates/vala/vala-eval/src/executor.rs:195` — new `IntoIterator::IntoIter` has no rustdoc.
- `crates/vala/vala-eval/src/executor.rs:547` — materially changed `first_skipped_dependency_returns_declaration_order` has no rustdoc; full body returns Result from ID/DAG/registry construction and contains a registry-presence expectation and outcome assertion.

**Reachability and consequence:** the iterator is consumed by both required engines (`execute_plan` and `VerificationEngine::run`) and preserves consuming ordered settlements; its associated items are new explicit Rust items. The required legacy regression is run by the existing library lane and was materially adapted to the common state owner. These items leave the explicit documentation merge gate unsatisfied. No runtime bug or external data loss is alleged.

**Minimum correction:** add concise attached rustdoc to the two associated types explaining the owned `(TaskId, O)` settlement and consuming settlement-order iterator. Add substantive rustdoc to the changed test explaining that declared dependency order wins over ledger insertion order, plus Errors for propagated typed-ID/DAG/registry setup failures and Panics for its registry invariant/outcome assertion. Preserve all implementation and test behavior. Do not document untouched siblings, repeat async workflow prose, invent fixture failures, add consumer guards, change cancellation, create a test/check script, add a dependency, or refactor the iterator/test.

**Ladder:** deleting the required iterator or regression loses actual workflow/proof; existing/native IntoIterator and ledger already provide the necessary behavior. Reuse them unchanged. The minimum remaining correction is attached documentation, not new machinery. No new public API, product, architecture, security, cross-service, persistent-data or concurrency decision is needed.

**Focused closure proof:** inspect the three final docs against their unchanged bodies/callers and AGENTS §16, check that the source diff is documentation-only at those items, then run `mise run fmt:check` and `git diff --check`. No prose-only runtime test or fresh full library/integration/journey round is justified; the unchanged implementation already has the independently passing evidence below. Any source behavior change would require its own focused proof rather than being smuggled into this correction.

The skill's “never create a style-only round” excludes optional wording/layout churn; it cannot negate its explicit mandatory missing-rustdoc exception or stronger AGENTS gate. This retained correction is completion of an already-open mandatory finding, not a new optional style round. There is no remaining behavioral finding to batch it with; waiving it would silently weaken that gate. All rejected wording requests are omitted from the correction.

## Verification, immutability and limits

Root review-verification.md and logs show exit 0 for the exact skipped-owner, elapsed-deadline and Custom-completeness selectors; the combined vala-eval/vala-drift library lane reports **198 passed, 0 skipped**. Narrow all-features library/test Clippy, fmt:check, dependency checks and cumulative whitespace checking pass. I inspected the actual regression bodies and log results. This evidence supports behavior and compilation; it does not check every private/test rustdoc item.

Reconfirmed HEAD as `99cf2f18a8cadfe3c500cf7157934047112d156a`; `git diff --quiet HEAD` and `git diff --cached --quiet` pass. Tracked source and index remain unchanged; untracked parent packet/review artifacts remain uncommitted. Only the assigned validation report was added.

Skipped extra Cargo, integration/journeys, live provider/database and mutation-based RED reproduction per original TASK-002 and immutable review scope. Risk: FIND-TASK-002-5 still blocks task acceptance; hosted composition and end-to-end proof remain for integrated change review.
