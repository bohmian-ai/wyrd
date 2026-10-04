# Provider request contract domain review

## Subject and reviewed boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md`,
  `TASK-004-R2-align-revision-and-source-contracts.md`, and
  `TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Domain: provider-tagged `ProviderRequest` authoring/deserialization, Prompt
  transformation and persistence, generated contracts, SDK-facing fixtures,
  runtime/cache dispatch, native provider sends, and registered Vertex
  execution through the in-process gateway.

`HEAD` resolved to the requested candidate before this report was written. I
reviewed the complete base-to-candidate diff and candidate source. Per the
standing instruction, I ran no build, test, Cargo, mise, package-manager, or
code-generation command.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Adjacent-tagged wire contract | Revision 13 requires `#[serde(tag = "provider", content = "body", rename_all = "snake_case")]` with derived `Deserialize`. `crates/skald/skald-spec/src/request.rs:18-59` implements that exact standard serde representation. The cumulative diff deletes the hand-written shape-ordered deserializer, its helper wire structs, and manual equality implementation. | PASS |
| Named-provider mismatch refusal | The provider tag must select one variant without fallback. Derived adjacent tagging at `request.rs:24-27` does so, while `request.rs:307-337` records structurally invalid cross-provider bodies as errors. No secondary shape inference or compatibility parser remains. | PASS |
| Gemini/Vertex identity | Identical GenerateContent bodies must retain authored identity. `request.rs:90-103,278-288`, `skald-spec/src/authoring.rs:232-267`, and `skald-prompt/src/builder.rs:238-242,270-307` keep Gemini and Vertex in distinct variants from construction through round-trip. | PASS |
| Embedded provider identity | Revision 13 keeps `ProviderName` inside `OpenAiChatCompatible` and `RawV1` bodies. `request.rs:31-37,52-58,90-103,290-305` preserves and dispatches those values; the generated schema exposes the corresponding nested body members. | PASS |
| Prompt binding/rendering | Provider identity must survive variable, media, settings, and tool transformations. `skald-spec/src/prompt.rs:136-280,306-428` matches Gemini and Vertex separately and has `bind_mut` serialize and deserialize the complete tagged enum, so it cannot re-infer a variant from the native body. | PASS |
| Prompt hashing and persistence | `wyrd-spec/src/card/prompt/hash.rs:9-41` hashes the tagged request. `wyrd-spec/src/card/prompt/{spec,codec}.rs`, `wyrd-cards/src/prompt.rs`, and `wyrd-cards/src/prompt/io.rs` serialize and deserialize the native Prompt without provider inference. The distinct supported declarative `PromptDraft` input compiles to a native tagged variant before persistence; it is not an untagged `ProviderRequest` compatibility reader. | PASS |
| Loader, registry, and hydration | `wyrd-loader/src/parse.rs:196-224` decodes the typed Card spec, and `wyrd-client/src/cards/hydrate/workflow.rs:27-165,221-235` carries stored `Spec::Prompt` bodies without rewriting the provider. Server graph resolution therefore receives the persisted tagged variant. | PASS |
| Native fixtures and examples | The changed native Prompt YAML under `examples/workflows/code-review`, `tests/fixtures/workflow-loading`, CLI fixtures, server fixtures, and Python gateway/state fixtures uses `request.provider` plus `request.body`. The three canonical specification snippets now match the executable examples, closing prior `FIND-TASK-004-15` without an alias or check. Declarative SDK fixtures using top-level `provider`, `model`, and `messages` remain the approved `PromptDraft` surface rather than the removed native request form. | PASS |
| Generated schemas and public projections | `crates/wyrd-spec/schemas/prompt_spec.json:3899-4132` describes a `oneOf` of adjacent-tagged variants, including disjoint Gemini and Vertex tags and nested provider identity for OpenAI-compatible and RawV1. The other changed schema/golden embeddings carry the same definition. Python assertions read the new `body` envelope and round-trip Vertex; Rust and TypeScript declarative paths converge through the same native owner. No compatibility alias or alternate public native shape was found. | PASS |
| Cache and runtime dispatch | `skald-runtime/src/dispatch.rs:22-89` selects the provider through `ProviderRequest::provider`; `skald-cache/src/key.rs:58-91` scopes typed cache keys by that identity and refuses opaque RawV1 cache derivation. Neither owner guesses from body shape. | PASS |
| Native provider send sites | `skald-providers/src/clients/openai.rs:452-513`, `anthropic.rs:108-143`, `google.rs:112-157`, and `vertex.rs:105-153` accept only their typed variants (plus RawV1 after registry selection) and send only the unwrapped native body. RawV1's approved `serde_json::Value` is converted to `RawValue` at those send boundaries and passed through the existing `raw.rs:10-31` transport. | PASS |
| In-process gateway projection | `wyrd-server/src/components/gateway/workflow.rs:81-131,134-249` maps each supported typed request to its own ingress and response decoder. Vertex maps to `IngressDialect::VertexGenerateContent` and `ProviderResponse::VertexGenerateContent`; Gemini remains separate. Unsupported variants fail before gateway admission. | PASS |
| Registered Vertex journey | `wyrd-server/tests/pg_workflow_runs.rs:2208-2226,2324-2420` registers and reloads provider-specific Prompts, executes them through the accepted server graph, and checks exact upstream paths. The Vertex case requires `/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro:generateContent`, proving the saved Prompt did not return as Gemini. | PASS |
| No drift machinery | The implementation uses serde's established tagged-enum mechanism, existing schema generation, existing Prompt/Card/loader owners, existing provider dispatch, and the existing Workflow journey. The cumulative provider-contract diff adds no migration, legacy reader, source scanner, documentation check, setting, option, dependency, registry, or parallel validation layer. This conforms to the direction to reject mechanisms absent from established and common project practice. | PASS |

## Material findings

No material provider-contract finding is proposed.

The mismatch test is deliberately not a Cartesian product of every provider
body. That is not a gap: serde adjacent tagging attempts only the named variant,
some provider-native JSON bodies legitimately overlap, and provider DTOs retain
forward-compatible fields. A second validator or mutually exclusive body
schema would duplicate the standard tagged-union mechanism and would be drift.

The R3 implementation changes only the imported name used by the existing
`raw_body` test helper within this boundary; it does not alter provider wire,
persistence, projection, or send behavior. No prior provider finding reopened.

## Verification evidence and limits

The recorded R1 evidence reports passing focused request tests for provider
round-trip, Vertex retention, RawV1 key-order tolerance, and mismatch refusal;
the Skald family; code generation; Rust/Python/TypeScript unit and integration
paths; Workflow loading; and S5
`server_routes_keep_gateway_and_external_ownership`. That journey's current
body still asserts OpenAI Chat, OpenAI Responses, Anthropic Messages, Gemini,
and Vertex paths, pre-upstream capability refusal, fallback isolation,
deadline, and cancellation behavior. R2 records code-generation and Wyrd
family evidence after its example correction, and R3 records the Wyrd family
after its behavior-neutral import correction.

Those results were inspected as recorded evidence and were not rerun under the
strict read-only direction. This report therefore establishes source and
recorded-evidence conformance, not a fresh executable result. No source
contradiction, missing required provider path, or nonstandard enforcement
mechanism was found.

## Overall result

**PASS**

The candidate satisfies Revision 13's provider request contract from authoring
and deserialization through persistence, SDK-facing representation, runtime
dispatch, native sends, and registered in-process Vertex execution. No
provider-contract remediation is required.
