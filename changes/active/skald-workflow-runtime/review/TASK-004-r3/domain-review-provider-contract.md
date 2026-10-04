# Provider request contract domain review

## Subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations: `review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md` and `review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Reviewed domain: provider-tagged request wire format, prompt authoring and transformation, persistence and Card IO, generated contracts and public language projections, provider/cache dispatch, and registered Vertex execution through the in-process gateway.

`HEAD` resolved to the requested candidate before this review. The review used the complete base-to-candidate diff and current candidate source. No build, test, Cargo, mise, package-manager, code-generation, or other executable verification command was run. No other `TASK-004-r3` report was read.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Provider request discriminator | Revision 13 requires serde adjacent tagging, derived deserialization, native provider JSON under `body`, and deletion of the shape-ordered fallback reader | `crates/skald/skald-spec/src/request.rs:18-59`; cumulative diff confirms `RawProviderRequest`, `OpenAiChatCompatibleRequest`, the hand-written `Deserialize`, manual `PartialEq`, and shape-based helper are removed | PASS |
| Named-body refusal | The named variant alone must be decoded; an invalid body must not fall through to another variant | `request.rs:24-27,307-337`; serde's adjacent tag chooses one variant, and the recorded mismatch cases exercise structurally invalid cross-dialect bodies | PASS |
| Gemini versus Vertex identity | A Vertex `GenerateContent` body must remain Vertex even though Gemini has the same native body shape | `request.rs:90-103,278-288`; `skald-spec/src/authoring.rs:232-267`; `skald-prompt/src/builder.rs:238-242,270-307`; the provider tag, not body shape, controls identity | PASS |
| Embedded dispatch identity | `OpenAiChatCompatible` and `RawV1` retain their `ProviderName` inside the adjacent-tagged body | `request.rs:31-37,52-58,90-103,290-305`; runtime dispatch reads `ProviderRequest::provider`; cache treats raw input as opaque; native provider clients accept `RawV1` only after provider selection | PASS |
| Raw JSON representation | Revision 13's approved `RawV1.body: serde_json::Value` is converted to `RawValue` only at provider send sites | `skald-prompt/src/builder.rs:244-267`; `skald-providers/src/clients/{openai,anthropic,google,vertex}.rs`; `skald-providers/src/raw.rs:10-31`; Card IO and unit fixtures compare the JSON value, not a legacy byte container | PASS |
| Prompt transformations | Bind, render, settings, media, and tool projection must preserve the selected provider variant | `skald-spec/src/prompt.rs:136-280` and provider-specific helpers; `bind_mut` serializes and deserializes the complete tagged enum, while Gemini and Vertex retain distinct match arms throughout | PASS |
| Declarative authoring and builders | Existing `PromptDraft` remains a supported input surface but compiles to the tagged native `ProviderRequest`; builders must emit the same native variants | `skald-spec/src/authoring.rs:21-118,133-267`; `skald-prompt/src/builder.rs`; provider selection is explicit and Vertex constructs `ProviderRequest::Vertex`, not a Gemini-shaped inferred value | PASS |
| Prompt Card persistence | Saved native Prompts must serialize with the tag and retain it through filesystem, loader, registry, and hydration paths | `wyrd-spec/src/card/prompt/{spec,codec}.rs`; `wyrd-cards/src/prompt.rs`; `wyrd-cards/src/prompt/io.rs`; `wyrd-loader/src/parse.rs`; `wyrd-client/src/cards/hydrate/workflow.rs`; no persistence layer rewrites or infers the provider | PASS |
| Fixtures and examples | Revision 13 requires native Prompt examples and fixtures to use `request.provider` plus `request.body`, without a legacy reader | Every top-level native `request` inspected under `examples/workflows`, `tests/fixtures/workflow-loading`, and the changed CLI Agent fixtures uses the adjacent-tagged form. The three canonical examples in `spec.md:519-604` now match `examples/workflows/code-review/prompts/*.yaml` | PASS — prior `FIND-TASK-004-15` closed |
| Generated schemas | Generated contracts must distinguish variants by the tag and expose the native body under `body` | `crates/wyrd-spec/schemas/prompt_spec.json:3899-4132` and the other changed schema/golden embeddings contain the same `oneOf`, with disjoint Gemini/Vertex tags and the nested identities required for OpenAI-compatible and RawV1 variants | PASS |
| Rust, Python, and TypeScript projections | Public authoring and loading paths must consume the revised native contract without a compatibility alias | Rust uses the owning `skald_spec::ProviderRequest`. Python serialization assertions consistently read `model_dump()["body"]` and prove Vertex remains Vertex. TypeScript Workflow loading consumes the same updated registered Prompt fixtures through the shared loader/client path; its distinct declarative PromptDraft input remains expressly supported by the spec. No legacy native-request alias was found | PASS |
| Cache and provider clients | Dispatch and cache behavior must use the durable provider identity, not re-infer it from body shape | `skald-runtime/src/dispatch.rs:22-70`; `skald-cache/src/key.rs`; `skald-providers/src/clients/{openai,anthropic,google,vertex}.rs`; each owner matches the typed enum, and RawV1 dispatch uses its embedded provider | PASS |
| In-process gateway projection | Registered provider identity must reach the matching gateway dialect and response decoder | `wyrd-server/src/components/gateway/workflow.rs:81-131,134-249`; Vertex maps to `IngressDialect::VertexGenerateContent` and `Answer::VertexGenerateContent`, while Gemini maps to its separate ingress/answer pair | PASS |
| Registered Vertex end-to-end path | A registered Vertex Prompt must round-trip and reach the Vertex deployment/path | `wyrd-server/tests/pg_workflow_runs.rs:2206-2421` registers each declarative Prompt, reloads the accepted graph, and asserts the exact dialect-specific path; the Vertex case expects `/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent` | PASS |
| No compatibility or bespoke enforcement machinery | Revision 13 prohibits a migration, legacy alias/reader, new option, setting, dependency, or bespoke check; standing direction rejects nonstandard enforcement machinery | The cumulative diff deletes the shape-inference machinery and adds no compatibility reader, migration, provider registry, source scanner, documentation check, setting, or option for this contract. Existing serde, schema generation, owner tests, and user journeys are the standard mechanisms | PASS |

## Findings

No material provider-contract finding is proposed.

The prior provider-domain gap, `FIND-TASK-004-15`, is closed at its source: the approved specification's three canonical Prompt snippets now use `provider: open_ai_chat_completion` and put the unchanged OpenAI request under `body`. The correction reuses the executable examples and adds no compatibility or checking machinery.

The mismatch test is intentionally not a Cartesian product of every provider body. That is not a verification gap: adjacent tagging ensures serde attempts only the named variant, several native provider shapes legitimately overlap, and forward-compatible provider DTOs intentionally accept some unmodeled fields. Requiring mutually exclusive native JSON schemas or a second validator would conflict with the approved mechanism and with common tagged-union practice.

## Verification evidence and limits

Recorded implementation evidence reports green results for the focused Revision 13 request tests (`provider_requests_roundtrip`, `vertex_request_reads_back_as_vertex`, `raw_v1_request_reads_back_when_body_precedes_tag`, and `mismatched_body_is_refused`), the Skald family, Wyrd/server family, Python unit and integration paths, TypeScript unit and integration paths, Workflow-loading journeys, S5 `server_routes_keep_gateway_and_external_ownership`, and `codegen:check`. The R2 remediation additionally records format, lint, Wyrd family, Bifrost Oracle journey, and code-generation evidence after its behavior-neutral source and documentation corrections.

Those results were assessed against their current test bodies and the candidate source but were not rerun under the strict read-only direction. Consequently this report establishes source and recorded-evidence conformance, not a fresh executable result. No source contradiction or missing required provider journey was found.

## Overall result

**PASS**

The candidate satisfies Revision 13's provider request contract across authoring, wire serialization, persistence, generated schemas, SDK-facing serialization, provider dispatch, and registered in-process Vertex execution. No provider-contract remediation is required.
