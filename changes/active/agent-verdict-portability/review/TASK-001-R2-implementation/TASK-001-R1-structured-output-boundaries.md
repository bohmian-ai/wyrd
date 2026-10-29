---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-agent-verdict-portability
spec_revision: 2
requirements: [REQ-004, REQ-005, INV-001, AC-003, AC-005, AC-007]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4]
---

# Align structured validation with outgoing requests and preserve refusals

Route directly to `$wyrd-implement`. Repair the four independently validated
gaps while retaining the accepted portable-schema, slot-isolation and final
verification behavior. This is the first remediation; the earlier R1 authority
review directory was an intake attempt, not an implementation remediation.

## Authority and subject

- Approved [spec revision 2](../../spec.md).
- Original [TASK-001](../../tasks/TASK-001-portable-verdict-and-repair.md).
- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Original base: `911a8e278d829ab2722fc20ae881b67481dcf9f3`.
- Reviewed candidate: `c82414d38561cc3f7e17ac3f3ff52223378abde8`.
- [Verdict](verdict.md), [independent source validation](findings-validation.md).

Read the original spec/task and applicable root/nested rules. The diagnoses
below are the correction scope, not permission to replace runtime owners.

## Diagnose and correct the effective-schema authority

**FIND-TASK-001-1 (INCORRECT).** REQ-004 promises validation against the exact
effective schema sent. `skald-agent/src/loop_runtime.rs:378–383` compiles the
original Prompt's cached response type, supplied at lines 152 and 254, and uses
it at line 496. However `skald-spec::Prompt::render` calls `bind_mut`, which
substitutes all request string leaves including schema enum values. The
`before_model` chain at loop lines 402–428 can also replace the outgoing native
request after validator construction. Both `run_with` and `run_prompt` consume
this shared loop; Vala's input binding calls `run_prompt` too.

For a schema `answer.enum: ["${expected}"]` bound with `expected=ready`, the
provider receives `enum:["ready"]`, but a valid `{"answer":"ready"}` is
rejected against the literal placeholder. Repair then sends contradictory
guidance and can exhaust a valid run. Stale-schema-only answers can instead
pass. Existing tests keep template/sent schemas equal and miss this defect.

Use the final outgoing request as the schema authority for its response, after
rendering and request callbacks. Reuse Skald spec's existing native-request
response-type derivation (`response_type_of`), rather than a second schema
extractor or Vala-only validation. Keep the compiled validator while the
effective schema is unchanged; refresh validation and correction fragments
when it changes. Private control flow stays implementation-owned. Deleting
validation or forbidding existing bindings/hooks would break approved behavior;
this bounded shared-owner correction closes every affected entry path.

## Exclude provider refusals from output repair

**FIND-TASK-001-2 (VIOLATION).** The spec expressly excludes model-output repair
when a provider refuses without a usable answer. At loop lines 489–516, a
typed native refusal with no text becomes an empty string, is treated as
malformed JSON, and generates another provider call while iterations remain.
OpenAI's successful Chat response has a typed `refusal` field; native clients
and dispatch return it unchanged. Anthropic refusal is already classified by
the shared adapter as `FinishReason::ContentFilter`. Gemini safety endings
are sibling adapter classifications; paths already rejected for absent
candidates must keep their existing behavior. HTTP/status failures already
return before structured repair.

At the base, the no-answer refusal ended immediately in the old decode error,
which Vala treated terminally. Candidate repair now adds paid calls and may
accept a later judgment despite the approved exclusion. Existing malformed
answer tests do not establish refusal behavior.

Exclude explicit native refusals with no usable answer at the shared Agent
final-response boundary, where the decision to call again is owned. Reuse
typed response information and existing adapter finish classification. Do not
classify arbitrary empty/malformed text as refusal. Keep immediate terminal
failure and Vala inconclusive/no-outer-retry behavior through the approved
`StructuredOutputInvalid` / `WYRD_AGENT_422_STRUCTURED_INVALID` family with fixed
safe diagnostics. Do not restore the removed decode code or add a public
refusal code. The spec mandates this error on exhaustion without making that
the exclusive use, and separately forbids refusal repair. Correct directly
affected error/rustdoc/architecture/reference wording so an early refusal is
not described as having consumed all iterations; regenerate affected projections
from their source. A downstream Vala guard would act after extra calls and is
insufficient. Keep text-Agent behavior and transport failure classification.

## Make the live compatibility proof validate the reply

**FIND-TASK-001-3 (MISSING).** AC-007 requires validating each live reply against
the runtime-generated schema. Python
`tests/smoke/test_gateway_live_providers.py:103–113` defines a partial
`assert_verdict`, called at lines 425 and 478 without that schema. It accepts
`citations:{}` and a citation whose `locator.page` is `"not-an-integer"`.
Both were independently demonstrated without credentials and both violate the
sent schema. Provider request acceptance and Rust generation tests cannot prove
that these actual replies are valid, especially for non-strict OpenAI.

Validate each parsed reply against the exact runtime schema already obtained
for its request, using the installed Python JSON Schema implementation
(`jsonschema` is in the SDK lockfile). Replace the handwritten partial schema
checker instead of expanding it or copying a schema. Preserve native request
modes, strictness assertions and provider/model evidence. This is a small test
proof correction, with no new dependency or durable Python behavior.

## Complete the mandatory changed-item documentation

**FIND-TASK-001-4 (VIOLATION).** Root AGENTS explicitly blocks merge for missing
changed-item rustdoc; the Rust guide includes private/test helpers, fields and
panic contracts. Green Clippy does not discharge this gate. Complete this
bounded inventory with concise intent, relevant invariants and panic contracts,
preserving helper behavior and assertions (paths relative to `crates/`):

| Item | Gap |
|---|---|
| `skald/skald-agent/tests/integration/structured_output.rs:32` `registry_returning_text` | Missing behavioral rustdoc |
| Same file `:84` `format`, `:88` `chat_prompt` | Missing rustdoc and construction panic contracts |
| Same file `:138` `encodings`, `:192` `text_of`, `:197` `last_user` | Missing construction/serialization/content panic contracts |
| Same file `:224` `structured_output_parses_when_schema_declared` | Missing changed test behavior and panic documentation |
| `skald/skald-agent/tests/integration/callbacks.rs:346–347` `RecordingProvider`, `name` | Missing owner/field docs |
| Same file `:353` `RecordingProvider::new`, `:399` `Provider::name` | Missing changed construction/name behavior docs |
| `skald/skald-agent/tests/integration/loop_basic.rs:78` `openai_tool_call_response` | Missing widened helper docs |
| `skald/skald-agent/tests/integration/loop_gemini.rs:50` `gemini_text_response` | Missing widened helper docs |
| `skald/skald-agent/tests/integration/session_journal.rs:617,622,626` `RecordingJournal`, `new`, `events` | Missing widened recorder/construction/snapshot docs |
| `skald/skald-spec/src/prompt.rs:2186` `response_schema_edit` | Missing new-module rustdoc |
| `vala/vala-verify/tests/integration/blocks_agent.rs:824` `Recording::send` | Missing mutex panic contract |
| `wyrd-spec/src/verification/execution.rs:1091` nested `portable` assertion helper | Missing assertion panic contract |

Also document all items materially changed by the behavioral correction.
Do not expand into unchanged documentation debt or create a doc-check framework.
Static inspection and formatting suffice for the documentation-only edits.

## Preserved behavior and non-goals

Preserve generated schema structure, custom authored data/local-reference
meaning, named/inline strictness policy, per-invocation slot isolation, no-source
behavior, final typed/data/evidence guards, sensitive capture and secret-free
errors. Keep the same conversation, execution route, tool restrictions,
iteration/time budget, hooks and session/journal ownership. Ordinary malformed
JSON, non-object and schema-invalid answers remain eligible for bounded repair.

No new retry setting, runner, provider compatibility analyzer, schema artifact,
author-schema rewrite, code-check repair, permission change, persistence change,
deployment change, public refusal code or compatibility alias is authorized.

## Ordered proof and acceptance

1. **FIND-1:** In the existing structured-output tests, observe the actual
   outgoing schema after a variable-bound enum and a schema-changing
   `before_model` replacement. Its valid value succeeds; a stale-schema-only
   value is rejected/repaired. Cover both affected Agent entry paths as
   applicable. Correct call counts and unchanged Card state establish the
   shared-owner fix. Establish RED on the reviewed candidate, then GREEN.
2. **FIND-2:** With budget remaining and a valid response queued next, explicit
   Chat refusal and an adapter-classified no-answer refusal each cause one
   call, no correction and a fixed safe terminal error. Vala remains
   inconclusive with no outer retry. An ordinary malformed-then-valid answer
   still repairs. Preserve HTTP failure and text-Agent behavior. Establish RED
   for the no-repair obligation before fixing it.
3. **FIND-3:** A credential-free assertion check rejects the two diagnosed
   invalid replies and accepts a valid Verdict against the generated schema.
   Run the four existing focused live Verdict cases with exact-schema reply
   validation and record provider, model, mode and result. Missing credentials
   are explicit missing live proof, not a pass. Do not add typed/evidence
   semantics beyond the schema obligation.
4. **FIND-4:** Static inspection confirms complete meaningful rustdoc/panic
   sections for the inventory and materially changed remediation items. No
   artificial RED test is needed for documentation.

Record confirmed exact selectors for every added/named test, expected RED and
GREEN results and final commands in remediation completion evidence. Keep the
existing callback/encoding/budget/error tests intact. Use the narrowest owner
lanes for the actual write set, sequentially through mise:

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(/^(structured_output|callbacks|loop_responses|loop_gemini|session_journal|error_codes)::/)'
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(/^blocks_agent::/)'
mise run fmt:check
mise run lints
mise run py:format:check
mise run py:lints
```

For live proof, reuse the original task's repository-managed lifecycle and
credential/model configuration, selecting only the existing Verdict cases:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -rs -m smoke tests/smoke/test_gateway_live_providers.py -k verdict'
```

If error metadata or generated documentation changes, run `mise run codegen:regen`
and `mise run codegen:check`; apply `mise run docs:check` to affected public docs.
Run exact focused tests for any additional changed owner behavior. Do not run
full journey suites, full live gateway suites or broad aggregate gates for this
remediation; those belong to integrated change review. Preserve this verdict,
original task, prior candidate and cumulative range for focused closure review.

## Completion evidence

Candidate `c82414d38..f379283c5` (`396c1f415`, `2d8fb0b9b`, `f379283c5`);
cumulative TASK-001 range `911a8e278..f379283c5`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 effective schema | `skald-spec/src/prompt.rs` `ResponseType::of` (the former private `response_type_of`, now the one derivation); `skald-agent/src/loop_runtime.rs` compiles `StructuredOutput` from each outgoing request after `before_model`, reusing it while the schema is unchanged; `RunLoopInputs.response_type` removed | RED on `c82414d38`: `structured_output::structured_output_validates_the_bound_schema` (bound `ready` refused, queue exhausted), `structured_output::structured_output_validates_a_replaced_schema` (stale `foo/bar` accepted). GREEN after fix; covers `run_prompt` and `run_with`; Prompt response type unchanged | PASS |
| FIND-2 refusals | `skald-spec/src/adapter.rs` `ResponseAdapter::refused` (Chat `refusal`, or adapter `ContentFilter`); loop ends a structured run with empty text and a refusal as `StructuredOutputInvalid` with the fixed `provider refused without an answer`; error, architecture and Verifier reference wording; `errors.md`, `llms-full.txt` regenerated | RED on `c82414d38` (loop reverted): `blocks_agent::check_agent_refusal_is_not_repaired` made a second call. GREEN: `structured_output::structured_output_does_not_repair_refusals` (Chat and Anthropic, one call, no correction, fixed safe diagnostic), Vala test terminal `WYRD_AGENT_422_STRUCTURED_INVALID` after one call. Malformed-then-valid repair, text-Agent and HTTP-failure tests unchanged and passing | PASS |
| FIND-3 live reply validation | `tests/smoke/test_gateway_live_providers.py` `assert_verdict(text, schema)` uses `jsonschema.Draft202012Validator` against the exact sent schema; handwritten checker removed | `test_assert_verdict_validates_against_the_sent_schema` (offline): rejects `citations:{}` and `page:"not-an-integer"`, accepts a valid Verdict. Live, `-k verdict`: 4 passed — OpenAI `gpt-6-luna` non-strict named format; OpenAI `gpt-6-luna` strict; Anthropic `claude-haiku-5-5` `output_config`; Gemini `gemini-3.8-flash` `response_json_schema` | PASS |
| FIND-4 rustdoc | Every inventory item documented with intent and `# Panics` where it can panic, plus changed items (`ResponseType::of`, `ResponseAdapter::refused`, `StructuredOutput::schema`, error variants) | Static inspection; `fmt:check`, `lints` | PASS |

Commands (all exit 0):

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib                  # 102 passed
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --lib                 # 7 passed
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(/^(structured_output|callbacks|loop_responses|loop_gemini|session_journal|error_codes)::/)'  # 46 passed
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(/^blocks_agent::/)'  # 13 passed
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::plain_verdict_schema_is_portable)'
mise run fmt:check && mise run lints && mise run py:format:check && mise run py:lints && mise run py:typecheck
mise run codegen:regen && mise run codegen:check && mise run docs:generate && mise run docs:check
mise run test:gateway:smoke:live:pass -- -k verdict   # local mise.local.toml task: same lifecycle as the task's command, keys from pass
git diff --check
```

Non-goals held: no new retry setting, public refusal code, compatibility alias,
schema artifact, author-schema rewrite or new dependency; generated schema
structure, strictness policy, slot isolation and hosted code-check guards are
unchanged. Empty text without a refusal signal stays repairable by construction (the guard requires `refused()`); no separate test pins it. Full
journey suites were not run (change review).
