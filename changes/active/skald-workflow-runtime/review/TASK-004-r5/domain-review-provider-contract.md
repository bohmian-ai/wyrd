# Provider/request/gateway contract domain review

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in their preceding review directories
- Domain: every `ProviderRequest` discriminant from native Prompt authoring and
  persistence through Agent request construction, Workflow route validation,
  provider/external-gateway clients, the in-process Wyrd gateway, generated
  schemas, fixtures, and public language projections.

`HEAD` resolved to the requested candidate before this report was written.
`.codegraph/` is absent, so I used the immutable Git diff, repository search,
and direct source inspection. Per the standing direction, I ran no build,
test, Cargo, mise, code-generation, or package-manager command.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Adjacent-tagged request wire | Revision 13 requires standard serde adjacent tagging, derived `Deserialize`, no shape-ordered fallback, and no compatibility reader. `crates/skald/skald-spec/src/request.rs:18-59` implements the exact `provider`/`body` representation and the cumulative diff deletes the hand-written reader and helper structs. | PASS |
| Named-body decoding and Gemini/Vertex identity | The tag must select the named variant, including identical Gemini/Vertex GenerateContent bodies. `request.rs:24-27,278-337` exercises the derived selection and refusal behavior; `ProviderRequest::provider` at `request.rs:90-103` keeps dispatch identity typed. | PASS |
| Embedded provider identity and raw JSON | `OpenAiChatCompatible` and `RawV1` retain their `ProviderName` within the tagged body at `request.rs:31-37,52-58`. The R1 record identifies `RawV1.body: Value` as the approved representation; prompt builders, runtime dispatch, cache behavior, and native send sites consistently use that representation. | PASS |
| Prompt transforms and Agent loop | `crates/skald/skald-spec/src/prompt.rs` preserves the complete tagged enum through binding/media/settings transforms. `crates/skald/skald-agent/src/request_builder.rs:40-51,222-241` explicitly treats `OpenAiChatCompatible` as the OpenAI Chat conversation dialect and preserves it while rebuilding messages. | PASS |
| Prompt/Card persistence and graph hydration | `wyrd-spec` Prompt contracts, `wyrd-cards` IO, `wyrd-loader`, shared-client hydration, and server `PinnedWorkflowGraph` move typed Prompt bodies without provider inference. A registered Prompt therefore reaches Workflow validation with the stored discriminant intact. | PASS |
| Native provider and cache/runtime dispatch | The standard typed variants dispatch by `ProviderRequest::provider`; native OpenAI, Anthropic, Google, and Vertex clients unwrap only their owned body variants. Cache keys retain provider scope, and opaque RawV1 remains non-cacheable. | PASS |
| Workflow route matching | `REQ-039` requires an external protocol to match the Prompt request dialect before dispatch. The five primary variants are mapped at `crates/skald/skald-workflow/src/route.rs:430-453`, but the OpenAI Chat-compatible variant is omitted even though its declared semantics and Agent-loop classification are OpenAI Chat. | **FAIL — `DOMAIN-PROVIDER-1`** |
| Direct external-gateway send | `ExtGateway` must call the selected native protocol directly. `crates/skald/skald-providers/src/clients/external.rs:95-139` posts the five primary variants, but also omits `OpenAiChatCompatible`; extending only route validation would therefore reach a second rejection at the provider client. | **FAIL — `DOMAIN-PROVIDER-1`** |
| In-process Wyrd gateway | `crates/wyrd/wyrd-server/src/components/gateway/workflow.rs:193-249` projects OpenAI Chat, OpenAI-compatible Chat, OpenAI Responses, Anthropic, Gemini, and Vertex onto the existing gateway owner. Gemini and Vertex retain separate ingress and answer variants. Unsupported operations fail before admission. | PASS |
| Registered Vertex proof | `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:2324-2420` registers provider-specific Prompts and asserts exact upstream paths. The Vertex case reaches the Vertex project/location path, proving the persisted tag did not return as Gemini. | PASS |
| Schemas, fixtures, and SDK projections | Generated Prompt schemas describe a tagged `oneOf`, including distinct Gemini/Vertex tags and nested identities for OpenAI-compatible and RawV1. Native YAML fixtures/examples use `request.provider` and `request.body`; Python assertions read the new body envelope, while supported declarative authoring compiles into the same typed owner. No legacy native reader or alias was found. | PASS |
| R4 deadline remediation | R4 changes only the prepared-run deadline projection into built-in query tools. It does not alter ProviderRequest, route selection, gateway projection, provider clients, Prompt persistence, or generated contracts. | PASS |
| Standing DRIFT direction | The provider-tag correction itself uses standard serde tagging and existing schema, Prompt, provider, and journey owners. No provider-specific scanner, bespoke check, option, compatibility layer, alternate registry, or setting entered the cumulative diff. The correction below extends two existing exhaustive dispatch points and requires no new machinery. | PASS |

## Material finding

### DOMAIN-PROVIDER-1 — INCORRECT: an OpenAI Chat-compatible Prompt cannot use an `openai_chat` external gateway

- **Violated obligation:** `REQ-039` requires `ExtGateway.protocol` to match the
  Prompt's request dialect and preserve that dialect through direct dispatch.
  `ProviderRequest::OpenAiChatCompatible` is expressly a custom provider using
  OpenAI Chat Completions semantics (`crates/skald/skald-spec/src/request.rs:31-37`),
  and the Agent owner classifies it as the OpenAI Chat loop
  (`crates/skald/skald-agent/src/request_builder.rs:40-47`).
- **Exact locations:**
  `crates/skald/skald-workflow/src/route.rs:430-453` and
  `crates/skald/skald-providers/src/clients/external.rs:95-139`.
- **Producer-to-consumer evidence:** A typed or stored Prompt may contain
  `OpenAiChatCompatible { provider: Custom(...), request }`. Graph hydration
  preserves that variant, and `validate_prompt_loop_request` accepts it. This
  is not a dormant constructor: native public fixtures such as
  `tests/fixtures/workflow-loading/team/prompts/correctness.yaml:9-20` author
  exactly that tagged variant and exercise it through the shared loader. When
  such a Prompt's resolved route is
  `ExtGateway { protocol: OpenAiChat, ... }`,
  `ResolvedGraph::resolve` calls `protocol_matches` at
  `crates/skald/skald-workflow/src/plan.rs:74-93`; the matcher recognizes only
  `OpenAiChatCompletion`, so the run is refused as a dialect mismatch before
  dispatch. If that first omission alone were corrected,
  `ExternalGatewayClient::send` would still return `VariantMismatch` because
  it also recognizes only `OpenAiChatCompletion`. The sibling in-process Wyrd
  gateway already handles both Chat variants at
  `wyrd-server/src/components/gateway/workflow.rs:193-205`, confirming that the
  body has a valid existing Chat projection rather than requiring translation.
- **Observable consequence:** A valid custom OpenAI-compatible Prompt can run
  through its custom native provider or the Wyrd gateway, but cannot use the
  external OpenAI Chat gateway route intended for that same wire dialect. It is
  rejected before any upstream request, contrary to the stored route and
  request-dialect contract.
- **Smallest testable correction:** Extend the existing OpenAI Chat match arm in
  both `protocol_matches` and `ExternalGatewayClient::send` to accept
  `OpenAiChatCompatible` and post its inner `request` through the existing
  `/chat/completions` path, returning the existing OpenAI Chat response variant.
  Add one case to the current external-gateway Workflow proof that uses a
  custom OpenAI-compatible Prompt and asserts the existing path/body/response
  behavior. Do not add a protocol variant, translation layer, compatibility
  reader, option, check, setting, registry, or test harness.

This is one source-to-sink finding: a guard-only change in Workflow validation
would leave the same accepted dialect rejected by the existing downstream
client.

## Verification evidence and limits

- Recorded evidence reports green focused adjacent-tag request tests, Skald,
  code generation, Rust/Python/TypeScript authoring and loading paths, Workflow
  loading, Wyrd/server lanes, and Scenario 5's OpenAI Chat, OpenAI Responses,
  Anthropic, Gemini, Vertex, fallback, deadline, and cancellation branches.
- Scenario 5 constructs an ordinary `OpenAiChatCompletion` Prompt; neither it
  nor the external-client test covers `OpenAiChatCompatible`. Thus its recorded
  success does not contradict the two exact match omissions above.
- No executable verification was rerun under the strict read-only direction.
  The current result is based on source reachability, the complete cumulative
  diff, and recorded evidence.
- No verification limitation prevents adjudication: the producer, validation
  guard, downstream send owner, and sibling working projection are all present
  in the candidate source.

## Overall result

**FAIL**

`DOMAIN-PROVIDER-1` is a bounded provider/request/gateway contract defect. The
rest of the reviewed provider-tagged contract, including durable Vertex
identity and its registered in-process gateway path, passes.
