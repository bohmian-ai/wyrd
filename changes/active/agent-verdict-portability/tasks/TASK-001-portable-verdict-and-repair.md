---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-agent-verdict-portability
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007]
depends_on: []
parent_task:
remediates: []
---

# Make Agent Verdicts portable and repair invalid structured answers

## Outcome and value

An Agent judge can reach all three tested provider families with Wyrd's plain
Verdict schema and correct an invalid answer within its existing run budget.
For example, `outcome: FAIL` receives a correction and may become a valid
`failed`; a persistently invalid answer becomes inconclusive with
`WYRD_AGENT_422_STRUCTURED_INVALID` and no reply content in the error.

This is one end-to-end task at the user's request. It owns the schema,
compiler, Agent loop, Vala consumers, public error projections, documentation
and necessary test closure. The maintainer approved spec revision 2 on
2026-10-28. The user originally requested both artifacts together, overriding
the ordinary spec-first task-creation sequence.

## What changes and what must stay true

Implement [the spec](../spec.md) without a second retry loop or a new setting.
Preserve author data schemas, semantic Verdict checks, bare original-evidence
slots, Card identities, per-invocation isolation and sensitive capture policy.
Repair uses up to the existing iteration/time budget and therefore adds bounded
latency and token cost. No promise covers arbitrary author-schema acceptance
by a provider. Schema-valid but semantically invalid citations remain Vala's
responsibility and need not receive repair.

## Approach

1. Revalidate the existing owners and the uncommitted named-format change;
   preserve it and close any missing lowering regressions.
2. Produce the portable generated Verdict projection before embedding `data`,
   keeping the canonical wire types and clarifying bare-slot documentation.
3. Extend Skald's existing structured-output path and conversation loop with
   exact-schema validation, bounded feedback and the approved failure contract.
4. Prepare check-specific slot restrictions before invocation; remove duplicate
   action validation and preserve final Verdict/evidence guards.
5. Close public error mappings and focused journeys, update owning architecture
   and documented provider limits, then regenerate and verify consumed artifacts.

## Ordered implementation scenarios

### Scenario 1 — Portable generation and author-schema preservation

**Behavior.** Plain output matches the spec's structural target. Each of the
five locator shapes remains closed and disjoint; outcome is the exact enum.
Expanded formats retain authored keywords, optional fields and local refs.
Negative/overflowing integers remain invalid at final decoding.

**RED.** Add focused owner tests comparing runtime output with the target and
exercising valid/invalid values. Confirm failures come from the existing
`oneOf`, reference siblings and numeric annotations. Run the owner filters in
Verification; record each added test's exact selector before RED.

**GREEN.** Change generated-envelope projection only and clarify SlotName.
Prove final typed validation rejects the broadened numeric cases.

**REFACTOR.** Reuse the existing generator/resolver and schema validator; do
not replace canonical types or remove their rustdoc to influence schemars.

### Scenario 2 — Named OpenAI formats preserve authoring policy

**Behavior.** Portable Chat and native Chat/Responses named formats use
non-strict mode with and without `data`; inline/native authored settings retain
their specified behavior and authored serialization is unchanged.

**RED.** The supplied working-tree fix already passes the existing named-format
regression: this portion is verification-only, not a manufactured RED. Add
missing policy cases and run their exact selectors; any missing behavior must
fail for its request strictness or serialization assertion.

**GREEN.** Keep the supplied change and complete only missing lowering paths.

**REFACTOR.** Retain one policy for named formats rather than splitting by data.

### Scenario 3 — Agent repairs within its budget

**Behavior.** Malformed JSON, non-objects and schema-invalid objects receive
actionable bounded correction in the same run. A corrected answer succeeds;
persistent invalidity ends with the approved error at the cap. All supported
conversation encodings work, union diagnostics expose permitted shapes, and
tool turns, deadlines, cancellation and ordinary provider failures retain
their meaning. Verify cap zero, cap one, and mixed tool/repair iterations.

**RED.** Extend the existing structured-output integration module and provider
loop tests with invalid-then-valid and permanently invalid scripts. Assert
actual next requests, feedback limits and call/span counts, not just final
errors. Current code either accepts an invalid object or ends on decoding.

**GREEN.** Extend the existing loop, use the effective schema and budget, add
the crate-local error and derive-backed public projection, and align retry
classification with repair exhaustion.

**REFACTOR.** Use existing conversation, adapter, journal and gateway machinery.
Compile/reuse validators within an invocation rather than rebuild per repair.
Do not introduce a parallel validation/orchestration owner.

### Scenario 4 — Reachable slots are enforced per check invocation

**Behavior.** Requests and local validation use the same reachable bare slot
set, including fixed workspace attachments. Wrong/prefixed slots receive
repair when evidence is reachable. With no reachable source, leave the citation
schema unchanged and rely on final evidence validation. Concurrent calls retain
distinct restrictions without changing stored cards. Final evidence validation
still prevents an uncited pass or an invalid locator from establishing a pass.

**RED.** Add an executor seam test inspecting the outgoing schema before model
execution, a repair sequence for a wrong slot and a concurrent isolation case.
Today reachable slots are computed only after invocation.

**GREEN.** Extend Wyrd's per-call Prompt preparation and existing compiler
boundary to narrow before invocation; preserve input/workspace provenance.

**REFACTOR.** Reuse `CallInput.sources` and existing named-schema compilation;
keep Skald format-name agnostic and avoid alternate evidence traversal.

### Scenario 5 — Terminal results are safe and code checks remain guarded

**Behavior.** Validation errors contain fixed diagnostics/schema locations,
never marker values, arbitrary returned keys or parser excerpts. Action
validation is owned by Skald. Code checks retain schema/typed/data guards and
never call a provider to repair. Agent exhaustion stays terminal/inconclusive
with the approved stable code and no outer retry.

**RED.** Assert redaction across nested unions, invalid JSON and extra keys,
including journal error events and stored requirement reasons. Confirm current
action errors leak validator detail. Pin code-check and typed/data-bound
regressions without weakening their assertions.

**GREEN.** Remove `conforms`, retain `verdict_from`, sanitize error projections
and preserve the new code through the Vala reason and public boundaries.

**REFACTOR.** Share only existing validation/error ownership; detailed repair
content belongs to sensitive conversation capture, not error/log payloads.

### Scenario 6 — Real journeys and generated-schema provider smoke

**Behavior.** Real client/server journeys observe successful repair and
inconclusive exhaustion. Live OpenAI strict, Anthropic and Gemini calls accept
the generated plain schema and return schema-valid Verdicts; named OpenAI
non-strict production lowering works separately.

**RED.** Extend existing verification journeys on Rust, Python, TypeScript and
MCP/HTTP surfaces using scripted invalid answers. Confirm the missing repair
causes the expected failure. Add live generated-Verdict cases to the existing
smoke lane; credentials stay outside deterministic tests.

**GREEN.** Complete consumer closure and require assertions on recovery,
exhaustion code, budgets and stored safe reasons. Live smoke uses runtime
schema generation, never a copied scratchpad payload. Record every provider
skip as missing evidence, not success.

**REFACTOR.** Reuse existing WyrdTestServer/Postgres/provider scripts and live
credential/model configuration; no new server, provider emulator or test lane.

## Acceptance mapping

| Spec proof | Task evidence |
|---|---|
| AC-001 | Scenario 1 structural/acceptance corpus and final numeric decode assertions. |
| AC-002 | Scenarios 1–2 resource semantics, optional data and strictness regressions. |
| AC-003 | Scenario 3 request-level repair, budget and telemetry assertions across native encodings. |
| AC-004 | Scenario 4 provider/local schema equality, source reachability and concurrent isolation. |
| AC-005 | Scenario 5 marker/key redaction, code-check and final typed/evidence guards. |
| AC-006 | Scenarios 5–6 public client → real server/Postgres → client success and failure observations, including MCP/HTTP. |
| AC-007 | Scenario 6 opt-in live evidence naming provider, model, strictness and validated reply. |

## Implementation reference

Inspected owners and precise gaps; revalidate before editing:

| Existing capability | Extension and consumer closure |
|---|---|
| `wyrd-spec/src/verification/execution.rs::Verdict::response_format`, `Citation`, `Locator`; `ids.rs::SlotName` | Normalize generated nodes before existing custom-resource embedding. Both Prompt and code-check schema resolution use this owner. Preserve decode and `check_data_bounds` behavior. |
| `skald-spec/src/authoring.rs::AuthoredPrompt::compile_with`, `compile_resolved`, `named_output`, `openai_chat`; `prompt.rs::Prompt::new`, `render`, derived `response_type` | Keep supplied non-strict changes; ensure per-call schema reaches provider request, derived response type and named-format metadata consistently. Named formats resolve at compile time, so mutating only the response type after compilation is insufficient. |
| `skald-agent/src/loop_runtime.rs` bounded loop and `parse_structured_output` | Existing final-answer path has JSON-object parsing only. Extend it rather than create another runner. Existing `tests/integration/structured_output.rs`, `loop_responses.rs`, `loop_gemini.rs`, session/journal tests own regression seams. Existing decode-failure expectations must change deliberately. |
| `skald-agent/src/error.rs`, `gateway.rs::agent_error_retryable`; `wyrd-spec/src/error.rs` | Add safe invalid-output error and derive-backed `WYRD_AGENT_422_STRUCTURED_INVALID` projection. Existing AgentError has manual crate-local codes; do not copy that pattern into new public catalog code. Close exhaustive matches, SDK/CLI/MCP errors and generated TypeScript code artifacts. |
| `vala-verify/src/blocks/agent.rs::AgentExecutor::{resolve,bind,invoke,execute}`, `conforms`, `block_error` | Prepare check-specific effective schema before invocation. Existing sources from inputs/workspace already supply reachability; currently collection occurs after invoke. Delete action-only `conforms`; preserve toolset session, route, deadlines and provenance. |
| `vala-verify/src/blocks.rs::verdict_from`; `wyrd-server/src/verification/hosted/sandbox.rs` | Keep post-schema typed and data guards for both Agent and hosted code checks; inspect call sites and error mappings before edits. |
| Existing Python/TS `verifier-building-blocks` journeys, `wyrd-ai` journey owner and MCP verification journeys | Add focused recovery/exhaustion surface coverage to existing owners/fixtures. Preserve negative schema, evidence and authorization coverage. No SDK durable behavior. |
| Python `tests/smoke/test_gateway_live_providers.py`, `test_prompt_live_providers.py` | Existing generic-schema live tests do not exercise generated Verdict. Extend the requested gateway smoke file and reuse Prompt rendering/schema resolution and native clients. |

Paths in this table are relative to the owning crate family under `crates/`
unless an SDK path is specified; they guide investigation, not a file allowlist.
Read Python/TypeScript nested AGENTS rules when touching their boundaries.

## Verification and evidence

All commands run via mise. Before RED, record each added/changed test's exact
nextest or pytest/Vitest selector and its confirmed target in this task's
completion evidence. New private test names and fixture structure belong to
implementation; no unverified test selector is asserted here.

Existing exact named-format regression (already passed during research):

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::named_verdict_formats_preserve_authored_forms)'
```

Focused owner lanes for new scenarios and their immediate regressions:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/^verification::execution::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(/^(structured_output|loop_responses|loop_gemini|session_journal|error_codes)::/)'
mise exec -- cargo nextest run --locked -p vala-verify --lib
```

Use the existing `test:bifrost:journey:drift` Postgres lifecycle and hidden inner
task as the source for focused `wyrd-ai` verification journey commands. Its
dependencies build server/Python/Node and its grouped journey runs their public
clients. Select only the added recovery/exhaustion cases for this task; record
their exact commands after confirming actual module names. Derive focused
MCP/HTTP commands from their owning journey target in the same way. Full SDK
and journey aggregates run once at integrated change review.

Live evidence uses `mise run test:gateway:smoke:live`; credentials, explicit
skips and model overrides follow that task's existing contract. Record focused
pytest node IDs for added live cases as well as results for all three providers.
Do not declare the task verified if required provider evidence is missing.

Final checks for the actual write set: `mise run fmt:check`, `mise run lints`,
`mise run check:deps`, `mise run codegen:regen` followed by
`mise run codegen:check`, and `mise run py:format:check`, `mise run py:lints`
for Python changes. Run TypeScript/Python type checks if their projected public
types change. No new generated schema file is committed. Documentation updates
and generated projections use static/diff proof, not manufactured RED tests.

Completion evidence must include per-scenario expected RED and GREEN results,
exact final selectors, public error propagation, runtime schema equality,
redaction assertions, final checks, live provider results and remaining limits.

## Material stop conditions

Stop affected implementation for spec revision if it requires changing public
code/budget semantics, mutating author schema meaning, changing Card identity,
relaxing typed/evidence guards, exposing reply content in errors, or moving
format-name ownership into Skald. Provider rejection of the generated target
requires evidence and renewed design, not silent author-schema rewriting.
Missing live credentials block compatibility evidence, not deterministic work.
Routine private implementation choices remain implementer-owned.

Authority: [spec revision 2 (approved)](../spec.md),
[workflow](../../../spec-driven-development.md),
[implementation discipline](../../../implementation-execution.md),
[testing](../../../../TESTING.md), and the spec's owning architecture links.

## Completion evidence

Status: `IMPLEMENTED`. At implementation time, spec revision 2 was
still `draft`; the maintainer's direct instruction authorized implementation.
The maintainer subsequently approved revision 2 on 2026-10-28. Task acceptance
remains subject to independent review.

Commits: `834c4b609` (Scenario 1), `58c16749d` (Scenario 2), `d778881d2`
(Scenario 3), `474c9f303` (Scenarios 4–5), `020c2eb68` (Scenario 6 journeys),
`a6a14448b` and `0b1bea6ed` (Scenario 6 live smoke), `ee3709fe6` (architecture
and reference docs), `ff5ac7345` (clippy).

### Reuse map, revalidated

| Existing owner | Gap | Extension |
|---|---|---|
| `Verdict::response_format` | The generated envelope used `oneOf`/`const`, `$ref` siblings, `format`, and `minimum` | A private `portable` pass over the generated envelope only, before `data` is embedded |
| `AuthoredPrompt::compile_with` named lowering | Named formats were strict | `strict: false` for every named format (the supplied change, kept) |
| `run_loop` final-answer path | The JSON-object parse ended the run | `structured::StructuredOutput` is compiled once per run. It feeds a correction turn back through the existing `Conversation::append_user` and the session turn, so no runner is added |
| `Prompt` derived `response_type` | A host could not change a schema per call and keep the sent and validated forms equal | `Prompt::edit_response_schema_mut` edits the native request and re-derives the type. It stays format-name agnostic |
| `CallInput.sources` in `CheckExecutor` | Reachable slots were collected after invoke | They are now collected before invoke and passed to `bind`, which applies `Verdict::restrict_slots` to the per-call copy |
| `conforms` in `ActionExecutor` | It duplicated validation and leaked validator text | Deleted; the Agent loop owns action validation |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 portable plain Verdict | `wyrd-spec/src/verification/execution.rs::portable` | `verification::execution::tests::plain_verdict_schema_is_portable` checks exact structure, a keyword sweep, valid and invalid values, and negative or overflowing pages that the schema admits but decoding refuses | PASS |
| AC-002 authored data kept, named formats non-strict | same; `skald-spec/src/authoring.rs` | `expanded_verdict_keeps_authored_data_schema`; `authoring::tests::named_verdict_formats_preserve_authored_forms` (chat and responses `strict: false`) | PASS |
| AC-003 repair within budget across encodings | `skald-agent/src/structured.rs`, `loop_runtime.rs::run_loop` | `structured_output::*` checks repair for chat, responses, Anthropic, and Gemini; the correction's content and bound; safe exhaustion; caps 0 and 1; and the shared tool budget. `session_conversion` checks the code | PASS |
| AC-004 per-invocation slots | `skald-spec/src/prompt.rs::Prompt::edit_response_schema_mut`; `Verdict::restrict_slots`; `vala-verify/src/blocks/agent.rs::{bind,invoke,execute}` | `response_schema_edit::edit_response_schema_reaches_request_and_type`; `restrict_slots_admits_only_reachable_slots`; `blocks_agent::check_agent_narrows_citation_slots_per_call`, which checks that the recorded provider request carries `enum: ["deck"]` on both turns, that a `inputs.deck` cite is repaired, that concurrent calls keep `["deck"]` and `["deck","policy"]` while an unsourced call stays open, and that the Card is unchanged | PASS |
| AC-005 safe errors, guards kept | `block_error` maps `StructuredOutputInvalid` to `WYRD_AGENT_422_STRUCTURED_INVALID: <schema locations>` and is terminal; `conforms` removed; `verdict_from` and the hosted sandbox untouched | `blocks_agent::{step_agent_rejects_schema_violation,check_agent_rejects_non_verdict}` check the code and the absence of `s3cr3t` across nested keys; `check_agent_validates_custom_data` keeps every typed and data-bound refusal at a one-iteration budget; `structured_output_exhaustion_is_safe` covers the error, problem JSON, and journal; the `wyrd-server` `verification::hosted` lib tests pass 23/23 | PASS |
| AC-006 real client/server journeys | `policy-judge` and `judge-agent` fixtures at `max_iterations: 2`; `JUDGE-REPAIRED` scripted on every surface | Rust `verifier_building_blocks::{judge_repairs_an_invalid_verdict,invalid_judge_verdicts_are_inconclusive}`; Python `test_judge_repairs_an_invalid_verdict` and `test_invalid_judge_verdicts_are_inconclusive`; TypeScript `judge repairs an invalid verdict` and `invalid judge verdicts are inconclusive`; MCP `verification::pg_tests::invalid_judge_answers_are_repaired_or_inconclusive`. Each checks two judge requests, the correction on the second, and on exhaustion the code without reply text | PASS |
| AC-007 live providers | `tests/smoke/test_gateway_live_providers.py`: the schema comes from `Prompt.load` of a `verdict` Prompt, compiled at runtime | `test_live_providers_accept_the_generated_verdict_schema[openai]` (strict) passed on `gpt-6-luna`; `[gemini]` (`response_json_schema`) passed on `gemini-3.8-flash`; `test_openai_chat_accepts_the_named_verdict_format` (non-strict) passed on `gpt-6-luna`; `[anthropic]` (`output_config`) passed on `claude-haiku-5-5` | PASS |

### RED and GREEN per scenario

| Scenario | RED | GREEN |
|---|---|---|
| 1 | Before `portable`, the structure test failed on `oneOf`, `const`, and `$ref` siblings | passes |
| 2 | The named-format strictness assertions failed while named formats were strict | passes |
| 3 | Not observed separately. The old decode test showed the run ending on the first malformed answer, which is the missing behavior | `structured_output::*` passes |
| 4 | The narrowing seam test expects `enum: ["deck"]` in the outgoing request; before `bind` narrowed, the schema had no `enum` (the first run's `Null` came from the test's body path, which was fixed) | passes |
| 5 | `check_agent_validates_custom_data` failed once repair consumed the next scripted answer, which confirmed repair reached Vala; the earlier action path returned the validator's text through `conforms` | passes with one-iteration budgets and code assertions |
| 6 | Not observed: the journeys were written after repair existed. With the old one-iteration fixtures, `JUDGE-REPAIRED` exhausts and is inconclusive | all focused journeys pass |

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib                          # 759 passed
mise exec -- cargo nextest run --locked -p wyrd-skald-agent -p wyrd-skald-spec -p vala-verify   # 295 passed
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(/verification::hosted/)"'   # 23 passed
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(invalid_judge_answers_are_repaired_or_inconclusive)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(/verifier_building_blocks::(judge_repairs_an_invalid_verdict|invalid_judge_verdicts_are_inconclusive)/)"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_verifier_building_blocks.py -k "judge_repairs or invalid_judge_verdicts"'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "judge"'   # 7 passed
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -rs -m smoke tests/smoke/test_gateway_live_providers.py -k verdict'   # 4 passed; provider keys from pass via a local mise.local.toml task
mise run fmt:check && mise run lints && mise run check:deps && mise run codegen:check   # pass
mise run py:format:check && mise run py:lints && mise run py:typecheck && mise run ts:format:check && mise run ts:lints   # pass
mise run docs:check   # pass, including example journeys and a11y
git diff --check   # clean
```

### Non-goals and write set

No new Card kind, lane, server, provider emulator, or schema file. Card
identity and author `data` schemas are unchanged. The hosted code-check guards
(`wyrd-server/src/verification/hosted/sandbox.rs`) and `verdict_from` are
untouched. Generated projections (`error-codes.ts`, `card-types.ts`,
`api/errors.md`, `llms-full.txt`) were regenerated, not edited.

### Remaining limits and reviewer flags

- **Public code removed.** `WYRD_AGENT_422_STRUCTURED_DECODE`
  (`AgentStructuredDecode`) became unreachable and was replaced by
  `WYRD_AGENT_422_STRUCTURED_INVALID`, per the spec.
- **Shared fixtures changed.** `policy-judge` and `judge-agent` now allow one
  repair (`max_iterations: 2`). The full journey aggregates run at change
  review.
- **Partial narrowing.** Slot narrowing applies only to a `verdict`-format
  schema, which carries the generated `SlotName` definition. A hand-written
  check schema is validated as authored and then judged by final evidence
  validation.
