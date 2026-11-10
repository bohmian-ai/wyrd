# TASK-003 R2 independent closure review

Result: **PASS**. **FIND-003-5 CLOSED**. Proposed material finding ledger: **empty**. FIND-003-1 through FIND-003-4 remain CLOSED. This is fresh remediation discovery/closure review, not final independent validation.

## Subject and method

Prior `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`; candidate `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9`; original base `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`. Reviewed complete remediation diff: one linear commit, three Python test files, 35 additions/38 deletions.

Read the review skill and subject first, root/SDK AGENTS, spec-driven workflow, maintainer style, applicable TESTING rules, original task and approved baseline/revision-22 context, original validated acceptance context, R1 verdict/validation/follow-up, and R2 task/evidence. CodeGraph preceded targeted navigation; its result did not index these Python symbols, so targeted reads/caller searches supplied coverage. No narrower Python AGENTS exists. No source, authority, prior report or other current report was edited; no builds/tests/subagents were run.

## Complete owner and affected-path trace

`support.py:92-120` extends the existing typed Receiver dataclass. `holds` uses `field(default_factory=dict)`, alongside its existing per-instance deliveries/Condition. `hold` creates arrival/release Events on self; `release` pops that pair and sets the same release Event. `await_release` snapshots items, signals matching arrival and waits on captured release references. Removing a dictionary entry preserves the reference already captured by the handler. The 300-second wait is unchanged.

`conftest.py:604-643` creates one received Receiver per serving context. Its installed Handler closes over that instance, calls `received.await_release(json.dumps(body))` before response selection, then routes `/v1/chat/completions` through completion (`:506-589`) to orchestration_turn (`:395-480`). The script no longer reads new shared state. Delayed deliveries still record before sleep; non-delayed delivery records remain in finally, including abandoned connections. Startup/yield/shutdown remain unchanged.

The callback now can gate any POST carrying an installed matching fragment, including hooks; it is not limited to the orchestration branch. This creates no supported regression: only the six-case test installs gates, using full role-plus-question fragments and releasing each case. An empty holds snapshot returns immediately; there is no global lookup. Reusing the existing closure avoids passing a test-owned registry through unrelated scripts or creating another workflow owner.

Session receiver (`conftest.py:654-658`) yields the same object; wyrd_server (`:661-665`) uses its URL as provider_base_url. Module desk (`test_verifier_orchestration.py:114-124`) uses that server, checked-in fixture, Gateway deployment and hydrated Service. The public test receives the same session receiver and installs its holds before public submit. Arrival observation plus public completion proves the handler/test relationship beyond compatible object shapes.

Sibling owners: watching (`conftest.py:739-776`) creates its own serving hook Receiver; delayed staging (`test_verify_in_real_time.py:1028+`) creates another delayed serving context. Both start with empty holds and preserve old reply/record/delay behavior. gateway_server (`conftest.py:865-882`) creates its own Receiver and RecordingUpstream. `gateway/support.py:177-226` Upstream.do_POST/_send only records/answers, never reading holds. Other Receiver constructors in gateway-JEV, configured-context, support-desk and local-development support also get separate default fields. Search found only speculative test hold/release calls and only serving Handler await_release. No sibling caller was silently rerouted to another fixture's state.

## R2 assertion-level matrix

S = integration/support.py; C = integration/conftest.py; P = integration/test_verifier_orchestration.py. References are candidate source, not implementation-summary proof.

| Obligation | Implementation evidence | Verification/assertion evidence | Result |
|---|---|---|---|
| R2.1 no new mutable global | HOLDS declaration/imports and free helpers deleted; S:99 default_factory gives each Receiver separate state. No HOLDS references remain; older S:54-58 Events untouched. | Complete diff and caller search establish ownership; lint alone would not. | PASS |
| R2.2 same fixture owner | C:613 creates received; C:623 reads it; C:640 yields it; C:657-658 yields the same session instance; P:264/270-284 hold/release that receiver. | P:272 asserts the returned arrival Event was signalled by the handler; P:265-281 public submit/connect/wait completes. Fresh exact journey passes. | PASS |
| R2.3 ordinary/sibling endpoints | C:619-635 preserves delay/record/write/finally except instance gate before reply; independent empty Receiver state and sibling paths traced above. | Empty snapshot returns immediately; gateway Upstream has no gate read. New placement applies to all POSTs only when that receiver has matching installed state. | PASS |
| R2.4 preserve six public cases | P:190-220 retains six rows. Holds precede submit. Action waits for extra action before comparison release; decision observes extra arrival, compare action and excluded disposition where applicable; known holds bootstrap until compare action. | P:285-306 asserts consolidated passed, exact extra status, called/never-called request, manifest speculation/names, retained disposition, reader publication value and unchanged disposition after read. Six rows cover admitted action/decision, discarded action/decision, blocked action, known-false no-call. Fresh selector selects one test looping all six and passes. | PASS |
| R2.5 captured-event wake/cleanup | S:117 captures Events; S:112-113 pops and sets same release Event. P:282-284 finally releases all three possible fragments through that method; absent entries harmless. | Decision ADMIT P:272 observes arrival while extra held; P:277 releases; P:281 completes within 90s and final succeeded/account/retained assertions pass, preceding 300s gate timeout. This exercises captured Event wake. Finally uses identical release body; no separate forced-failure test is claimed or required. | PASS |
| R2.6 scope/rules | Only S/C/P changed; no production/native/public contract/dependency/other-language/old Event edit. Typed/documented methods reuse existing owner; top-level public SDK test shape retained. | Actual immutable diff and root/SDK AGENTS plus maintainer/testing rules; supplied format/lint/typecheck and fresh diff check pass. | PASS |

Decision-first ADMIT proves comparison accepted while extra is held, then extra settles admitted after release. Requiring admission before settlement would contradict corrected approved R1 interpretation.

## Prior closure and acceptance carry-forward

Carry-forward references R1 findings-validation.md's Source and assertion-level closure matrix and Source-backed acceptance carried forward, R1 verdict.md:77-106, and original task-003-384a0def7 behavior/invariant/domain/system source/assertion matrices as reconciled by its validator. These are substantive evidence; unchanged file names alone are insufficient.

| Finding/group | Carried evidence and dependency check | Result |
|---|---|---|
| FIND-003-1 | Engine common requirement admits before spend/acknowledged begin. Waiting-check regression asserts only slow call/record before interruption and judged attempt 1/Passed on recovery under one invocation. No admission/engine/checkpoint/test change here. | CLOSED |
| FIND-003-2 | instance_requirements uses Busy-aware settlement. Controlled-time assertions prove refusal waits for sibling, accepted work/running records, recovery calls only refused. No Tasks/engine/checkpoint change. | CLOSED |
| FIND-003-3 | R1 checked all ten rustdoc/precondition/attachment corrections against bodies. No Rust body/doc/attachment edit here. | CLOSED |
| FIND-003-4 | R1 Rust action/decision/known-false accepted milestone assertions and MCP/TS mirrors unchanged. Python milestones/six final assertion sets traced above; owner movement preserves Event sequence. | CLOSED plus fresh Python proof |
| O1/O2 declarations/readiness | Original exact field/rule refusals, invalid-predecessor no-call and successor-before-unrelated-settlement assertions; planner/contracts/Scope untouched. | PASS carried |
| O3/O4 identity/fanout | Original reversed completion/ordered output, duplicate indices, invalid/over-limit zero calls, scratch isolation and frozen-membership replay; aggregation/workspace/control owners untouched. | PASS carried |
| O5/O6 conditions/joins/repeat | Original condition-before-binding reason, missing-name refusal, empty/inconclusive collection, previous/index/check-before-next/final-v2 assertions; sequencing/check owners untouched. | PASS carried |
| O7/OL finality/recovery/eligibility | Recorded-decision-over-changed-input, excluded named/output no-call, retained attempt and no repeated settled calls on recovery; drive/eligibility untouched. Python retained/read assertions preserved above. | PASS carried |
| O9/O-L35/65/66 admission/deadline/cancel | Prior actual ceilings, independent barrier, original DB deadline, live renewal/cancel and other-tenant progress plus R1 shared-check closures. No production permit/budget/lease change or quota/reservation promise. | PASS carried |
| Persistence/security/system | Prior exact File descriptors/accepted prefix/format refusal, retained conflict/WAL rejection/canonical replay/neighbor ACK, tenant/current-grant checks, both terminal-writer orders and ACK-before-retirement. No SQL/store/auth/WAL/runner change. | PASS carried |
| OC/OR/O10/OP projections/document/cancel | Prior exact cancellation error/status/details, ordered document/repeat/citation and HTTP/CLI/MCP/SDK assertions; no client/catalog/wire change. Changed Python sibling callbacks specifically traced above. | PASS carried |
| Reuse/non-goals | Existing engine/ledger/Scope/checkpoint ownership unchanged; Receiver reused, no new harness. No nested graph/quorum/stream, alternate engine/store, quota/reservation, acceptance limits snapshot or non-Rust server controls. O9.5 retired by approval, not runtime PASS. | PASS carried |

## Specialist applicability, findings and convergence

Default fresh remediation coverage suffices for one Python test owner: it includes Python rules, instance lifecycle, threaded consumer, sibling callback effects and public proof. Separate repo reviewer is unnecessary for these three files; prior cross-tier coverage survives outside this expressly reviewed rule. Separate concurrency/invariant/system/security/persistence reviewers are unnecessary: matching/wait/set semantics moved to the existing fixture without new coordination policy or production admission/durability/grants/deployment boundary. Actual gate lifetime and siblings were fully traced; hypothetical same-process parallel use does not establish an additional required boundary. Prior domain/invariant/system evidence cited in R1 covers unchanged production contracts. Existing dataclass reuse/deleted helpers create no substantial new abstraction needing maintainer specialist. UI/provider-wire specialists have no affected contract. Fresh final validator remains required; this report does not replace it.

Explicit material proposal ledger: **empty**. No source-proven supported regression or mandatory-rule violation remains. New locks, teardown registries, duplicate-fragment concurrency semantics, forced-failure tests or all-language timing duplication lack an approved obligation. Successful captured-event wake and identical finally release provide sufficient focused proof.

Second remediation review converges: R1 closed four findings and introduced one bounded explicit Python ownership violation; R2 fixes that shared owner. No incomplete fix, fix-induced supported regression or expanding expectation requires another remediation.

## Verification and immutability

Current verification.md records orchestrator fresh exact-candidate selector: **1 passed in 13.46s**, including final doc wrapping; both migration-readiness tests pass. It records clean remediation diff check. R2 evidence supplies Python format/lint/typecheck success. Reviewer did not execute runtime/check lanes. Static source establishes ownership compliance; green journey supplies supported wake/outcome proof.

Conclusion: HEAD exactly matches candidate, candidate-relative crates/ and sdks/ diff is empty, all 12 pinned authority/evidence hashes pass. Pre-existing dirty skills/untracked packets remain outside write scope. Only this assigned report was written.

Skipped: reviewer runtime execution, broad original rediscovery, forced-failure/parallel matrices and full SDK/platform aggregates. Risk: historical and immediate-close fixture limits remain disclosed in prior reports; no additional guarantee is certified here.
