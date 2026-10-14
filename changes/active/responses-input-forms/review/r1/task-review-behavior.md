# Behavior acceptance review

## Subject and result

**FAIL** — the changed decoding satisfies the two captured request forms by source inspection, but the public Agents SDK gateway workflow has no real-server journey proof required by repository authority. The orchestrator supplied passing format-check, exact captured-body test, and `test:skald` (345/345) results. Lints and codegen remain pending and must be reconciled in the verdict.

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/responses-input-forms`.
Base: `807c4fe68`; candidate: `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`.
Approved spec and original task: `changes/active/responses-input-forms/spec.md` revision 1. There is no separate task packet, as explicitly directed by the user. Reviewed the complete cumulative diff. No prior findings or remediation exist.

Authority applied: AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, reference router, spec-driven-development, maintainer-style, Rust, PyO3, Python contract, TypeScript contract, and testing workflow references. The spec's recorded journey gap is not an explicit exception to AGENTS §11, agent-rules' journey requirement, or design doctrine 20.

## Caller-to-result and regression evidence

- `routes.rs::responses` (915–929) decodes through `typed_body::<GatewayResponsesRequest>`. `GatewayResponsesRequest` transparently wraps `OpenAiResponsesRequest` (`crates/wyrd-spec/src/gateway/openai.rs:46`). `typed_body` (1723–1739) validates a borrowed raw body and returns the original `Value`; it does not re-serialize normalized easy messages.
- `OpenAiResponsesInput::Items` (`openai_responses.rs:291`) uses `easy_items` (307–333), adding message type only when absent and a role is present, and expanding string content into input/output text by role. Non-message items still decode through the existing `OpenAiResponseItem` owner. The new parser does not introduce a second gateway validator, transport, loader, or retained-state owner.
- `OpenAiFunctionCallOutput` (427–434) uses an untagged text/parts enum. The captured `input_text` array is represented without collapsing it to text. Serde serialization retains its text-versus-array form.
- `invoke_openai` (`routes.rs:1779–1820`) moves the original body into the existing `GatewayInvocation`; `adapter::prepare` (`crates/wyrd/wyrd-gateway/src/adapter/mod.rs:150–231`) takes the native Responses path for OpenAI and compatible deployments. `native` (531–540) changes only the model projection, leaving both input forms untouched. Thus the easy-input normalization is validation-only at gateway ingress.
- Existing text producers remain text producers: `request_builder.rs::tool_result_message` and `Prompt::with_tool_result` use `String::into()` to select the new Text variant. The existing Skald Responses tool-loop expected text output was mechanically adapted; its behavioral assertions were preserved.
- The existing Python accessor intentionally retains a string return through `OpenAiFunctionCallOutput::text()`; it does not mutate the stored parts or gateway forwarding. This is a read projection, not a second wire representation.
- Tenant identity is not decoded from the added forms. The unchanged `Caller::from_authenticated` takes identity from the verified principal; the gateway's invocation retains caller-scoped admission/accounting. No new SQL, tenant field, identity resolution, or privilege path enters the diff.
- Generated schema and Python/TypeScript changes represent the expanded function-output union. No manifest, endpoint, retry, timeout, credential, transport, or lifecycle change entered the candidate.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 easy message decodes as typed equivalent | `easy_items`, `openai_responses.rs:307–333` | New `easy_input_messages_decode_as_typed_messages`, lines 692–716; execution awaited from orchestrator | PASS by source |
| REQ-001 function-call output accepts text or content parts and preserves its form | Untagged `OpenAiFunctionCallOutput`, lines 427–434; unchanged text producers use From<String> | New unit covers parts round trip; existing `loop_responses` expected text output remains intact | PASS by source |
| REQ-001 both forms reach provider unchanged | `typed_body` returns original raw Value; native adapter changes only model | No real-server test of these forms; see BEH-001 | PASS by source, proof gap |
| INV-001 tenant isolation on changed paths | No new identity or durable access; existing authenticated Caller and GatewayInvocation remain owners | No new path-specific tenant journey; existing boundary not altered | PASS by source |
| AC-001 unit decodes captured Agents SDK body shape | New unit includes easy user, typed assistant text, function call with replay fields, and parts output | Exact focused command: `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'`; result awaited | PASS for implemented test |
| AC-002 format, lints, codegen and owning lanes pass | Owning lane is `test:skald`; generated changes require codegen checks | Supplied format-check PASS, exact unit PASS, family tests 345/345 PASS; lints/codegen pending | FAIL: complete success not established |
| Stronger authority: public capability ships real SDK → real server journey | Existing TS gateway journey is Chat Completions only; candidate adds only an isolated decode unit | Spec explicitly records missing Agents SDK Responses tool journey; repository search found no `OpenAIResponsesModel` or Agents SDK use in SDK/example sources | FAIL — BEH-001 |
| Scope excludes unrelated feature/architecture changes | Small wire decoding extension, necessary consumer adaptation, generated contracts only | Complete base-to-candidate diff inspection | PASS |

## Proposed finding

### BEH-001 — MISSING: the newly accepted SDK Responses forms lack the required real-server user journey

**Violated obligation:** AGENTS.md §11, architecture/agent-rules.md (“Every user- or agent-facing capability ships a user-journey test”), and architecture/wyrd-design.md doctrine 20. These govern the spec objective and REQ-001's provider-forwarding guarantee. The spec explicitly records the absent TypeScript journey but does not amend the repository's completion rule.

**Exact changed location:** `crates/skald/skald-spec/src/wire/openai_responses.rs:692–716`, the candidate's only new test. The necessary consuming path is `/v1/responses` at `crates/wyrd/wyrd-server/src/components/gateway/routes.rs:915`.

**Evidence and reachable consequence:** the new test stops at `serde_json::from_value`, then asserts the normalized typed input. It never authenticates a client, calls the route, crosses `GatewayInvocation`, reaches an upstream, or executes an SDK tool loop. Existing `sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts:34–45,79–104` builds a Chat Completions-only deployment and client. Existing Python official-client gateway coverage also exercises Chat Completions. A typed round trip actively reshapes easy messages, so it cannot establish the separate REQ-001 guarantee that the gateway provider sees the original easy form. The existing Skald tool-loop integration consumes constructed typed items against a provider mock, bypassing the public gateway and real server. The regression that motivated this task occurs at exactly that cross-boundary ingress.

**Minimal correction and existing owner to reuse:** extend the existing TypeScript `serverTest` gateway integration owner, with its recording local provider and repository-managed WyrdTestServer. Drive the real Agents SDK Responses model through the normal OpenAI client pointed at `/v1`, invoke one tool, and assert that the provider recorded the easy message and the content-parts tool output in their original forms, followed by the final answer. Keep the existing authentication/credential mechanism. Include the corresponding refused caller check without upstream dispatch on this Responses path. Do not add a transport wrapper, alternate decoder, live-provider requirement, or new fixture architecture. This correction adds proof, not product behavior or an expensive architectural decision.

**Focused closure proof:** the exact new Vitest test(s), run through `mise exec --` and the repository Postgres/migration setup used by `ts:test:gateway:journey:inner`; include the file in that existing lane if a separate test file is warranted. Run only the focused gateway test file during remediation. Complete gateway journeys belong to final change review, not this remediation task. Preserve the current isolated decode test.

## Limits

No source was edited and no mutating generator ran. Executable verification was run by the orchestrator: format-check, exact captured-body test, and all 345 Skald tests pass; outstanding checks must be added to the reconciled verdict. This report does not claim a functional ingress defect beyond the missing required cross-boundary proof; source inspection supports the intended happy path. No optional hardening or unrelated cleanup is proposed.
