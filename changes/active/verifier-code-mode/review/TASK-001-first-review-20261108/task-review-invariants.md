# TASK-001 invariant review

**Result: FAIL.** First cumulative review of `5234961d51798581a026834cfffd5efaadc226b4...0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`; HEAD remained the candidate when inspected. No carried-forward evidence. Reviewed approved revision 2 `changes/active/verifier-code-mode/spec.md`, original `tasks/TASK-001-bounded-code-orchestration.md`, scope.md, actual committed diff and surrounding producers/consumers. No source changes or Cargo commands.

Scope: BlockCall/ToolCalls logical invocation versus attempt; Dispatch/Halt; record production and engine settlement; hosted Invocation/Python-time reservation; pending futures and blocking workspace mutations; cancellation/timeout/cleanup. Security, wire projection, and comprehensive behavior have separate reviewers. This report makes no acceptance claim for those separate scopes.

Authorities: root AGENTS.md (no nested AGENTS.md in the reviewed runtime directories), changes/spec-driven-development.md, agent_docs/index.md, rust.md, maintainer-style.md, architecture/wyrd-design.md Agent built-ins section, approved spec REQ-009–017/INV-001/INV-006 and associated acceptance rows. `.codegraph/` is absent.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
| --- | --- | --- | --- |
| Logical invocation spans authored retries; requests never refunded | `blocks/engine.rs:657–696` creates ToolCalls outside attempt loop; `blocks/tools.rs` CallLog atomic spend and ToolCalls clone | `blocks_agent::agent_tool_budget_spans_attempts_without_refund` checks completed/cancelled calls across timeout/retry and successful final allowance | PASS |
| Budget/Busy stop prevents subsequent model work | Dispatch::spend/Halt::observe; AgentExecutor::invoke races Halt and attaches before_model callback | budget/Busy fixtures and `agent_tool_budget_spans_attempts_without_refund`; source-backed stop path | PASS for budget/Busy |
| REQ-009/012, host time limits cannot become an ordinary recoverable model result | Sandbox::exec timeout -> Terminal -> Invocation::code untyped ToolError; Halt recognizes only Busy | `code_host_waits_obey_deadlines_and_cancellation` explicitly expects a successful model answer after timeout | FAIL: INVREV-001 |
| REQ-011 non-code shared cap, cap 1 no parent/child deadlock | One Dispatch semaphore, CodeRun exempt; ToolBridge and RecordedTool share dispatch | `code_bridge_shares_the_recorded_path`, `code_calls_share_non_code_concurrency` source covers cap 1/2 and recipients | PASS for active async operations; blocking cancellation exception below |
| REQ-012 operation time includes workspace coordination | Reservation starts before workspace.writer, but timeout begins only later in Sandbox::exec | Existing host-wait test stalls Bifrost, not initial writer acquisition | FAIL: INVREV-004 |
| REQ-012/013 blocking extraction settles within cleanup and retains serialized mutation | Invocation::call holds writer in async future, blocking closure owns only Workspace Arc; dropping join detaches closure | Evidence packet explicitly has no extraction interruption test and claims Arc ownership is sufficient | FAIL: INVREV-002 |
| REQ-015 every admitted request, including cancellation while waiting, recorded once in start order | Dispatch validates input, then awaits permit, then creates record | Existing tests cover admitted in-flight cancellation, not an admitted semaphore waiter | FAIL: INVREV-003 |
| REQ-015/017 completed calls survive failed step/inconclusive check and retries | Engine owns ToolCalls and settles independent of ActionOutput/CheckOutput; Judged carries calls after citation/result rejection | `agent_records_survive_failed_steps_and_inconclusive_checks` and retry test | PASS for records actually created |
| No owning-reference cycle/new worker per child | Session/Dispatch own Invocation; Invocation owns no Dispatch; bridge borrowed by Sandbox; children invoke declared tools directly | Source tracing plus bridge cap-1 fixture | PASS |
| REQ-014 ordinary nested success does not append model conversation messages | Child call returns through Sandbox answer/resume_futures; only outer RecordedTool is installed in Skald model loop | `code_bridge_shares_the_recorded_path`; real-worker tests exercise child delivery | PASS for normal awaited success |
| Cancellation cancels outstanding async query and worker checkout | Calls FuturesUnordered scoped inside timeout; query CancellationToken drop guard; checkout dropped | `code_host_waits_obey_deadlines_and_cancellation`, `code_shutdown_settles_pending_host_work` source | PASS for async query path; extraction differs |

## Findings

### INVREV-001 — INCORRECT: operation timeout/time exhaustion still permits model work and a successful Agent result

**Obligation:** REQ-009/012, AC-009/018; original task's Host stops row explicitly prohibits continuing **Python/model work** or manufacturing a judgment after deadline. This is distinct from a catchable ordinary domain error.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:304–305,328–334`; `hosted/sandbox.rs:391–394`; `crates/vala/vala-verify/src/blocks/tools.rs` Halt::observe and Stop.

**Trace:** A declared `code.run` awaits a slow nested Bifrost query. Sandbox timeout cancels the query and returns `BlockError::Terminal("Python exceeded its time limit")`. Invocation::code preserves a typed cause only for Busy; it converts this host timeout into an ordinary Invocation tool error. Dispatch records it but Halt::observe sees no Busy and raises no stop. Skald's `loop_runtime.rs:632–716` appends the error tool result and starts the next model iteration. A following code.run with no cumulative Python time also returns an ordinary tool error, so the model can still answer successfully. This is not hypothetical: `pg_hosted_artifacts.rs:2378` (`waited.expect("the model answers after the time limit")`), 2385–2405 explicitly asserts this forbidden continuation and successful retry answer.

**Consequence/materiality:** A time-limited verification can publish a successful preparation result or judgment after the host has terminated the evidence-gathering operation. Green evidence here tests weaker behavior than the approved boundary.

**Smallest correction:** Preserve a distinguishable time-stop from the existing Sandbox/Invocation owner and propagate it through the existing Halt/before-model/session race, as Busy and request exhaustion already do. Include cumulative Python time exhaustion. Keep ordinary permission/domain errors catchable and retain cancellation/completed-call records. No new execution manager or global stop service is needed.

**Focused proof:** Extend the existing deterministic stalled-query case to assert invocation failure/check inconclusive, zero subsequent model calls or tool dispatch, cancelled query record, and successful fresh invocation. Cover cumulative-time exhaustion without replenishing on retry. Replace the assertions currently requiring successful post-timeout model continuation.

### INVREV-002 — INCORRECT: blocking extraction loses its writer guard and outlives invocation cleanup

**Obligation:** REQ-012/013, AC-008/009 and Scenario 4 Blocking extraction interruption: cleanup must await settlement; workspace mutations remain serialized and budgeted.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:263–277`; consumers `Workspace::extract` (`workspace.rs:675–770`) and `Sandbox::exec` Python writer path.

**Trace:** fs.extract acquires `held.writer()` in the async Invocation::call future and then awaits spawn_blocking. The blocking closure moves Workspace but does **not** own that guard. A code-operation timeout, program failure/return with outstanding futures, session Halt or external cancellation drops the async caller and JoinHandle. The blocking task keeps running; the writer guard and Dispatch operation permit drop immediately. Workspace Arc preserves the directory/admission permit until extraction ends, but it does not keep the writer locked or make the caller await settlement. Consequently the Agent invocation can finish while extraction is still executing. With another active direct code.run on the same workspace, its Python writes can now acquire the released writer and overlap the extraction's `work_room()` check/persist; scratch accounting is no longer serialized. The async tool cap likewise no longer counts the live extraction.

**Sibling writers/alternatives:** Python file writes serialize on Workspace::writer in Sandbox; other direct/nested extractions use this exact Invocation branch. Existing Workspace owns the writer, temporary root, scratch accounting and permit. Retaining an Arc alone is not a settlement mechanism. `persist_noclobber` protects one pathname but not the shared remaining-space calculation.

**Consequence/materiality:** Explicit lifecycle contract fails; workspace capacity can remain unavailable after the invocation reports completion, and concurrent scratch writes can bypass serialized shared-budget accounting after cancellation. This is the newly enabled nested cancellation path, not unrelated extractor hardening.

**Smallest correction:** Keep writer ownership attached to the actual blocking mutation until it settles, and have existing attempt/workspace cleanup join already-started blocking work before reporting cleanup complete. Reuse the Workspace/Invocation lifecycle; do not duplicate extraction or introduce rollback. Simply moving the guard fixes serialization but is insufficient for the explicit await-settlement requirement; simply retaining an Arc is the current insufficient implementation.

**Focused proof:** Add a deterministic barrier at the extraction work seam (no synthetic load). Start extraction, cancel/time out its owner, verify cleanup has not completed and a sibling writer cannot enter, release extraction, then verify settlement/workspace permit release/no late tool value and successful fresh invocation. The task's recorded extraction-proof gap is therefore material, not just missing optional coverage.

### INVREV-003 — MISSING: a valid admitted call cancelled while waiting for concurrency has no record

**Obligation:** REQ-015 definition of admission (name resolution and argument boundary passed, entered common recording/execution path), cancelled-after-admission retention, AC-012.

**Location:** `crates/vala/vala-verify/src/blocks/tools.rs:491–519`.

**Trace:** ToolBridge::call spends one request and resolves the declared name; Dispatch::call checks the argument object and byte boundary. It then awaits the semaphore at 502–510 **before** ToolCalls::start. At cap 1, one nested slow query occupies the permit and a second valid nested fs.read spends its request and passes both boundaries but waits here. A sibling over-budget request raises Halt (or the outer operation times out); dropping the loop drops this waiter. It has no ToolCallRecord slot, so engine settlement cannot retain it, despite producing a failed result and despite this being an admitted, budget-spending request. Both direct RecordedTool and nested calls share this site; one source correction covers both.

**Consequence/materiality:** Produced execution evidence omits an admitted cancelled request. It also cannot expose its intended recipient/zero returned bytes/cancellation as explicitly required. Existing cancellation tests only exercise calls that already crossed this await.

**Smallest correction:** Allocate the existing bounded ToolCalls record after successful name/argument validation and before waiting for the operation permit. Existing unfinished-record settlement already supplies the required cancellation/no-value representation; no queue journal/new record type is needed. Keep pre-boundary failures unrecorded and preserve the active-operation semaphore.

**Focused proof:** With cap 1, hold a first operation, admit a second valid call and halt/time out before its permit is granted. Assert both records appear once in admission order and the waiting record has intended recipient, no digest, zero bytes and cancellation; rejected name/oversized-input requests still have no record.

### INVREV-004 — INCORRECT: initial workspace coordination is charged but not bounded by the Python operation deadline

**Obligation:** REQ-012 and original task Scenario 4 time spent waiting: workspace coordination remains inside operation timeout, not merely subtracted from later allowance.

**Location:** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:304–316`.

**Trace:** Invocation::code reserves time and computes deadline, then awaits workspace.writer and spawn_blocking(workspace.mounts) without timeout. Only after both finish does Sandbox::exec enforce that already-computed deadline. A direct or nested extraction can legitimately hold this writer longer than code_timeout (extraction has its own bounded size/time behavior). A concurrent code.run consequently remains stuck beyond its operation limit until extraction releases the lock; even then the past deadline can surface as worker-capacity Busy rather than the host time stop. The enclosing Agent invocation/step deadline may be much later and does not enforce this shorter operation ceiling.

**Consequence/materiality:** The documented per-operation bound does not hold under supported concurrent extraction/code use. It also occupies reserved cumulative time and delays cancellation/recovery semantics beyond that bound. Existing Bifrost-wait coverage starts inside Sandbox and misses this await.

**Smallest correction:** Apply the computed operation deadline to initial workspace coordination within the existing Invocation::code operation, preserving the extraction settlement rule in INVREV-002. Return the same noncatchable host-time stop as INVREV-001, not Busy for an already-expired local operation. Do not add another timeout setting or alter workspace admission semantics.

**Focused proof:** Hold the workspace writer deterministically longer than a short code_timeout while the invocation deadline remains distant; assert the code operation stops at its deadline, no worker/tool/model work begins afterward, existing extraction is settled safely, and a fresh invocation succeeds.

## Verification limits

Static review only; no Cargo/test execution or host load was performed. Named checks were inspected as source and compared with their claimed evidence, not treated as proof of acceptance. Findings require independent validation by the orchestrator's fresh validator. Unrelated surfaces and exhaustive interpreter inputs were not audited here.

The orchestrator's `verification.md` was received before closeout: the three selected Vala invariant tests passed independently, as did config/conversion tests, server build and three public Rust journeys. Those runs corroborate the matrix's passing paths; none exercises the admitted semaphore waiter, initial writer-timeout or blocking-extraction barrier above. The inspected host-wait test's forbidden successful post-timeout answer remains source evidence independent of those green runs. Candidate HEAD is still `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`.
