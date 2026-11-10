# TASK-003 R1 repository standards review

Result: **PASS** within the remediation scope. No material repository-rule finding remains in that scope.

Immutable subject: prior `384a0def73ed6eac4041dc1e428dc30e981514fe`, candidate `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`, original base `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`. I inspected the 13-file prior-to-candidate diff, affected helper bodies/callers and prior FIND-003-3/4 closure. HEAD matched the candidate; all ten entries in `authority-sha256.txt` verified immediately before writing this report. The remediation diff passes `git diff --check`.

This is a fresh repository-standards review, not task acceptance or a repeat of the original 93-file discovery. No new-round reviewer report was read. No source, prior report, authority, commit or build output was changed; no Cargo command or test was run.

## Authority and carried coverage

Read root AGENTS, the two applicable nested SDK AGENTS (there are no nested rules in the changed Rust owner trees), `agent_docs/index.md`, `rust.md`, `errors.md`, `maintainer-style.md`, `TESTING.md`, `changes/spec-driven-development.md`, the review skill, subject, original task, approved-amendment status in the spec, prior standards report, prior validated FIND-003-3/4 and remediation instructions/evidence. CodeGraph preceded source navigation. Architecture navigation used `architecture/README.md` and the owning Verifier section of `wyrd-design.md`.

Carry forward the source-backed unaffected rows of `../task-003-384a0def7/standards-review.md`, as reconciled by that round's `findings-validation.md`: pure wire contracts; shared-client errors; actual TenantConn/OperatorPool signatures; lease/permission/audit boundaries; SQL migration; checkpoint/File custody; generated declarations; docs; fixtures; and original language-binding parity. This diff changes neither those owners nor their contracts. The Scribe change only restores comment attachment. The engine change moves existing admission before existing spend/begin and reuses existing Busy draining; it introduces no SQL capability, durable format, public error, clock or permission owner. No new evidence requires reopening those rows. The prior historical TDD exceptions remain exceptions; this report does not relabel them as RED evidence.

| Changed surface | Governing authority and inspected standards | Result and source evidence |
| --- | --- | --- |
| `vala-verify/src/blocks/engine.rs` | Root owning-tier/struct/async/portable rules; Rust rustdoc and error rules; Verifier contract | PASS. Existing `VerificationEngine::admit` is reused at requirement `:1507` before spend/begin; `judge` documents caller-held permits and no longer reacquires. `instance_requirements :1455` uses existing `settled`, matching `members`/`drive`. No second lifecycle/admission/checkpoint owner, dependency, error catalog or runtime. |
| Engine tests | Root inline pure tests, deterministic synchronization, meaningful negative/recovery proof; changed-item docs; TESTING exact selectors | PASS. Existing Script gains a documented held-check map and Notify wait in its executor; tests use paused time and assert actual calls, accepted ledger attempts, retained settlement and recovery outcome. All three added tests and the changed executor/field have rustdoc, including their panic/cancellation contracts. No allow/ignore, widened glob or removed assertion is introduced. |
| Validator `controlled`; Scribe retained/slice helpers | Root changed-item documentation; pure spec dependency boundary; Rust panic/error docs | PASS. Documentation-only corrections describe actual object/index preconditions and retained identity vs slice-pop behavior. No validation, error handling or signature changes. |
| Rust SDK journey and helpers | Root real SDK/server/Postgres journeys, public inspection, changed-item docs; TESTING fixture/lane ownership; Verifier retained-publication semantics | PASS. `Race`/variants/field, `reached`, changed journey and startup helpers have intent/precondition docs. Provider holds gate the existing public invocation observations. Existing final verdict, retained inspection/read-grant and publication assertions remain. The unrelated capacity-stimulus HELD delay remains with narrowed documentation. |
| MCP journey and OrchestrationDesk | Root agent-first public contract and real MCP/server journey; Rust helper docs/errors; TESTING pg separation | PASS. New held field is documented; start forwards the existing gateway provider root to the local provider; `reached :4307` documents failures and reads `verification.list_invocations`. Local Race/Case changes are documented. All six original case outcomes remain. No MCP-only production lifecycle/contract is added. |
| `wyrd-testing/src/gateway_capture.rs` | Root existing-owner/reuse, test-helper boundary, portability, Rust owning-state/IO/error/docs; TESTING provider mocks | PASS for material rules. HeldUpstream owns its address, hold state and serving task; Holding retains one reqwest client; forward handles body/forward/read failures. Every new type, field and method/function has descriptive rustdoc. This is loopback local-provider support, using installed axum/reqwest/wiremock, not a new production transport or Wyrd worker/lease/replica control. See limited documentation observation below. |
| Python conftest/support/public journey | Python AGENTS thin public client and top-level integration tests; root lifetime/testing rules; deterministic proof | PASS. Public `wyrd` imports and top-level `test_speculative_disposition_controls_consumer_admission` remain; no native API/stub/export change. Existing threaded local provider uses Event holds, unique per-case prompt fragments and finally releases. The test-only event registry extends existing local mock signaling and owns no server/durable lifecycle or SDK state. |
| TS public journey and receiver/server support | TS AGENTS thin client, precise callback/return types and Node-lifetime tests; root deterministic proof; TESTING local-provider reuse | PASS. Receiver.start and ServerOptions both accept `Reply | Promise<Reply>`; existing synchronous callers stay assignable. The promise is resolved before the existing response/recording path. Journey holds release in finally; provider teardown retains Receiver.close's closeAllConnections/close. No napi, generated declarations, production exports or transport changes are needed. |
| All changed tests/evidence | Root no weakening/skip; spec-driven source/proof authority; current user correction of decision-first ADMIT instruction | PASS for this round's standards. Sleeps that chose speculative orders are removed. Public accepted-state milestones gate opposing replies; 50ms polls observe acceptance rather than manufacture ordering. No engine-semantics change is inferred from proof wording; existing drive only records admission after step settlement (`engine.rs :902-912`), while exclusion may cancel and record during running work. |

## Prior FIND-003-3 closure: closed

All ten listed locations were checked against their bodies, including the Scribe pair counted as one prior location:

| Prior item | Candidate evidence |
| --- | --- |
| `VerificationEngine::members` | `engine.rs :1229` Panics describes `spec.steps[index]`. |
| `VerificationEngine::member` | `:1290` Panics describes its declared-step index. |
| `VerificationEngine::instance_requirements` | `:1432` Panics describes its declared-step index; docs also describe Busy draining now implemented. |
| `ready` | `:1946` Panics accurately applies only to Node::Step. |
| `missing_requirements` | `:2009` Panics describes its declared-step index. |
| `Node::id` | `:2159` Panics covers both step/disposition and whole-requirement indexing. |
| Validator `controlled` | `validate.rs :2651` Panics describes `as_object().expect("step fields")`. |
| `contradicts_retained` / `next_prepared_slice` | `shards.rs :4472-4510`: predicate keeps retained-conflict/Errors contract; slice-pop documentation is attached to slice-pop again. |
| Rust `section_turns` | `verifier_orchestration.rs :2210` Panics covers upstream recording through Desk::requests. |
| Rust `mapped` | `:2230` Panics covers execution/page reads through Desk::publications. |

The doc correction itself adds no guards or behavior. Newly changed Rust items were included above, rather than restricting the gate to this prior list. No missing rustdoc blocker was found.

## Prior FIND-003-4 closure: closed for standards

Rust now supplies direct public ordering proof, not only final outcomes:

* Action-first (`verifier_orchestration.rs :1236-1253`) holds comparison until extra is publicly Action(Succeeded) with Speculation::None.
* Decision-first (`:1255-1289`) observes the extra provider request while its answer is held, accepts the comparison, and asserts extra is still Started. ADMIT observes None until settlement, then the unchanged final manifest/retained assertions prove admitted. DISCARD waits for recorded Discarded while extra is still Started before releasing its answer.
* Known false (`:1291-1304`) holds bootstrap until comparison is accepted and asserts no extra request before release; unchanged called-false/final assertions cover the completed run.

MCP `:4919-4970`, Python `:270-305`, and TS `:602-636` preserve the six original cases and final assertions while waiting on equivalent public/provider milestones. Mirrors do not add another timing matrix. The current user's explicit corrected ADMIT instruction matches unchanged drive semantics; requiring an admitted record before settlement would demand different behavior and is not a valid closure condition.

Wiremock already owns scripted responses and request recording; PacedUpstream already owns timed streaming but has no test-event hold capability. HeldUpstream leaves scripting/recording in wiremock, adding only asynchronous forwarding/holds shared by Rust and MCP. Python extends its existing threaded receiver; TS extends its existing Receiver callback. Those boundaries are sufficient local provider support. No source evidence establishes a need for new Wyrd server lifecycle controls.

## Nonblocking source/documentation observations

Two narrow comments overstate behavior: Rust `Race::DecisionFirst :1058-1059` still says the disposition is recorded before release, although the detailed journey docs/assertions correctly distinguish ADMIT settlement; HeldUpstream's type doc at `gateway_capture.rs :578` says a held connection closes on drop. Its Drop only aborts axum::serve. Installed axum's serve implementation spawns connection tasks independently, so aborting that accept loop alone does not establish immediate closure of every active held request. These are not missing documentation or a material failing supported path under the current callers: successful flows release holds, Python/TS finally release them, and Rust/MCP helper lifetimes end with their per-test Tokio runtimes. I do not prescribe a new connection-task controller, teardown harness, or style-only remediation round. The final evidence should not claim an independently proven close-on-drop guarantee.

## Verification and limits

The remediation packet records meaningful RED/GREEN for the two lifecycle corrections, characterization of interrupted Agent spend, all four exact changed speculative journey selectors passing, and Rust/Python/TS owner checks. I checked assertions and selectors against source, not those historical command executions. The new round's `verification.md` additionally records fresh orchestrator execution: seven exact engine regressions passed and the HeldUpstream-backed Rust speculative journey passed (one selected, 14.487s), with both migration-readiness checks passing. Fresh execution remains orchestrator-owned; no extra test is needed to decide this standards scope beyond those focused selectors/checks. No schema, declaration, SQL, platform or dependency changes justify repeating unrelated gates here.

No build, test, broad suite, generated-byte comparison, platform qualification or production load was run by this reviewer. Historical TDD exceptions remain disclosed, and immediate HeldUpstream close-on-drop is not certified. No material finding or new FIND ID is proposed.
