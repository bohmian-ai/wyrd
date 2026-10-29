# TASK-001 behavior review — FAIL

## Subject and scope

Repository `/home/thorrester/Documents/GitHub/wyrd`; immutable first-review
range `911a8e278d829ab2722fc20ae881b67481dcf9f3...c82414d38561cc3f7e17ac3f3ff52223378abde8`.
Approved authority is SPEC-agent-verdict-portability revision 2 at
`changes/active/agent-verdict-portability/spec.md`; original task is
`tasks/TASK-001-portable-verdict-and-repair.md` in that packet. The earlier
intake-only authority attempt supplies no implementation acceptance evidence.
HEAD remained the candidate during inspection. No implementation was changed.

I reviewed the cumulative diff, the original requirements and task scenarios,
root and affected nested AGENTS, workflow, maintainer/errors/Rust/agent-surface
guides, testing map and owning Prompt/Agent/Verifier architecture. CodeGraph
was consulted first, then source and actual diffs. Starting scope/map is
`scope.md`; I expanded it through Prompt render/bind, callback replacement,
response adapters, native provider clients, request builder and runtime dispatch.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 portable envelope, canonical values and bounds | `execution.rs:583-648,709-763`: projection precedes data embedding; five closed disjoint alternatives retain wire types | Runtime structural target and invalid-value corpus in `plain_verdict_schema_is_portable`; root independently reran schema tests | PASS |
| REQ-002 / AC-002 author data, local refs and private resource | `response_format` preserves authored object and adds existing resource `$id` after projection | `expanded_verdict_keeps_authored_data_schema`; original local-ref tests; root's owner tests | PASS |
| REQ-003 / AC-002 named OpenAI non-strict; inline/native policy | `authoring.rs`: named detection passed to portable lowering and `named_output` sets false on native Chat/Responses | named-format regression covers plain/data/native and preserved authored form; root's owner tests | PASS |
| REQ-004 / AC-003 exact effective schema | `run_loop` compiles from Prompt `response_type` before rendering/replacement differences are considered | Existing four-encoding recovery tests use identical template/sent schemas and do not cover the mismatch | FAIL: BEH-001 |
| REQ-004 / AC-003 correction, union diagnostics, retained reply and bounds | `structured.rs:69-191`; `loop_runtime.rs:473-517` retains assistant and appends correction in same accumulator/session | Four encodings, enum/union/multiple failures, bounded feedback and omission tests; root reran agent tests | PASS, subject to BEH-001 |
| REQ-005 / AC-003 budget, route, hooks, iteration/model journal | Same existing `for iteration` and dispatch path; zero cap exits before validator; last invalid ends structured-invalid, tool-only ends max-iterations | cap 0/1, mixed tool/repair, journal counts; existing timeout/callback/session tests passed in root's 295-test execution | PASS |
| Refusal non-goal / REQ-005 existing classification | Native 200 refusal response is not distinguished before schema check | No refusal regression in structured-output corpus; source traces a refusal to repair | FAIL: BEH-002 |
| REQ-006 / AC-004 / INV-002 reachable bare slots and isolation | Vala CheckExecutor collects `sources` from inputs and workspace before invoke; bind edits cloned native Prompt and rederives schema; empty sets unchanged | `check_agent_narrows_citation_slots_per_call`, restriction unit test; concurrent sets/no-source/card comparison | PASS |
| REQ-007 / AC-005 typed/data/evidence guards; no code repair | Action `conforms` removed; unchanged `verdict_from` retained for check/code consumers; final citation validation remains | Vala custom-data/negative tests, recorded 23 hosted guard tests; root owner tests | PASS |
| INV-001 / AC-005 safe terminal diagnostics | `Rejection.violations` contains fixed parser/non-object strings or schema paths; terminal error and Vala mapping use those, never correction text | exhaustion marker/key/error/journal assertions, Vala nested-key redaction corpus | PASS |
| AC-006 all four real-server surfaces | Existing Rust/Python/TS/MCP owners use shared two-iteration fixture, scripted malformed/repaired cases and public execute path | Recorded focused real Postgres journeys; source asserts recovery and inconclusive stable code with safe reason | PASS |
| AC-007 live generated schema and validated reply | All four smoke modes derive schema via runtime Prompt lowering, but response assertion omits most schema semantics | Recorded provider successes; direct assertion self-check accepts invalid schema values | FAIL: BEH-003 |
| Non-goals: no new public setting, second loop, author rewrite, external fetch, code repair, deployment/storage changes | Actual diff adds private validator and uses existing loop/request/session owners; installed jsonschema has default features disabled | Source/diff inspection; no new runtime feature or persistence path | PASS |

## Existing-owner and maintainability assessment

The new validator uses the already-installed `jsonschema` dependency and retains
one compiled validator per run, with feedback beside that state. The alternative
was Vala's action-only `conforms` plus Skald JSON-object parsing: neither could
repair within the conversation, and `conforms` duplicated the changed owner.
Deleting that duplicate and extending the existing loop is justified. No second
orchestration layer, provider compatibility analyzer, evidence traversal or
generic workflow framework was introduced. `portable` operates solely on Wyrd's
generated nodes; replacing canonical Rust types or rewriting author data would
be incorrect. Narrowing reuses `CallInput.sources` and the per-call Prompt copy,
so shared Card state remains outside the invocation. No independent DRIFT
proposal is justified by line count or implementation preference.

## Proposed findings

### BEH-001 — INCORRECT: validator uses a different schema from the dispatched request

- **Obligation:** REQ-004 requires the exact effective schema sent in the
  invocation, and REQ-005 preserves hooks. This applies to structured Agent
  runs generally, not only Vala's fixed Verdict fixtures.
- **Location:** `crates/skald/skald-agent/src/loop_runtime.rs:375-383,402-433,496`;
  `RunLoopInputs.response_type` is sourced at lines 149-152 and 249-254.
- **Producer → consumer:** `skald_spec::Prompt::render` (`prompt.rs:607-632`)
  clones and `bind_mut`s all JSON string leaves, including response schema
  constants/enums. `run_prompt` sends that rendered request as `template` but
  sends the original Prompt's derived `response_type` separately. `run_loop`
  compiles the original schema. Independently, `before_model` is a supported
  `CallbackOutcome<ProviderRequest>` replacement (callbacks.rs:59-62); its
  replacement is dispatched after compilation without rederiving validation.
- **Concrete supported scenario:** an object schema has property `answer`
  with `enum: ["${expected}"]`; rendering `expected=ready` sends
  `enum: ["ready"]`. A provider answering `{"answer":"ready"}` satisfies the
  sent schema but is rejected against the original placeholder. Every repair
  continues asking for the rendered enum while correction describes the
  unrendered enum; the run can exhaust incorrectly. A before-model hook
  tailoring schema similarly produces the mismatch.
- **Sibling paths:** `run_with` and `run_prompt` both feed this shared owner;
  all four wire encodings and direct/native versus gateway routes inherit it.
  Vala's `edit_response_schema_mut` correctly rederives the pre-render copy but
  cannot repair the shared dispatch-time mismatch downstream.
- **Materiality:** this violates the task's principal exact-schema invariant
  and rejects valid provider output in an ordinary supported binding path.
- **Smallest correction:** derive validation from the final outgoing native
  request in the existing run loop, using the existing request-to-response-type
  owner rather than a second provider-specific schema extractor. Preserve
  same-schema validator reuse across repairs; refresh only when the effective
  schema changes. Do not forbid existing hooks or alter author schema meaning.
  This is a local owner correction, not a new public retry setting or API.
- **Focused proof:** a bound enum accepts its rendered value and rejects the
  placeholder; a schema-changing before-model replacement is validated against
  the actual recorded request. Existing repair, callback, cap and route proofs
  must continue passing.

### BEH-002 — VIOLATION: provider refusal is treated as invalid output eligible for repair

- **Obligation:** spec Invariants and boundaries explicitly says provider
  refusal must not trigger repair when no usable answer was returned.
- **Location:** `loop_runtime.rs:473-517`, especially default-empty text at
  491-495 and unconditional structured checking at 496.
- **Producer → consumer:** native Anthropic `send_native` returns a decoded
  successful `AnthropicMessagesResponse` unchanged; runtime `dispatch` returns
  it unchanged. A response with `stop_reason: Refusal` and no text reaches
  `assistant_message`, which accepts its role/content. Adapter text is `None`,
  converted to empty string; no tool calls means `StructuredOutput::check("")`
  produces a correction and `continue`. The next provider call therefore asks
  a refusal to become a schema-valid answer.
- **Existing mechanism / sibling paths:** the response adapter already maps
  Anthropic `Refusal` and Gemini safety finishes to `FinishReason::ContentFilter`
  (`adapter.rs:336-361`); OpenAI native refusal content also lacks usable output
  text. Both Agent entry points use this loop. Provider HTTP errors already
  return before this path and must remain unchanged.
- **Observable consequence:** extra billable calls and latency on a refusal,
  potentially ending as structured-invalid after consuming the full budget,
  contrary to the approved refusal boundary.
- **Smallest correction and burden:** before initiating structured repair,
  use the existing finish/typed response information to keep provider refusals
  without usable answers on a terminal non-repair path. Reuse adapter state;
  do not introduce a new retry layer or suppress real malformed-answer repair.
  No change to permission, tool refusal, or shared service availability is
  warranted. The validator should verify the terminal projection against the
  approved unchanged-classification constraint before prescribing a new code.
- **Focused proof:** a scripted native refusal with empty text makes exactly
  one provider call even with budget remaining, appends no correction and
  remains inconclusive in Vala. HTTP refusals retain their existing behavior.

### BEH-003 — MISSING: live proof never validates responses against its runtime schema

- **Obligation:** AC-007 and task Scenario 6 require schema-valid Verdict
  replies and validation of every returned reply, not only request acceptance.
- **Location:** `sdks/wyrd-sdk-python/tests/smoke/test_gateway_live_providers.py:103-113`,
  callers at 425 and 478.
- **Evidence:** `assert_verdict` checks top-level names/outcome/reason and each
  locator's single key, but no array type or locator value/closed nested shape.
  A direct self-check invoking the unchanged function extracted via Python AST
  accepted both `citations: {}` and
  `citations: [{"slot":"answer","locator":{"page":"not-an-integer"}}]`.
  Both violate the runtime-generated schema. Command used `mise exec -- python3`
  with `ast.parse`, compiled only this function, and supplied `json` in globals;
  no provider calls or source edits were involved. An initial `python` command
  could not run because that executable was absent; `python3` succeeded.
- **Materiality:** this is the required compatibility proof, including
  non-strict OpenAI where the provider need not enforce the schema. It can
  report compatibility success on an invalid Verdict. Deterministic Rust
  schema tests do not validate those actual live replies.
- **Smallest correction and burden:** pass the exact runtime schema already
  obtained by each smoke case into its reply assertion and validate the parsed
  object with the installed JSON Schema validator (`jsonschema` is already in
  the Python lockfile). Keep request-mode assertions; avoid maintaining a second
  handwritten schema checker. No dependency or new harness is needed.
- **Focused proof:** the two invalid examples fail the assertion, a valid
  Verdict passes, and the existing four focused live cases validate their
  replies against the sent schema with model/mode evidence retained.

## Verification and limits

Root reported an independent rerun of all 295 Skald-spec/Agent/Vala tests and
eight schema-module tests, all green; its commands/results are in
`verification.md`. I inspected those tests as proof, but did not run Cargo in
parallel. Recorded journey and live results remain implementation evidence;
I did not rerun real providers or full journeys. The direct Python assertion
self-check above was independently executed. No speculative hardening or
optional refactor is proposed. The three findings still need independent
validation; this discovery report is not the final ledger.

**Overall: FAIL.** Skipped full journey/provider reruns; risks are the effective
schema mismatch, unintended refusal repair and incomplete live-reply proof above.
