# Provider-contract domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review boundary: provider-tagged `ProviderRequest` serialization, authoring and persistence projections, provider dispatch, generated schemas, and registered Vertex gateway execution.

The commits resolved to the requested objects before and after the review. No build, test, Cargo, or mise command was run. Source was read from the immutable candidate object; no sibling `TASK-004-r2` report was inspected.

## Boundary, authority, and source coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Durable request discriminator | Revision 13 requires serde adjacent tagging, derived `Deserialize`, no shape-ordered fallback, and no legacy reader | `crates/skald/skald-spec/src/request.rs:18-59,90-103`; cumulative diff confirms deletion of the hand-written deserializer, helper wire structs, and manual `PartialEq` | PASS |
| Variant/body refusal | Revision 13 requires deserialization to use the named variant and fail instead of falling through | `request.rs:24-27,307-337`; the adjacent tag selects one variant, and focused source tests cover four cross-dialect mismatches | PASS |
| Gemini/Vertex identity | Revision 13 requires a Vertex body, despite sharing Gemini's wire shape, to remain Vertex | `request.rs:278-288`; `skald-spec/src/authoring.rs:232-267`; `skald-prompt/src/builder.rs:238-242,270-307`; `ProviderRequest::provider` at `request.rs:90-103` | PASS |
| `OpenAiChatCompatible` and `RawV1` dispatch identity | Revision 13 keeps `ProviderName` inside these bodies | `request.rs:31-37,52-58,97-101`; `request.rs:290-305`; `skald-cache/src/key.rs:58-89`; `skald-providers/src/clients/{openai,anthropic,google,vertex}.rs` RawV1 send branches | PASS |
| Raw native JSON | Revision 13 permits `RawV1.body` as `serde_json::Value` and requires only the native body to reach the provider | `skald-prompt/src/builder.rs:244-267`; `skald-providers/src/raw.rs:10-31`; each native client converts `Value` to `RawValue` at its send boundary; `wyrd-cards/src/prompt/io.rs` round-trip coverage | PASS |
| Prompt transformations preserve the tag | Provider identity must survive render/bind/media operations | `skald-spec/src/prompt.rs` serializes and deserializes the full adjacent-tagged enum in `bind_mut`, and matches Vertex separately for settings, text, system, media, and splitting; the removed `deserialize_request_like` no longer recreates a variant by shape | PASS |
| Cards, loaders, and persistence | Every saved native Prompt must use the tagged request and retain its provider across save/load/register/read | `wyrd-spec/src/card/prompt/mod.rs`; `wyrd-cards/src/prompt.rs`; `wyrd-cards/src/prompt/io.rs`; `wyrd-loader/src/parse.rs`; all changed Workflow-loading fixtures use `request.provider` plus `request.body` | PASS |
| SDK projections | Rust/Python/TypeScript authoring and serialization must expose the revised contract without a compatibility alias | Python Prompt tests consistently read the new `provider`/`body` envelope and assert Vertex stays Vertex. TypeScript's inspected Prompt journeys use the still-supported declarative `PromptDraft` surface (`provider`, `model`, `messages`) rather than constructing native `ProviderRequest`; Revision 13 does not require replacing that distinct authoring surface. No legacy native-request alias was found. | PASS |
| Generated contract | Generated schemas must describe the provider/body envelope | `crates/wyrd-spec/schemas/prompt_spec.json:3899-4132` and every other changed schema/golden embedding `ProviderRequest` contain the tagged `oneOf` variants, including distinct Gemini and Vertex tags and the nested identity fields for OpenAI-compatible and RawV1 | PASS |
| Registered Vertex server path | A registered Vertex Prompt must round-trip and reach the in-process Vertex projection | `wyrd-server/tests/pg_workflow_runs.rs:1130-1158,2208-2224,2351-2421` registers the declarative Vertex Prompt, invokes the registered Workflow, and expects the Vertex resource path; `wyrd-server/src/components/gateway/workflow.rs:193-249` maps `ProviderRequest::Vertex` to `IngressDialect::VertexGenerateContent`; the recorded S5 evidence is PASS | PASS |
| Examples and documentation | Revision 13 explicitly says every example moves to the tagged form | Tracked YAML examples and `architecture/wyrd-design.md:1607-1617` are tagged, but the approved spec's three canonical Prompt examples remain untagged at `spec.md:519-599` | FAIL |
| No migration or compatibility surface | Revision 13 prohibits a migration, compatibility reader, or alias | The old shape-ordered deserializer and its helpers are deleted; no replacement compatibility reader, legacy alias, setting, check, or option was found in the reviewed boundary | PASS |

## Finding

### DOMAIN-PROVIDER-1 — MISSING: the canonical approved-spec examples still use the removed untagged request wire form

- **Violated obligation:** Revision 13 states that every fixture, example, SDK authoring path, generated schema, and stub moves to the tagged form, with no compatibility reader or alias (`changes/active/skald-workflow-runtime/spec.md:21-42`).
- **Exact location:** `changes/active/skald-workflow-runtime/spec.md:519-599`, specifically the `request` blocks beginning at lines 531, 555, and 579.
- **Evidence:** Each example places `model` and `messages` directly under `request`. The implemented contract requires `request: { provider: open_ai_chat_completion, body: { model, messages } }` (`crates/skald/skald-spec/src/request.rs:18-27`), and the generated Prompt schema requires both `provider` and `body` (`crates/wyrd-spec/schemas/prompt_spec.json:3899-3920`). The repository's corresponding executable examples under `examples/workflows/code-review/prompts/` already use the tagged form, demonstrating the intended correction without new machinery.
- **Observable consequence:** A user copying any of the three canonical Workflow Prompt examples from the approved specification gets a deserialization error. There is deliberately no compatibility path that could make those examples work.
- **Required testable correction:** Update only those three `request` examples to the existing adjacent-tagged OpenAI Chat form used by `examples/workflows/code-review/prompts/{security,correctness,final-reviewer}.yaml`. Do not add a compatibility reader, alias, migration, check, setting, or alternate parser. Static comparison to the executable examples plus the existing recorded codegen/example verification is sufficient proof.

This is not a request for a bespoke documentation check. The established native request shape and the repository's executable examples already supply the standard mechanism and source of truth.

## Verification evidence and limits

Recorded implementation evidence reports PASS for `test:skald`, `codegen:check`, `test:wyrd`, Python unit/integration, TypeScript unit/integration, the Rust Workflow-loading journey, gateway journey, and S5 `server_routes_keep_gateway_and_external_ownership`. The recorded Revision 13 unit selectors are:

- `request::round_trip::vertex_request_reads_back_as_vertex`
- `request::round_trip::raw_v1_request_reads_back_when_body_precedes_tag`
- `request::round_trip::mismatched_body_is_refused`

The recorded S5 proof covers OpenAI Chat, OpenAI Responses, Anthropic Messages, Gemini, and Vertex, including the exact Vertex in-process provider path. These results were reviewed as supplied evidence and were not rerun, per the strict read-only instruction.

The cross-dialect mismatch test samples four structurally invalid pairs rather than an exhaustive Cartesian product. This is not a finding: adjacent tagging itself eliminates fallback, provider wire types intentionally retain forward-compatible fields, and some provider-native JSON shapes overlap semantically. The required invariant is that the named variant alone is attempted, which the derived adjacent-tag representation establishes.

No verification evidence can make the stale specification examples valid under the implemented wire contract; the gap is directly established by source and generated schema.

## Overall result

**FAIL**

The provider-tagged runtime and persistence boundary satisfies Revision 13, including durable Vertex identity and in-process Vertex gateway dispatch. `DOMAIN-PROVIDER-1` remains because three canonical examples in the approved specification were not migrated to that same required wire form.
