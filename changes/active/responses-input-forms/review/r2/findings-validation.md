# Independent structured Ponytail validation — R2

Recommended verdict: **FIX_REQUIRED**, solely for the explicit per-item rustdoc gate. All three prior findings are independently closed. No runtime, schema, forwarding, tenant-isolation or journey defect remains in the reviewed cumulative implementation.

## Immutable subject and evidence

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`. Base: `807c4fe68ddcf43f325e970ed15f8a7c9918e0d4`. Detached candidate: `100adbc04a39ca22022fb013b3c0e93bfe903c3a`, tree `3bb993d4c32769cda67650eb02b991f981ebc3c2`. The approved revision-1 `changes/active/responses-input-forms/spec.md` is also the original task by owner instruction. Reviewed cumulative `candidate.diff`, the remediation delta, all six independent discovery reports, `claim-comparison.md`, `verification.md`, the prior R1 verdict/validation and frozen remediation task/evidence. Source navigation used CodeGraph before source searches. This fresh validator changed only this report and did not execute tests or generators.

Applied AGENTS.md, agent-rules, the invoked task-review skill, reference router, spec-driven-development, maintainer-style, Rust core/testing guidance and applicable design/doctrine client, provider-native Prompt and journey authority. The complete changed production bodies, adjacent writers, request forwarding/admission, test fixtures, schema/declaration changes and accessor documentation were inspected independently. Discovery agreement is not the basis of validation.

## Reconciled cumulative acceptance

| Obligation | Independently inspected implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 easy input decodes as its typed equivalent | `openai_responses.rs::easy_items:338` fills a missing message discriminator only when role exists, expands string message content into input/output text according to role, then invokes the existing tagged item deserializer | Exact captured-body unit independently rerun PASS in R2 | PASS |
| REQ-001 text or parts tool output retains selected form | `FunctionCallOutput:436` uses the closed untagged Text/Parts enum at 470; serde retains string versus array. Existing `tool_result_message` and `Prompt::with_tool_result` use `From<String>` and retain text output | Captured-body serialization assertion; supplied Skald family PASS | PASS |
| REQ-001 both forms reach provider unchanged | `components/gateway/routes.rs::responses:915` validates through `typed_body:1723`, which discards the typed temporary and returns original JSON; `invoke_openai:1779` passes original body into admission. `wyrd-gateway/src/adapter/mod.rs::prepare:150` selects `native:531`, which copies the object and rewrites only model | Real Agents SDK journey asserts original easy input, replayed input, parts tool result, two Responses calls and final answer; exact R2 selector PASS | PASS |
| INV-001 tenant isolation | Unchanged `GatewayInvocation::admit_call:424` checks invocation permission before loading a snapshot using verified `caller.data_tenant_id`; request parser/schema supplies no tenant/credential/SQL authority | Same-boundary scoped-reader real agent gets 403 and zero upstream requests; exact R2 selector PASS | PASS |
| AC-001 captured SDK-body unit | `easy_input_messages_decode_as_typed_messages:762` covers easy user text, assistant text, canonical function item and parts result together | Exact R2 decoder test PASS | PASS |
| AC-002 format, lints, codegen and owning lanes | Existing lane/toolchain owners retained; only required dev-only Agents packages added | R2 format/diff checks PASS; frozen final-pass lint/codegen/Skald/TS/Python lane evidence supplied | PASS within stated evidence limits |
| Required real SDK/server journey, prior FIND-1 | Existing `serverTest` starts the actual Wyrd server and recording provider; genuine Agents Agent/run/tool and OpenAIResponsesModel perform initial request and continuation. Ambient access/workload tokens are cleared by fixture deployment | Both exact R2 SDK selectors PASS, one selected each | PASS |
| Input machine-contract parity, prior FIND-3 | Input Items alone uses `input_items_schema:312`, registering named easy alternative alongside canonical items with required role/content, string-or-parts content and omitted-or-message type. Generated TS/Python inputs and all schema families reflect this union; canonical item/output schema stays canonical | Exact source-schema unit and served OpenAPI test PASS; supplied codegen/typecheck PASS | PASS |
| Accessor rustdoc and authored/generated stub parity, prior FIND-2 | `python/openai_responses.rs:571` describes text return, ordered no-separator projection, omission of nontext and retained representation; Errors names WrongVariant. Stub source/public copy describe the same projection and stable mismatch error | Static full-body/error-map/source-stub comparison; supplied codegen/Python typecheck PASS | PASS |
| Scope and regression boundaries | Existing parser/schema owner extended; sibling writers preserve text; no new transport, retained lifecycle, SQL/authz/audit owner, compatibility route, arbitrary-JSON escape hatch, weakened assertion/timeout/skip or production TS dependency | Full cumulative diff and caller inspection | PASS |
| Rustdoc on every newly introduced Rust item | New local `const EASY` at `openai_responses.rs:315` has no rustdoc | Direct candidate source/diff inspection | FAIL, FIND-4 |

## All claims adjudicated

| Discovery claim | Disposition | Stable ID | Source-backed decision |
|---|---|---|---|
| INV-R2-001 | CONFIRMED | FIND-responses-input-forms-4 | Newly introduced constant has no rustdoc; AGENTS §16 explicitly names constants regardless of visibility and hard-blocks missing rustdoc. |
| STD-R2-001 | CONFIRMED, duplicate of INV-R2-001 | FIND-responses-input-forms-4 | Same item, same obligation and same correction; no second finding. |
| Prior FIND-responses-input-forms-1 | CLOSED | Preserved, not reopened | Actual stock-client tool journey and permission-refusal proof now cross the original ingress seam. |
| Prior FIND-responses-input-forms-2 | CLOSED | Preserved, not reopened | Original accessor/stub documentation now matches the exact existing projection and error. |
| Prior FIND-responses-input-forms-3 | CLOSED | Preserved, not reopened | Input schema producer, generated consumers and served contract now describe accepted easy inputs without widening canonical outputs. |

Behavior, maintainer, system and domain discovery proposed no further claims. No claim is rejected by counting votes. Their source traces were checked against the actual owners above. No follow-up discovery is required: the sole uncertainty is instruction reconciliation for a fully inspected item, not a conflicting source fact or unexplored caller.

## FIND-responses-input-forms-4 — new schema-name constant lacks mandatory rustdoc

Status: **CONFIRMED**. Discovery sources: INV-R2-001, STD-R2-001. Classification: **VIOLATION**. Exact location: `crates/skald/skald-spec/src/wire/openai_responses.rs:315`.

Obligation: AGENTS §16 states that every new Rust item must have rustdoc, expressly including “constants” “regardless of visibility,” and that missing rustdoc on any touched item is “a hard blocker even when the code compiles and tests pass.” Agent-rules repeats the hard acceptance criterion. The invoked skill expressly preserves missing rustdoc as blocking.

Source diagnosis and reachability: the remediation introduces `const EASY: &str = "OpenAiEasyInputMessage";` with no rustdoc. The full enclosing `input_items_schema` body was inspected. Its only attachment is the schema_with on `OpenAiResponsesInput::Items`. Inside the helper, EASY names the definition inserted into the existing generator and the reference constructed with that generator's configured definitions path. Both uses refer to the same local constant; there are no sibling writers or consumers outside the function. Input request schema generation, the transparent GatewayResponsesRequest schema, generated authoring declarations and the served document consume its resulting definition. Runtime normalization and gateway raw forwarding remain separate and correct. The enclosing helper's documentation is substantive but does not document this separately introduced constant item under the explicit per-item requirement.

Consequence: only the explicit documentation completion gate is unmet. There is no claimed executable, security, durability or public-schema defect. This is not a reopened schema-parity finding and does not require another generator, consumer guard, type or runtime helper.

Ponytail ladder and correction boundary: deleting the schema hook would reopen FIND-3; deleting the named definition would remove required authoring-contract structure. Existing Schemars registration already supplies the outcome. Keep that mechanism and its exact schema. A substantive rustdoc line describing the constant as the shared definition/reference identity is the smallest safe correction. Removing the constant just to avoid its documentation obligation or duplicating its literal at both uses adds no needed behavior and is not the recommended remedy. No installed dependency, new test, parser, production refactor or wider documentation cleanup is needed.

Focused closure proof: inspect that this constant has substantive rustdoc explaining its role in keeping definition registration and references aligned; confirm the only source change is documentation and run `mise run fmt:check` plus `git diff --check`. No new runtime test, full journey, lint/typecheck rebuild or codegen rerun is needed to prove this documentation-only gap; those existing passing results remain evidence for the unchanged behavior.

### Reconciling the no-style-only-round instruction

The skill says, “Batch bounded documentation corrections with the same remediation task. Keep the rustdoc gate intact, but never create a style-only round.” It also explicitly says missing rustdoc stays blocking per AGENTS. AGENTS is the repository authority and expressly treats documentation as implementation correctness, with no visibility exception. Therefore the no-style-only-round sentence cannot waive this mandatory gate: otherwise “Keep the rustdoc gate intact” and the specific missing-rustdoc exception would be ineffective. Optional naming/layout/style claims remain excluded; this finding concerns only the separately mandated rustdoc item.

Recommended disposition/packaging: retain this one gate failure as the minimum continuation of the existing schema remediation, preserving closure of all three prior findings. If the orchestrator must emit a FIX_REQUIRED task under the skill's verdict rule, make it a bounded completion of that remediation containing only this documentation correction and static/format proof. Do not invent a behavioral finding, reopen prior IDs, request a general style round or broaden proof to justify the small remaining gate. The stronger explicit hard-rustdoc requirement controls any irreconcilable reading of “never create a style-only round.” No specification revision is required.

## Schema unit versus served OpenAPI proof

No test-removal finding is justified. The source unit asserts that easy messages have a named definition and a reference beside canonical items, plus required fields/discriminator; that structure supports generated language declarations. The served test compiles the actual mounted GatewayResponsesRequest document and validates five accepted and three malformed payloads after schema conversion/registration. These share a producer but prove different boundaries. Either alone misses part of the supplied proof: source structure does not establish served validity, while served validity does not assert the named source authoring-definition structure. The unit is not an added repository check script or a check verifying another check. The owner concern is answered by these distinct properties, without optional test editing or a retroactive FIND ID.

## Verification limits

This validator performed independent static source validation only. `verification.md` records independent R2 passing exact schema/decoder (two selected), served contract (one selected), both SDK journeys (one each), format and whitespace checks. Broader lint, codegen, family and language lanes remain supplied final-run evidence from frozen remediation execution. No live provider, broad aggregate, cloud lane, pre-fix server rebuild or manufactured RED history was run. The new journey verifies forwarding already implemented in R1; source-schema regression has recorded RED/GREEN. No test was weakened to clear a failure. Source remains immutable; the orchestrator owns the final candidate-consistency check.

Final deduplicated retained ledger: **one finding, FIND-responses-input-forms-4**. Recommended verdict: **FIX_REQUIRED** under the explicit hard rustdoc exception; no runtime risk has been established. Prior FIND-1/2/3 remain closed.
