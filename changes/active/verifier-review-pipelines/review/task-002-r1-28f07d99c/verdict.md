# TASK-002 R1 remediation verdict

**FIX_REQUIRED** — retain only **FIND-TASK-002-5** (O5.2 lost-response proof) and **FIND-TASK-002-7** (new changed-signature imports). FIND-TASK-002-1, -2, -3, -4 and -6 are closed. No retained finding alleges an unresolved production receipt, attempt-retention or security defect.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Original cumulative base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
Prior candidate/remediation base: `75c9bfa9e07163099eac40d5a556c4447c6a519d`.
Reviewed candidate: `28f07d99cee44669cd4a8209d1f096ee19c5dfd4`, branch `task/verifier-review-pipelines/TASK-002`.
Review range is prior candidate to new candidate, including first fix commit `cc4165193` whose parent is the prior candidate. No rebase/ancestry ambiguity. Original task, original verdict/validated ledger, R1 remediation, both candidates and cumulative context are retained.

This is the first remediation review, scoped to finding closure, the 15-file fix diff and affected callers/consumers/invariants. No comprehensive original-task/performance restart. Source-backed accepted path/snapshot/JSON/null/producer/selection/thin-projection and ordinary direct/queued evidence carries from `../task-002-75c9bfa9e/findings-validation.md` and its selected reports, checked against changed dependencies. See `subject.md` for navigation and exact reopened boundaries. The obligation ledger contains 28 actual stable O IDs, not the claimed 30; every ID is reconciled below and the count typo creates no new finding.

Authority is approved baseline revision 18 with explicitly approved publication/durability corrections. Unrelated revision-21 draft wait/approval is excluded. Before review freeze, the invoked wyrd-plan corrected the original task's private grant-removal sentence to match existing security snapshot/later issuance/principal-cancellation authority. A bounded independent document comprehension pass found no material ambiguity; `planning-correction.md` records it. No spec/runtime/security behavior changed. Frozen hash manifest includes resulting original task, remediation, supplements/spec and preexisting skill edits. Earlier evidence/admissions and review artifacts remain preserved.

## Reviewer selection and validated coverage

| Role | Report | Result |
|---|---|---|
| Fresh domain-rev data: receipt, invocation/forget/restore and changed recovery consumers | [domain-review-data.md](domain-review-data.md) | FIND-1/-2 closed; proposes O5.2 proof gap |
| Fresh domain-rev security: receiving File authority/audit, fixtures and sensitive denials | [domain-review-security.md](domain-review-security.md) | PASS, FIND-3 closed |
| Fresh repo-rev: changed cross-tier/language proof and mandatory test/helper rules | [standards-review.md](standards-review.md) | Surface/proof closure; proposes changed-signature import violation |
| Fresh followup-rev: exact recipient/loss boundary in conflicting O5.2 interpretations | [followup-review.md](followup-review.md) | RESOLVED: first receipt is delivered before reclaim; replay is after observed success |
| Fresh ponytail-rev: all closure, source/materiality/correction burden and specialist applicability | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; retains only FIND-5/FIND-7 |

The validator independently checked all selected/current and carried-forward evidence. Separate behavior/invariant/maintainer/system/concurrency discovery was unnecessary: small fixes extend the same receipt/invocation/authorization owners with no new loop/store/cache/IO/task/lock/queue owner. Targeted DATA covers changed persistent identity and retry lifecycle; SECURITY covers changed receiving authority; REPO covers broad changed test surfaces; prior source-backed whole-workflow/failure/concurrency coverage carries the rest. UI/provider/GitHub integration specialists remain inapplicable. No required report is missing. No material new issue or applicability gap originated in validation.

The one follow-up resolved the material difference between a successful reply lost to nobody and actual uncertainty before caller delivery. Unique import proposal went directly to independent validation. No reviewer-count vote or extra discovery pass was used to force agreement. Claim routing is in `claim-comparison.md`.

## Stable findings and closure

| Finding | Disposition | Source-backed result |
|---|---|---|
| FIND-TASK-002-1 | CLOSED | One shared receipt checks producer/name OR original call, then exact name/call/digest; live/restored/Agent proofs retain conflicts and fresh-attempt isolation. |
| FIND-TASK-002-2 | CLOSED | Shared forget preserves published attempts; real recovery, exhausted-retry and before-effect controls prove spent attempts and original content. |
| FIND-TASK-002-3 | CLOSED | Existing File authorization runs before work/replay; exact tool/File audit subject, actor and full delegation values, JSON exclusion and denial-before-IO are asserted. |
| FIND-TASK-002-4 | CLOSED | Actual Python/TS/MCP invalid selection/domain denials, real Agent CLI inspection, cards route/stable codes and served enum assertions close required public observations. |
| FIND-TASK-002-5 | CONFIRMED, narrowly retained | DATA-R1-001/FOLLOWUP-O5.2: first receipt reaches Agent and drafter settles before Busy/reclaim, so later same-ID replay with missing scratch proves reuse after success, not unobserved first reply. O5.1/O5.3–O5.5 close. Add only a small existing server-seam response-loss observation and correct unsupported rustdoc; no production/network/API machinery. |
| FIND-TASK-002-6 | CLOSED | Real Agent loop proves cumulative work budget, replacement credit, typed refusal/zero-byte record and unchanged prior bytes. |
| FIND-TASK-002-7 | CONFIRMED, narrowly retained | SSR-R1-001: original panic/local-import sites fixed, but added publisher/agent_engine/state-test signatures still qualify Caller/InvocationRecord/Value contrary to explicit Imports/signatures rule. Module imports/bare names only; batch with FIND-5. |

Retained ledger's exact locations, violated obligations, reachable observations, materiality, correction burden, whole-source/sibling tracing and smallest correction are independently established in `findings-validation.md` and repeated self-contained in the R2 task. Stable IDs are preserved; no new FIND ID assigned to the same proof/rule causes. The standards O5.2 PASS interpretation is rejected from the actual first-reply delivery trace, not converted to optional advice. No historical TDD repair, speculative optimization, live grant resolver or new capture policy is prescribed.

## Reconciled obligation matrix


Locations are candidate source; full paths are given where first needed. Execution classification follows the table. PASS is source/assertion-backed acceptance, not inference from a test name.

| ID / required observation | Implementation and exact verification evidence | Result |
|---|---|---|
| O1.1 same accepted call cannot take a new name | `verification/state.rs:961–978` matches producer plus name OR call ID, then exact name/call/digest. Rust SDK `hosted_agent_tools.rs:1080–1113` asserts two original receipts, typed `publication_name` on both changed calls, and exact listing without the second name. | PASS |
| O1.2 changed value conflicts, original retained | Same helper; SDK `:1089–1095,1135–1145` asserts conflict and retained JSON `once`. | PASS |
| O1.3 identical replay, competing-call conflict, fresh attempt reuse | State owner test `:1436–1470` asserts Replayed, conflicts, attempt-2 Accepted, and exactly two retained values; SDK pins per-attempt listings/readbacks. | PASS |
| O1.4 restored identity rule | Owner `a_call_keeps_its_original_publication` rebuilds RunState from the accepted snapshot; its loop asserts exact replay and all changed-call/name/value conflicts on restored entries. | PASS |
| O2.1 post-publication Busy consumes attempt, retains content, permits attempt 2 | `RunState::forget:1022–1037` preserves published step/attempt; engine `:329–349,838–844` reconstructs spent attempts. Server `pg_durable_verifier_work.rs:1600–1635` asserts accepted draft/once attempt 1, successful draft attempt 2, both exact retained values. | PASS |
| O2.2 exhausted retry cannot restart attempt 1 | Same engine bound; server `:1644–1656` asserts Failed/attempts 1, two unconsumed model responses, original retained value. | PASS |
| O2.3 before-effect Busy stays free deferral | Guard applies only with retained publication; server `:1600–1606` excludes early from retained attempts and `:1658–1663` asserts successful early attempt 1 and consumed remaining responses. | PASS |
| O2.4 all forget callers share invariant | Full engine requirement Busy/definition and action Busy/definition arms (`:786,878,882`, through `:390–397`) call the same Checkpoints owner. Requirements cannot publish; settled records still survive the existing retain predicate. No downstream copies added. | PASS |
| O3.1 separate tool/File Allowed, exact verifier grant, attribution | Publications path `:142–148` reuses existing authorization. `hosted/tools/pg_tests.rs:426–493` publishes nonempty tree under only exact-UID verifier grant; exact ordered event tuples distinguish tool/File/JSON/replay. `:372–396` compares every event's subject/actor and complete serialized delegation detail for distinct executed_by and nonempty delegation. | PASS |
| O3.2 tool-only receiving denial before IO | Same test `:495–508` asserts exact RBAC denial and [invoke allowed, File denied] on missing scratch path. Authorization precedes receipt/freeze/completion. | PASS |
| O3.3 replay no upload | Test removes original tree before replay, compares original directory receipt; source returns at `publications.rs:156–157` before freeze/upload. | PASS |
| O3.4 valid direct success fixtures, denial preserved | Leased::publisher `:148–160` grants exact claimed Verifier; VerificationFixture::publish `:706–720` changes only its local accepted principal. Existing backend success coverage supplied; O3.2 retains denial. | PASS |
| O3.5 JSON gains no File decision | Invocation JSON branch dispatches directly to Publications::json. Exact audit-event list in O3.1 contains only JSON invoke between path calls, detecting extra File decisions. | PASS |
| O4.1 Python invalid selection pre-dispatch | `test_hosted_agent_tools.py:225–249` loops four invalid fixtures; asserts stable loader code, `agent_tools` and exact reason, no Card, no NEVER CALLED request. | PASS |
| O4.2 Python invoke-granted domain denial | Reuse existing `test_verifier_building_blocks.py:706–717` no_domain_grant with tool/card permission but no query grant; `assert_tools:350–386` checks ordered calls, DENIED, None digest/zero bytes and permitted siblings. No new live grant removal semantics. | PASS |
| O4.3 TS invalid selection pre-dispatch | `hosted-agent-tools.test.ts:415–443` asserts same four stable loader reasons, exact Card-not-found, no NEVER CALLED dispatch. | PASS |
| O4.4 TS invoke-granted domain denial | Existing `verifier-building-blocks.test.ts:772–785`, `assertTools:333–367` pins ordered records, denial, null digest/zero bytes and successful permitted siblings. | PASS |
| O4.5 MCP selection/invoke/domain refusal | `wyrd-mcp/tests/bifrost/mcp/verification.rs:3672–3693` requires nonempty denied records/zero bytes/no digest and empty publication page. `:3696–3762` actually enqueues via MCP with tool but no query grant, asserting successful cards.get and denied query/no bytes/no digest. `:3765–3796` checks invalid loader reasons and no provider dispatch using existing client; MCP has no declare tool. | PASS |
| O4.6 thin CLI over actual Agent execution | SDK `hosted_agent_tools.rs:981–1037` uses the real Agent execution ID; asserts exact four names, file kind/size, tree directories, JSON/null and publication-not-found code. Same execution's SDK read assertions retain content proof. | PASS |
| O4.7 route and served schema | `pg_verification_routes.rs:2388–2447` asserts 400/stable validation code/agent_tools/exact reason, absent Card and valid tool-name readback. `pg_openapi_contract.rs:1688–1710` resolves actual served enum, requires four canonical built-ins and rejects provider spelling. | PASS |
| O5.1 Agent publication, unavailable inventory, recovered exact consumer, no drafter rerun | Server `pg_durable_verifier_work.rs:1725–1787,1820–1905` actually runs Agent write/publish then Busy; restores only settled draft, asserts exact unavailable-inventory reason, final attempts, only two remaining model responses, accepted invocations once each, mounted KEPT bytes and consumer digest/length/error equal exact read serialization. | PASS |
| O5.2 lost reply after acceptance, original call/attempt replay without acceptance/upload | `:1797–1817` strongly proves recovered original call_1/draft/1 replay with missing scratch and unchanged revision. But `:1725–1750` finishes the successful drafter reply before Busy/reclaim; loop `skald-agent/src/loop_runtime.rs:632–695` awaited and delivered the first tool result before that next model response. No response is lost/suppressed at this seam. | FAIL — FIND-5 |
| O5.3 deterministic direct disconnect, no later call | `:1910–1992` HeldRevisions pins begin/publication arrivals and exact remaining-response counts, drops owned engine future before release, then checks two untouched responses and no after publication after reclaim. No sleep. | PASS |
| O5.4 SDK keeps only public outcomes | `publications_follow_shared_run_state:1579–1629` submits and waits publicly, asserts passed consumer, both attempts 1, exact report/null and four publications. No private crash/flush/state/deadline expiry/sleep remains. | PASS |
| O5.5 no SDK digest computation | Removed digest helpers/algorithm calls; Rust constants `:66–76`, Python DIGESTS `:107`, TS DIGESTS `:236`, fixed receipt digest preserve typed plan/digest/size/content assertions. Server owner-tier calculations remain appropriate. | PASS |
| O6.1 cumulative write budget, replacement, failed record, unchanged file | SDK `:1505–1545` asserts work_budget on cumulative and replacement overflow, stable validation error and no digest/zero bytes, accepted replacement bytes `room()+1`, and prior b readback. Script includes big plus a/b and both rejected growths. Existing Workspace::write remains owner; supplied mutation removes replacement credit and fails the named assertion. | PASS |
| O7.1 changed helper docs/imports | Original builder/reply/invocation # Panics and MCP module imports are corrected, forbidden digest helper deleted. New R1 signatures still violate the same explicit import rule: server `:154,1438,1441`, state test `:1370`. Original named sites close, changed-item gate remains open. | FAIL — FIND-7 |
| O8.1 task authority correction and accurate history | Frozen original task Scenario 1 and appended planning correction distinguish snapshot/later issuance/principal cancellation and actual Proof-5 limits; planning-correction.md records independent document comprehension. Original TDD admission and supplied evidence limits are retained. | PASS |

Cross-cutting observations: unchanged accepted safety/content/selection/direct-queued foundation obligations carry forward with the prior validated matrix/reports, checked against the fix's dependencies as above. No new duplicate owner, schema/generator input, dependency, platform gate, runtime credential policy or non-goal implementation entered this diff. Historical production-before-RED remains a failed historical process observation; it is neither a repaired PASS nor another actionable runtime finding.


## Verification, limits, and handoff

Root independently reran five focused exact tests, all passing with nonzero selection: receipt owner; receiving File-audit owner; retained-Busy; recovered-consumer; dropped-direct-work. Managed Postgres migrations passed; command/Nextest records are in [verification.md](verification.md). Root full remediation diff --check is clean. Other SDK/MCP/CLI/HTTP/schema and formatting/clippy/deps/tenant/language executions are supplied evidence whose assertions were inspected, not claimed as reviewer reruns. Generation inputs did not change; no codegen/full suites/benchmark/load or unrelated foundation matrix rerun. A green existing test cannot prove its missing lost-response observation.

Original production-before-RED remains a recorded failed historical process fact, not repaired history or another actionable behavioral finding. R1 product fixes record genuine RED/GREEN; verification-only proof may pass first. No after-two-unsuccessful-remediations convergence assessment is triggered on this first remediation review. The next scope is bounded to actual incomplete O5.2 proof and fix-introduced signature import instances; no missed original runtime defect or expanding acceptance expectation was established.

The pre-review planning correction is complete and source-backed. Candidate HEAD, all seven frozen hashes and preexisting skill-edit boundaries rechecked unchanged. Source/spec/skills were not edited by review; only the separately invoked task wording correction occurred before freeze. No commits, merge, push or deployment.

Remediation: [TASK-002-R2-lost-reply-proof-and-imports.md](TASK-002-R2-lost-reply-proof-and-imports.md), directly to `$wyrd-implement`. It requires the smallest test-only response-loss observation plus module imports/static docs in the same edit, and only affected named server tests and normal owner checks. Existing closed evidence carries to the next candidate; task completion remains gated by PASS.
