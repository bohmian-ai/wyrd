# System resilience review — TASK-001

Result: **FAIL**.

## Immutable subject and coverage

Repository `/home/thorrester/Documents/GitHub/wyrd`; base
`911a8e278d829ab2722fc20ae881b67481dcf9f3`; candidate
`c82414d38561cc3f7e17ac3f3ff52223378abde8`. HEAD remained that candidate.
This is the first implementation audit; no previous implementation acceptance
or finding closure is carried forward. Scope follows `scope.md`: repair loop,
effective request, iteration/time limits, hooks, journal/session, deployment
callers and Vala failure propagation. Actual cumulative Git diff was read.

Authorities read: approved spec revision 2, original TASK-001 scenarios,
root AGENTS, spec-driven development, maintainer guide, errors guide, and the
Prompt/Agent and Agent-tools sections of `architecture/wyrd-design.md`.
No nested AGENTS governs the runtime files in this assigned boundary.
CodeGraph was consulted before source navigation.

## Deployed paths and existing owners

Vala `AgentExecutor::invoke` resolves a fresh Prompt, applies call-specific
bindings and slots, opens a per-invocation ToolSession, creates an Agent using
the declared/effective route and run config, and calls `run_prompt`. The
server-hosted executor requires the Wyrd gateway; native Agent callers use the
same `run_loop` through `run_prompt` or `run_with`. Every repair continues the
existing bounded loop; there is no second runner or durable recovery state.
The new StructuredOutput retains an invocation's validator and schema, extending
the existing final-answer parse owner. Reusing jsonschema is appropriate;
Vala's removed action validator does not provide conversation repair and
cannot own request-specific validation in the shared Agent runtime.

Provider calls retain `dispatch`, request reconstruction, gateway route,
before/after-model hooks, model journal events and chat spans. Invalid answers
are appended as assistant turns before rejection; corrections are user turns
and session turns. Safe schema locations enter the terminal AgentError journal,
public error and Vala reason. Detailed corrections do not enter that error path.
The ordinary sensitive conversation path remains their owner.

## Failure-path assessment

| Scenario / obligation | Source path and observable effect | Assessment |
|---|---|---|
| Invalid answer followed by valid answer; REQ-004/005 | `loop_runtime.rs:473–518`: retain assistant, append correction, continue ordinary iteration. `structured_output_repairs_every_encoding` checks four encodings and journal call counts. | PASS for unchanged effective schema |
| Persistent invalidity, zero/one iterations and tool-only exhaustion | `run_loop:369–385,496–517` and loop tail preserve cap behavior. Structured-invalid is terminal in `gateway.rs:105–118` and Vala `block_error:561–586`. Tests cover zero/one cap and shared tool budget. | PASS |
| Outer retries and stored requirement reasons | `AgentExecutor::invoke:399–415` maps failures through `block_error`; actions retry only `BlockError::Retryable` in `engine.rs:686`; checks become inconclusive through `judge:770–786`. New failure is terminal and carries public stable code. | PASS |
| Provider connection/status/decode failure | `dispatch.rs:37–58` returns provider errors before any structured answer handling; `run_loop:452` propagates before repair. Existing gateway classifier is retained for these errors. A refused HTTP request cannot enter output repair. | PASS |
| Run timeout / caller cancellation during another repair | `run_prompt:260–266` and `run_with:157–163` wrap the entire loop rather than each iteration. Repairs introduce no spawned task or detached work; dropping the awaited future cancels the same call tree. `AgentExecutor:401–410` keeps ToolSession refusal race and drops session before returning. Existing timeout test covers the shared wrapper, not specifically a repair call. | PASS by unchanged owner/wrapper; focused repair-timeout execution not reproduced |
| Session or nonterminal journal append failure | Every new correction-session append uses `?`; iteration and model journal failures still stop the run. Terminal journal append remains best effort after result. No new persistence/replay owner appears. | PASS |
| Shared server availability / restart | Failure returns an invocation Result and then a block outcome. No process-exit, global health, durable state or deployment topology changes; unrelated runs remain owned by their existing executors. A process interruption loses the invocation's in-memory repair state as before. | PASS |
| Exact schema after render or request hooks | Validator comes from the unrendered Prompt, while rendering and before_model may change the sent schema. | FAIL — SYS-001 |
| Provider refusal without usable output | A refusal-only Chat answer is converted to empty text, then corrected and called again. | FAIL — SYS-002 |

## Proposed findings

### SYS-001 — INCORRECT: validator does not necessarily validate the schema sent

**Obligation.** REQ-004 requires each proposed final answer to satisfy the exact
effective schema sent in its invocation; REQ-005 retains hooks. Scenario 3
requires extending the existing render/conversation owner, and author schema
meaning must remain intact.

**Location.** `crates/skald/skald-agent/src/loop_runtime.rs:254,378–382,402–434`.

**Producer-to-consumer evidence.** `skald-spec::Prompt::render:607–633` clones
the Prompt and calls `bind_mut`; `bind_mut:519–523` serializes the entire request
and replaces string leaves, including schema strings, without rederiving the
original Prompt's cached response_type. `run_prompt` passes that rendered
request as template but passes `prompt.native().response_type` separately at
line 254. `run_loop` compiles this pre-render schema once at lines 378–382.
It then permits a `before_model` hook to replace the native request at
402–431 and dispatches that replacement at 434. Neither render nor hook edits
reach the validator. `run_with` uses the same loop; its before_model replacement
has the same divergence. The hook contract expressly allows ReplaceWith of
ProviderRequest (`callbacks.rs:14–21,74–76`), and
`agent_run_before_model_replace_swaps_native_request` proves that behavior.

**Reachable failure.** A normal structured Prompt with
`properties.answer.enum: ["${expected}"]`, rendered with `expected=ok`, sends
`enum:["ok"]`. A provider returns `{"answer":"ok"}` satisfying that request.
The new validator rejects it against literal `${expected}`, sends a misleading
correction, spends the available iterations and returns structured-invalid.
Conversely a scripted/native provider returning the literal can be accepted
against the stale schema despite violating the schema sent. Replacing an enum
through the established request hook creates the same defect without template
variables. Both public Agent entry points and Vala's input-binding path are
consumers of this shared producer; this is one root cause.

**Materiality and correction burden.** The approved exact-schema promise fails
for supported schema and request authoring, producing inconclusive verification
and unnecessary paid calls for valid answers. It is not a request for extra
schema features. Deleting validation or forbidding existing substitutions/hooks
would break approved behavior. The smallest correction belongs in the existing
Agent request-to-validation boundary: use the schema of the effective rendered
request after applicable request hooks, and reuse the compiled validator while
that schema remains unchanged. Existing Prompt schema derivation is the owner
to reuse; do not add a second schema interpretation or a Vala-only guard.
Private cache/control-flow shape remains implementer-owned.

**Focused closure proof.** In the existing structured-output integration owner,
exercise a schema enum bound by `run_prompt` variables and a schema replaced by
before_model. Assert recorded provider schema, acceptance of its valid answer,
rejection/repair of a stale-schema-only answer, correct call counts and no Card
mutation. Cover both entry points where hooks share the fix. Existing
four-encoding repair and zero/tool budget tests remain unchanged.

### SYS-002 — VIOLATION: an explicit provider refusal enters repair

**Obligation.** Spec Invariants and boundaries explicitly says: “Provider refusal
must not trigger model-output repair when no usable answer was returned.” It
also preserves existing refusal classification. This is an approved exclusion,
not speculative refusal hardening.

**Location.** `crates/skald/skald-agent/src/loop_runtime.rs:489–516`.

**Producer-to-consumer evidence.** Native OpenAI client `send_native:455–466`
returns a decoded successful Chat response without classifying its message's
`refusal`. `OpenAiChatMessage` has the explicit `refusal: Option<String>` wire
field (`wire/openai_chat.rs:400–414`). `dispatch:47–58` returns that response.
`assistant_message` clones its message unchanged (`request_builder.rs:148–157`).
For a refusal with `content:null`, no tools and a nonempty refusal field,
`adapter.rs:220–225` returns no text; `run_loop:491–495` turns that into an
empty string. StructuredOutput::check rejects the empty string as malformed
JSON. Unless it is the last iteration, lines 505–516 append a repair correction
and dispatch again.

**Baseline and candidate semantics.** At base `911a8e278`, the same refusal-only
answer reached `parse_structured_output` directly and returned
StructuredOutputDecode immediately, before another model call. Its Wyrd
projection was the old 422 decode code. Vala's baseline `block_error` classified
every such non-provider/non-timeout failure terminal, so a hosted check became
inconclusive after that one call. The approved candidate intentionally replaces
that old public code with structured-invalid; restoring the removed code or
inventing a new public refusal code is not required. The regression is the new
model-output repair after an explicit refusal, which the spec excludes.
HTTP status errors already exit before validation and are unaffected. This
finding concerns the existing typed successful-response refusal representation.

**Consequence and materiality.** An ordinary provider refusal with a configured
repair budget now prompts additional paid calls instructing the model to answer
again. A later scripted valid response can produce a judgment where the
required refusal boundary should already have ended that invocation. This is
an explicit negative-path obligation and a supported native Chat wire shape.

**Smallest correction boundary.** Exclude an explicit refusal with no usable
answer at the shared Agent response boundary before appending a repair turn.
Preserve the existing immediate terminal/inconclusive behavior using the
approved replacement structured-output error projection and fixed safe
diagnostics; do not restore the removed decode code, introduce a new public
error, stringify refusal content into an error, or change transport retry
classification. Do not treat every empty/malformed answer as a refusal:
ordinary malformed-output repair remains required. Reuse native response
fields/adapters rather than an external refusal classifier.

**Focused closure proof.** Script a typed Chat refusal-only response followed by
a valid structured answer with budget greater than one. Assert one provider
call, no correction turn, safe terminal outcome and the approved public error
projection. Retain a distinct malformed-answer-then-valid case proving ordinary
repair still occurs. Use the existing integration provider/journal fixtures.

## Verification and limits

No commands that compile or test were run by this reviewer; the orchestrator
reported its independent rerun of 295 Skald/Vala tests and eight schema tests in
`verification.md`. Source inspection confirms the existing tests cover ordinary
repair, budgets, encoded conversations, safe errors and existing timeout owner;
neither failing scenario above is represented by the changed tests. Live
provider calls and full journey suites were not rerun here. No implementation,
fixture, generated output, dependency or deployed state was changed.
