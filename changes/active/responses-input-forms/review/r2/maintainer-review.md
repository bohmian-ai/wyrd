# Independent maintainer review — R2

## Subject and result

**PASS.** No material maintainer finding remains in the complete cumulative
`807c4fe68ddcf43f325e970ed15f8a7c9918e0d4` →
`100adbc04a39ca22022fb013b3c0e93bfe903c3a` candidate (tree
`3bb993d4c32769cda67650eb02b991f981ebc3c2`). Branch HEAD is deliberately
unchanged; the reviewed candidate includes tracked remediation changes through
the immutable snapshot described in `subject.md`.

Read the approved revision-1 spec as the original task, full cumulative and
remediation diffs, prior verdict and validated ledger, and frozen remediation
evidence. No other R2 reviewer report informed this review. Source inspection
followed CodeGraph first; its incomplete TS/test navigation was supplemented
with exact owning-file reads. I changed only this report.

## Authority and changed-surface coverage

Applied AGENTS ownership/struct style, PyO3/stub/schema, testing and rustdoc
rules; agent-rules; active design/doctrine's Skald and shared-contract
boundaries; reference router; spec-driven-development; maintainer-style; and
Rust, PyO3, Python API/stubs, TypeScript and testing references. No Bifrost,
database, deployment or UI implementation is changed.

| Changed surface | Owner, surrounding consumers and proof inspected | Assessment |
|---|---|---|
| Responses input decoder/schema | `skald-spec/src/wire/openai_responses.rs::OpenAiResponsesInput`, complete `easy_items` and `input_items_schema`, canonical `OpenAiResponseItem`, input access/edit methods, content parts and source-schema unit | PASS: bounded synchronous transformations stay beside the existing wire owner; schema customizes input only and reuses canonical item/content schemas. |
| Tool-result representation/projection | Same module's `OpenAiFunctionCallOutput`, full `text` and `From<String>` bodies; serde derives and captured-body assertion | PASS: two real forms justify the closed enum; borrowed text and owned concatenation justify `Cow`. No utility owner, trait, transport or second parser is introduced. |
| Rust text-result writers | `skald-agent/src/request_builder.rs::tool_result_message`, `skald-prompt/src/prompt.rs::Prompt::with_tool_result`; surrounding provider branches and existing `loop_responses.rs` scenario | PASS: `.into()` selects the existing text form, preserving each writer's previous output and all adjacent error/normalization behavior. |
| Python accessor | Full `PyOpenAiResponseItem::as_function_call_output`, retained item/view creation, `PromptBuilderError::WrongVariant` mapping, authored/public prompt stubs and assembly owner | PASS: signature stays `str`; substantive rustdoc plus `# Errors` describes the lossy text projection and correct error. |
| Published/generated contract families | All nine Wyrd schema diffs, both eval schema fixture diffs, generated `_card_types.py` and `card-types.ts`; `scripts/gen_card_types.py` lowering and existing stub assembly/codegen lanes | PASS: one input-only named definition reaches each contract consumer. Canonical message/output alternatives are not widened to easy messages. Existing function-output widening remains the approved original change. |
| Served contract proof | Entire owning `gateway_ingress_publishes_typed_contracts` test, new validator/case section at `pg_openapi_contract.rs:1109`, gateway transparent request contract and Schemars-to-OpenAPI boundary | PASS: extends established served-document proof and existing JSON-schema dependency; no additional harness or check script. |
| Real Agents SDK journey | Entire `gateway-inference.test.ts`, `serverTest` setup/teardown, `Receiver` recording behavior, existing Chat/workflow siblings, deployment/openai/echoAgent/new response fixture helpers | PASS: retains existing server, credential, scoped-token and receiver owners; two new tests assert observable success and refusal without dispatch. |
| Test dependencies/lock | TS workspace devDependencies and full added lock graph, already-installed OpenAI/Zod use | PASS: the two Agents SDK packages are dev-only and required by the named real-client journey; no production SDK/runtime dependency is added. |
| Spec artifact | Revision-1 approved spec in cumulative diff | PASS: unchanged by remediation, remains the acceptance authority; no additional behavior or speculative option entered implementation. |

## Prior finding closure from source

**FIND-responses-input-forms-2 / prior MNT-001:** closed. Accessor rustdoc at
`crates/skald/skald-prompt/src/python/openai_responses.rs:571` states text is
returned as sent, parts concatenate input/output text in order without a
separator, non-text parts are omitted and stored form is retained. Its Errors
section names `WrongVariant`. The actual body projects through `output.text()`;
the existing error conversion maps the wrong-kind branch to
`WYRD_PROMPT_400_PROVIDER_MISMATCH`. Authored `stubs/prompt.pyi` and generated
`prompt/__init__.pyi` agree with that operation and keep `-> str`.

**FIND-responses-input-forms-3 / prior MNT-002:** closed. The Items field now
points to `input_items_schema`. The helper reuses
`generator.subschema_for::<OpenAiResponseItem>()` and existing content parts,
and registers `OpenAiEasyInputMessage` with required `role`/`content`, string
role, string-or-parts content and optional message-only discriminator. The
generated TS input is `string | readonly (OpenAiResponseItem |
OpenAiEasyInputMessage)[]`; generated Python input has the corresponding union
and named dataclass. Canonical item/output types do not acquire the easy
alternative. The change does not implement a second runtime validation path;
the literal is the declarative accepted-input schema which derive alone cannot
infer from the Serde rewrite.

**FIND-responses-input-forms-1:** maintainer coverage supports closure. The real
`OpenAIResponsesModel` runs with an actual Agents `Agent`/tool through the
existing Wyrd server fixture. The recording upstream observes exactly two
Responses calls, original easy user input, parts tool output and the final
answer; the scoped reader is rejected with 403 and no new upstream requests.
The fixture's path branch preserves all previous Chat test responses. Existing
agent-loop fixtures still assert text tool results through the enum's Text
conversion. Discovery acceptance/verification reviewers own final acceptance.

## Schema unit versus served test

Keep both; this is not a material duplication finding. The unit's concrete
distinct assertion is the **named easy-message definition and input reference
composition** used by checked-in schema and language generators, plus its
required fields/discriminator. The served test checks **behavior of the
composed runtime OpenAPI document** against five accepted and three malformed
payloads. A structurally equivalent inline schema could pass served validation
while losing the named declaration; a broken OpenAPI conversion/registration
could leave the source unit green. Both extend existing owners without new
dependencies/harnesses. The unit alone does not prove payload acceptance and
is not used as a substitute for the served proof or real SDK journey.

## Findings and limits

Material finding ledger: **empty**. No optional cleanup, style-only round or
unrelated hardening is recommended. Missing pre-fix journey execution is
disclosed in the evidence; existing correct forwarding receives
verification-only proof rather than artificial production churn.

This reviewer performed static source/diff review, not executable reruns or
generator execution. Frozen implementation evidence reports all required lanes
passing; `verification.md` independently records two selected source tests,
the served contract test, both exact SDK journeys and format/diff checks
passing on the same snapshot. Broader lint/codegen/typecheck/family results
remain supplied evidence. No merge, commit, deployment,
source modification or baseline checkout was performed.
