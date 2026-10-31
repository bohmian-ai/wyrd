# Independent remediation validation — TASK-009

Recommendation: **PASS for TASK-009-R1's bounded remediation**, with no retained new implementation finding. This does not complete original TASK-009: FIND-TASK-009-1's final-source integrated proof and original V14–V16 remain pending under the user's explicit post-review sequence.

## Immutable subject and independent method

- Original task: `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md`, revision 2; approved `changes/active/benchmark-experiments/spec.md`, revision 24.
- Original reviewed range: `6f38db16c524417301ffc4a5dc0b478daf997ed1..f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`. Prior verdict, ledger and reports remain in `../TASK-009-R1-f0ea3b402/`.
- Remediation range: `git diff f0ea3b402 b6e69d66b`, cumulative candidate `b6e69d66b5657881f7d605a0e308b2985464c420`, independently checked HEAD. `a8590dcc8` records the prior review; the remaining commit performs remediation.
- Remediation authority: prior `TASK-009-R1-complete-proof-and-authority.md`, including its focused acceptance and Implementation Evidence.
- Selected discovery report: `task-review-behavior.md`. Read this round's scope, verification and comparison reports, prior verdict and validated ledger, relevant original task/specification obligations, root/nested rules, skill, workflow, maintainer/Rust guidance, architecture map and complete owning group/schedule/publication paragraphs. Checked changed-directory ancestors for nested AGENTS; SQL rules apply to traced SQL ownership.

This validator is separate from remediation discovery and implementation. CodeGraph preceded code navigation. Actual delta and source, not reviewer agreement, establish closure. Independently inspected the complete changed V18 journey; Run/Window Observe dispatch and enqueue; observed Service/Model/Verifier fixtures; scheduler candidate/settled handoff and Drop; group acceptance, activity generation upsert and conditional deletion. No changed runtime implementation, persisted contract, feature, dependency or SDK surface invalidates the previously accepted admission/recovery/tenancy evidence. Unchanged acceptance is carried forward rather than rediscovered.

## Stable prior-finding ledger and dispositions

| Stable ID and prior discovery | Validation disposition | Bounded correction and evidence | Remaining original obligation |
|---|---|---|---|
| FIND-TASK-009-1 / B-01, REVISED, MISSING | Prescribed evidence correction accepted; original proof obligation DEFERRED, not erased | Original task's `Later executable source and closeout status` identifies later `da366eefe`, `f0ea3b402` and this remediation, preserves earlier `e9e3d95cb` green attribution, links remediation proof and explicitly marks V13–V16 pending | Final-source V13 must still identify settled SHA, invocation, overall exit, lane outcomes and log. No whole-task completion is established |
| FIND-TASK-009-2 / B-02, REVISED, MISSING | CLOSED within stated verification limits | V18 now pulls declared bundles, hydrates WyrdStates, starts Bifrost, records Window sample with Observe::submit, queues grouped Run with Observe::submit and calls Observe::verify under held archival. Ordinary returned Execution supplies staged ID; typed status supplies fixed group readback | Final integrated surface proof remains part of original V13 |
| FIND-TASK-009-3 / DP-001, CONFIRMED, INCORRECT | CLOSED | Wyrd group paragraph at 1638 onward now describes preparation rollback, reserved publication, own-record ACK and tenant admission with selected-generation consumption; concurrent activity gets a fresh generation. Wyrd schedule freezes at preparation. Bifrost at 1827 onward describes the same ACK-first order | None specific to this finding |
| FIND-TASK-009-4 / REPO-001, CONFIRMED, VIOLATION | CLOSED | `scheduler.rs:148–151` documents final-holder release, shared-set removal and poison recovery; Drop body is unchanged | None specific to this finding |

The independently validated **new/remediation implementation finding ledger is empty**. No next FIND number is assigned. FIND-1 remains recorded above as an existing deferred original-closeout obligation, not a rejected finding or a newly invented source defect. No conflicting claims, new blocker or untraced common cause requires focused follow-up or another source validator.

## Source and correction-boundary checks

For FIND-2, the complete journey at `verification_runtime.rs:7632` uses existing Cards/CardGraphPuller/WyrdState owners. The local hydration closure shares the two caller-owned setups and retains their temporary bundles. The observed fixture binds the Model's `latency` sample to the daily Window Verifier; `Observe::record_or_stage` projects Window features through `drift_value` and returns Recorded, while Run projection captures grouped evidence and `enqueue_run` reuses the existing Target. This executes the previously bypassed supported authoring path. Raw enqueue/execute bodies, write_batch and private schedule declaration no longer stand in for those operations. Existing activation/due controls determine timing only.

The assertions still refuse a visible held submitted execution, call direct verification while input archival is stalled, refuse premature background queue insertion, await three completed origins on first attempts, and read the exact held turn under both grouped source slots. No held-ACK assertion, exact readback, timeout or test selection was weakened. No parallel harness, new public API, new authoring fixture or production admission change is warranted. Existing owners already solve the correction; deleting the test's public authoring would reopen the explicit proof gap.

For FIND-3, the complete group section preserves cumulative cutoff/order/null selection, incomplete-source refusal, auth/infrastructure distinction, retention and prequeue-orphan limits. Preparation of a new candidate differs from unchanged-content and terminal-refusal settlement, whose explicit transactions remain intact. Actual `prepare_group` resolves/deduplicates without a new execution; scheduler releases candidate TenantConn before staging; `accept_grouped` inserts/replays then uses the carried generation for conditional consumption on its caller's tenant transaction. The existing upsert generates a new token on activity and the generation migration initializes new rows. The corrected owner paragraphs now agree with those producers/consumers and existing ACK-first sections. No migration, SQL mechanism, admission guard or stronger cross-store guarantee was added or required.

For FIND-4, method rustdoc directly describes the lifecycle and matches final Arc ownership release/removal with poison recovery. Mandatory changed-item documentation is satisfied. Runtime testing of documentation alone would add no proof.

## Acceptance reconciliation and pending completion

| Obligation | Source/verification evidence | Result |
|---|---|---|
| Bounded FIND-1 evidence attribution | Original evidence update and remediation reference; older green and later executable changes distinguished | PASS bounded correction; final-source V13 pending |
| V18 actual Run/Window/public direct authoring with ACK/readback assertions | Complete changed journey, existing fixtures and Observe dispatch inspected; implementation raw log corroborates exact selector 1/1 | PASS within provenance limit below |
| Owning architecture matches ACK/generation admission | Complete revised paragraphs compared with existing preparation/admission/generation owners | PASS |
| Mandatory destructor rustdoc | New method documentation, unchanged method body | PASS |
| REQ-036/037, AC-016–018, relevant invariants, exact work/query counts | Prior validated acceptance retained; no runtime/dependency/test-count change invalidates it | PASS carried forward within original evidence limits |
| Non-goals and Ponytail burden | Existing SDK/fixture/test owner reused; no new framework/store/API/guard or stronger durability | PASS |
| Original REQ-038/AC-019 final integrated proof and REQ-039 measurement/delivery | Honest pending V13–V16 statuses; accepted older L1 records retain their SHA/history attribution | NOT COMPLETE, explicitly downstream |

The skill's PASS completion rule applies to the subject being accepted: this user requested review of TASK-009-R1 remediation with bounded closure, and the remediation itself assigns final V13 to original integrated closeout. Its FIND-1 acceptance explicitly requires honest pending attribution and prohibits claiming whole-task PASS while V13 is pending. The current user also expressly sequences gate/measurements/records/PR after this review. Accepting that completed bounded correction does not waive REQ-038/039, replace the prior original-task verdict, or authorize marking TASK-009 complete. A verdict must state that scope explicitly. Another implementation remediation containing the same pending gate would add no correction and conflict with the prescribed narrow remediation proof.

## Verification limits

Directly read the raw implementation `v18.log` at the external path in verification.md: migrations, build:server, recompilation and nextest run `b379e204-1d3a-4f1e-9018-d48fbcc5a7f3`, 1 test passed, 55 skipped, 33.473 seconds. This corroborates the remediation's recorded narrow proof and nonzero selection. It contains no source SHA, so it is not an independently pinned final-candidate execution. Independently read `/tmp/task-009-r2-v18.log`: fresh review attempt failed before migrations/build/tests because Docker socket access was denied. That environment refusal is neither test failure nor test pass. Static source closure plus the recorded execution and raw corroboration is credible for this bounded existing-journey correction; final-source integrated proof remains outstanding explicitly.

No Cargo, Docker, gate, benchmarks, publication, commit, push or source edit ran in this validator. This report is its only write. Prior reports and the unrelated untracked diagram remain untouched; HEAD remains the recorded candidate.

Skipped fresh execution and broad rediscovery; original V13–V16 still govern final-source completion. Risk: no fresh reviewer-run V18 or final-source aggregate is established by this review.
