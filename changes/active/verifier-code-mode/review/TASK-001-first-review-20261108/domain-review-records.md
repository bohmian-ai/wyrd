# Persisted and wire evidence review

**Result: FAIL.** Two material evidence-integrity gaps remain.

Subject: base `5234961d51798581a026834cfffd5efaadc226b4`, candidate `0e51af2a2332f362ab44b7c5b88b51c3cb4f8011`. HEAD remained the candidate during inspection. This is independent static discovery of the records domain; no source, task, or specification edits, Cargo runs, or subagents were used.

## Boundary and authority

Reviewed the approved revision-2 specification, original TASK-001, review scope, cumulative diff, root and TypeScript directory rules, relevant Verifier architecture, Bifrost sensitive output-table declaration, workflow and maintainer guidance. `.codegraph/` is absent. The governing obligations here are REQ-014–017, INV-001/006 and AC-012–013, including task-specific failed/retried/cancelled record and sensitive-result preservation. TASK-002 authored language/MCP workflows are excluded; generated mechanical projections are included.

Navigation and source coverage:

- `blocks/tools.rs`: shared dispatch, admission/bounds, request counters, start/finish records, cancellation settlement, direct and nested recipients.
- `blocks/agent.rs`, `blocks/engine.rs`: action/check outputs, retries, failed output/Verdict/citation paths, engine settlement and record handoff.
- `hosted/tools.rs`, `hosted/sandbox.rs`: ordinary protected capability dispatch, JSON-to-Python conversion, future answering and return failures.
- `wyrd-spec/src/verification/execution.rs`: ToolCallRecord, ToolRecipient, RequirementResult, generic custom-data decoding, historical defaults and contract regression test.
- `verification/results.rs`: StoredOutput borrows full requirement/step results without manually enumerating their record fields. `verification/inputs.rs::InputResolver::result` restores verdict/evidence and deserializes the full output. `components/verification/service.rs::get_execution` uses an authenticated ScheduledQueryCaller, and the direct execution response uses the engine outcome.
- `ExecutionOutputsTable` declares `output` sensitive and gates its projection by the existing elevated payload permission. The new records remain inside that same Variant; summary/status coordination fields do not acquire an argument projection.
- Rust SDK re-exports; shared Rust client immediate/status result deserialization; generated TypeScript requirement/recipient types. Existing direct/catalog/capability callers and sibling step/check consumers were followed, rather than treating the changed files as the entire boundary.
- Proof inspected: Vala `agent_records_survive_failed_steps_and_inconclusive_checks`, `agent_tool_budget_spans_attempts_without_refund`; wire `verification_result_round_trips_documented_outcome`; hosted JSON conversion test; Rust public reduction, failure-release, and retained-output helpers.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Additive recipient/default; default-empty requirement calls | execution.rs defaults and explicit enum; new record construction always sets recipient | Wire round-trip checks both recipients and absent historical fields | PASS |
| Preserve calls through typed custom-data decode | RequirementResult::decode_data transfers `self.tool_calls`; VerificationOutcome/Result delegate through it | Wire regression checks typed conversion retains records | PASS |
| Preserve completed records across retries and failed/inconclusive output settlement | Engine creates ToolCalls outside action retry loop; Attempted::settle transfers it for both success/failure; Agent-check error/citation result receives the same settled calls | Vala failed-result and retry-budget tests; Rust failed/cancelled phase and retained equality assertions | PASS, subject to findings below |
| Record every admitted call, including one cancelled while awaiting shared capacity | Dispatch validates arguments before semaphore acquisition, but starts the record afterwards | Existing cancellation test covers an executing pending query, not an admitted permit waiter | FAIL: DATA-002 |
| Digest/count describe the value actually delivered; no-value delivery has safe error/no digest/zero bytes | Dispatch finalizes a successful JSON result before Sandbox::answer performs fallible recipient conversion | Conversion unit test deliberately rejects u64::MAX, but does not assert associated execution record | FAIL: DATA-001 |
| Persist/read/project the additive records without a parallel store | StoredOutput serializes full requirements/steps; InputResolver deserializes full outcome; SDK uses shared types; generated TS matches new fields | Rust reduction journey compares immediate/retained requirements and steps, including leaf digests/bytes and custom data | PASS |
| Keep nested raw results outside model conversation and retained leaf record | Bridge returns values to Python; ToolCallRecord stores args/digest/count/error only; ordinary outer code output reaches model | Reduction journey checks model messages and matching direct/nested leaf digests | PASS |
| Preserve sensitive result authorization and avoid a new disclosure surface | Existing sensitive output Variant and authenticated result query retained; no new summary record fields carrying arguments | Source coverage; capture-disabled scenario has no dedicated executed proof in supplied evidence | PASS structurally; proof limitation below |

## DATA-001 — Successful record is finalized before fallible delivery to Python

Classification: **INCORRECT**. Severity: material data-integrity/public-contract failure.

Violated obligation: REQ-015/016 and AC-012 require digest and byte count to describe the successful bounded value actually delivered to the intended recipient; no delivered value must mean no digest, zero bytes and a safe failure.

Location: `crates/vala/vala-verify/src/blocks/tools.rs:537–565`, together with `crates/wyrd/wyrd-server/src/verification/hosted/sandbox.rs:489–510` and `:800–803`.

Proven path:

1. A permitted declared tool returns a bounded JSON value. Dispatch computes its SHA-256/count and calls `self.calls.finish(slot, record)` as a success.
2. The nested future later reaches `Sandbox::answer`. Only there does `python_from_json` run. It may fail; the code turns that failure into a catchable RuntimeError without revising the already-finished leaf record.
3. For a concrete supported tool result, an Experiment Card can contain `default_parameters.n = {"type":"json","value":9223372036854775808}`. `ParameterValue::Json(serde_json::Value)` in `wyrd-spec/src/card/common.rs:24` and `ExperimentSpec.default_parameters` permit that ordinary JSON value. Declared, authorized `cards.get` reads the Card through `GetCardArgs::read` and serializes it unchanged (`hosted/tools.rs` CardsGet branch). The result is tiny relative to the default result bound. `to_python` refuses this unsigned JSON integer because `as_i64()` fails and `is_u64()` excludes the float path. The unit conversion test itself explicitly requires rejection of `u64::MAX`, independently confirming the branch.
4. Python can catch this ordinary conversion error and finish normally. Even if it does not catch it, the engine now intentionally retains the completed leaf record on produced failed/inconclusive results. Both immediate and retained results therefore claim a successful code-directed value/digest/count that Python never received, and omit the actual failure.

This does not depend on deciding whether the bridge should support unsigned integers: even a permitted conversion refusal must produce an honest no-value record. Increasing the supported range alone leaves the same ordering defect for other conversion failures.

Smallest correction boundary: keep the existing shared dispatcher/ToolCalls owner, but make nested record completion include the existing host delivery-conversion outcome. A rejected conversion must finish that admitted slot with the safe failure, no digest and zero bytes; successful conversion keeps the current bounded JSON digest/count. Do not add a new journal, raw-result store, duplicated capability dispatcher or downstream result-reader repair. Direct records and pre-dispatch rejection behavior stay unchanged.

Focused proof: extend the existing actual-worker bridge boundary fixture with an authorized Card carrying the unsigned JSON parameter, catch the conversion failure and return a valid result, then assert the nested `cards.get` record has recipient `code`, error set, no digest and zero bytes. If integer transport is expanded instead, retain a separate reachable result-conversion refusal case and the same no-value assertions. The proof must inspect the execution record, not just the conversion helper.

## DATA-002 — Admitted calls cancelled while waiting for a permit disappear

Classification: **MISSING**. Severity: material evidence completeness failure.

Violated obligation: the spec explicitly defines admission as passing invocation-local name and argument-boundary checks and entering the common execution/recording path. REQ-015 and AC-012 require every admitted request, including cancelled calls, to appear once in invocation start order. Only target/argument rejections are excluded.

Location: `crates/vala/vala-verify/src/blocks/tools.rs:502–519`; nested caller at `:588–591`; cancellation projection at `ToolCalls::settled`.

Proven path:

1. At effective concurrency cap 1, Python gathers two valid declared non-code requests. The first holds the shared permit on pending host work.
2. The second spends its allowance, resolves its declared target, passes object and byte-boundary validation, enters Dispatch::call, and waits in `self.permits.acquire().await`.
3. An authored step timeout cancels the invocation and can still produce a failed StepResult (or retry it and produce a result). The first call already has a slot and settles as cancelled; the second has no slot because `calls.start` occurs only after permit acquisition. Its bounded admitted arguments and intended recipient vanish from the execution evidence. `ToolCalls::settled` cannot recover a request never entered into the log.

The current retry/cancellation proof exercises a call already executing, so it cannot detect the missing waiter. This affects normal supported concurrent orchestration under a deliberately small cap; it is not an exotic malformed-input case.

Smallest correction: start the existing bounded record immediately after declaration/argument checks and before waiting on the non-code semaphore. Let the current unfinished-record settlement represent cancellation while waiting. Preserve request counting, permit ownership, no dispatch after cancellation, and target/argument rejection without a leaf record. This is an ordering correction in the shared owner, not new machinery.

Focused proof: extend the existing Vala cap/cancellation fixture with cap 1, one deterministically held operation and a second validated waiter, then terminate the attempt into a produced result. Assert both admitted leaves occur once in admission order; the waiting leaf has its bounded arguments/recipient, no digest, zero bytes and cancellation error, and its underlying capability was never invoked. A retry must not lose either original slot.

## Verification limits

No tests were executed by this reviewer, as assigned. The orchestrator independently ran the focused Vala records/retry/bridge tests, startup/conversion tests, historical wire round-trip and three Rust public journeys; all passed, as recorded in `verification.md`. This reviewer inspected those test sources. None asserts a record after recipient-conversion rejection or cancellation while waiting on the shared semaphore, so those GREEN results do not close DATA-001/002. Runtime capture-disabled non-disclosure has no dedicated proof in the supplied evidence. Inspection found the existing sensitive-output container and authorization boundary retained, and no new argument-bearing alternate surface; no new disclosure defect is asserted. Generic ContextCapture governs assertion actual values and is not evidence of a separate outcome-capture toggle.

Authored Python/TypeScript/MCP journeys belong to TASK-002. Concurrency/lifecycle/security completeness outside record production and consumption belongs to the separately selected specialists. No migration or persistence owner changes are needed for the two corrections above.
