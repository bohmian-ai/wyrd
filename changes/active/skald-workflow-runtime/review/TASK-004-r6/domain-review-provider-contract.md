# Provider/request contract domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Cumulative remediations: TASK-004 R1 through R5
- Review mode: source and recorded-evidence inspection only; no build, test, Cargo, `mise`, or code-edit command was run.

The candidate commit resolved to the requested identity at the start and end of this review. The fixed human decisions were treated as authority, including one request variant per wire schema, `Prompt.provider` as the optional native dispatch destination, Vertex GenerateContent as a Google body targeting `vertex`, gateway-owned upstream selection, and the unchanged local `wyrd-client` Vertex refusal.

## Domain and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Request schema identity | Revision 14 lines 21-54; `crates/skald/skald-spec/src/request.rs:17-98`; generated Prompt schemas | PASS. `ProviderRequest` is adjacent-tagged by schema; the compatible Chat and Vertex GenerateContent variants and wrappers are gone; `VertexPredict` and `RawV1` remain distinct. |
| Prompt destination | Revision 14 lines 31-38; `skald-spec/src/prompt.rs:28-53,142-173`; `skald-spec/src/authoring.rs:70-115`; `skald-prompt/src/builder.rs`; `skald-prompt/src/prompt.rs` | PASS. The optional destination is omitted when absent, the effective provider falls back to the dialect default, declarative and ergonomic Vertex/custom authoring set the destination, and ordinary Prompt transformations preserve the field. |
| Durable identity and hydration | `wyrd-spec/src/card/prompt/hash.rs:9-45`; Prompt Card codec/IO; loader parsing; `wyrd-client/src/cards/hydrate/workflow.rs`; registered Workflow graph consumers | PASS. Serialization carries the destination, the Prompt content hash includes it, and graph hydration moves the typed Prompt without inference. |
| Native and Agent dispatch | `skald-runtime/src/dispatch.rs:14-53`; `skald-agent/src/loop_runtime.rs:126-143,205-229,331-335`; `skald-workflow/src/workflow.rs:545-587`; native provider clients | PASS. Agent and Workflow execution use the effective `Prompt::provider`; a Vertex GenerateContent body reaches the Vertex client and a custom Chat body reaches its registered custom client. |
| External and Wyrd gateway routing | `skald-workflow/src/route.rs:394-481`; `skald-providers/src/clients/external.rs:95-136`; `wyrd-server/src/components/gateway/workflow.rs:177-234`; `wyrd-client/src/workflow/gateway.rs:230-274` | PASS. Both external GenerateContent protocols accept the shared body, external Chat accepts the sole Chat body, the server selects Vertex ingress from the model identity, and the local public gateway retains its prior pre-IO Vertex refusal. |
| Python and generated projection | `skald-prompt/src/prompt.rs:907-935,1070-1156`; `skald-prompt/src/python/{google,response,mod}.rs`; both generated `prompt.pyi` files; public Python Prompt tests | PASS. `Prompt.provider` exposes the dispatch destination, `ProviderRequest.provider` exposes the dialect default, the Vertex request/response wrappers are removed, `Prompt.vertex` remains, and stubs match those shapes. |
| Provider-sensitive Prompt transformations | `skald-spec/src/prompt.rs:232-269,747-767,910-948`; Python media binding tests | **FAIL — `DOMAIN-PROVIDER-R6-1`.** Media binding still hard-codes Google where the folded GenerateContent body may target Vertex. |
| Provider-scoped cache identity | `skald-cache/src/key.rs:14-23,58-105`; public crate exports; base implementation of the removed compatible variant | **FAIL — `DOMAIN-PROVIDER-R6-2`.** Cache identity now loses the custom Prompt destination and scopes every Chat request as OpenAI. |
| Standing DRIFT direction | Complete provider-related diff, new files, settings, generated files, and checks | PASS. Revision 14 deletes duplicate schema/accessor machinery and reuses standard serde, Prompt, provider registry, route, gateway, schema, and stub owners. No novel checker, option, compatibility reader, registry, adapter, or fixture system was found in this domain. |

## Material findings

### DOMAIN-PROVIDER-R6-1 — REGRESSION: Vertex media rejection is attributed to Google

- **Violated obligation:** Revision 14 makes `Prompt.provider` the native dispatch destination and requires a Vertex Prompt to remain a Google GenerateContent body targeting Vertex. Existing provider-specific Prompt behavior must therefore consult the effective Prompt destination when that behavior reports the provider.
- **Location:** `crates/skald/skald-spec/src/prompt.rs:244-258`, especially the unconditional `ProviderName::Google` at line 257; the resulting provider-bearing error is constructed at `prompt.rs:921-935`.
- **Evidence:** Before the fold, the `ProviderRequest::Vertex` branch called the same existing `bind_media_google` owner with `ProviderName::Vertex`. The candidate deleted that branch but left the surviving GenerateContent arm hard-coded to Google. A Vertex Prompt created by `PromptDraft`, `builder::vertex`, or Python `Prompt.vertex` has `prompt.provider == Some(Vertex)` and `request == GeminiGenerateContent`. Binding an unsupported HTTPS URL reaches `build_google_part`, which now returns `UnsupportedMediaForProvider { provider: Google, ... }` even though native dispatch targets Vertex. The current media test covers only successful Vertex base64 replacement (`test_prompt_media_binding.py:30-47`) and does not exercise the provider-bearing refusal.
- **Observable consequence:** Rust and Python callers receive a stable structured error that identifies the wrong provider for a valid Vertex Prompt. This is a direct regression from the pre-fold Vertex branch and makes the destination projection inconsistent across authoring, dispatch, and error surfaces.
- **Required correction:** In the existing `Prompt::bind_media_mut` owner, derive the effective destination from the Prompt before mutably borrowing the request, and pass that destination to the existing `bind_media_google` helper for the shared GenerateContent body. Do not restore a Vertex request variant, add a second media implementation, or special-case Python.
- **Focused closure proof:** Add a focused Rust-native Prompt test, projected through the existing Python media test if desired, that binds an unsupported URL to a Vertex Prompt and asserts `WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER` identifies `vertex`; keep the existing Gemini rejection proving it still identifies `google` and the successful Gemini/Vertex base64 replacements.

### DOMAIN-PROVIDER-R6-2 — REGRESSION: custom OpenAI-compatible cache keys are scoped as OpenAI

- **Violated obligation:** Revision 14 moves destination identity from the Chat request variant to `Prompt.provider`; `CacheKey` still promises deterministic identity scoped by provider (`crates/skald/skald-cache/src/key.rs:14-22`). Removing the duplicate request variant must not erase that existing provider isolation.
- **Location:** `crates/skald/skald-cache/src/key.rs:58-64,91-105`.
- **Evidence:** At the base, `OpenAiChatCompatible { provider, request }` passed its embedded custom provider into `openai_cache_key`. The candidate removed that arm, and the sole `OpenAiChatCompletion` arm always supplies `ProviderName::OpenAi`. Revision 14 custom authoring now produces that same Chat request plus `Prompt.provider = Some(Custom(...))`, but `CacheKey::from_request` accepts only the request and cannot observe the moved destination. Thus OpenAI and any number of custom endpoints using the same model and `prompt_cache_key` produce equal provider scopes. The crate publicly exports `CacheKey`; although no current workspace production caller invokes `from_request`, this changed public cache primitive is itself the owner of the provider-scoping invariant and was materially modified by Revision 14.
- **Observable consequence:** A caller using the cache primitive for custom OpenAI-compatible Prompts can collide resource identities across distinct providers, or between a custom provider and OpenAI, defeating the type's documented isolation and potentially reusing a provider-owned cache resource under the wrong destination.
- **Required correction:** Move cache-key derivation to the effective Prompt destination using the existing `Prompt::provider` mechanism (or require that effective provider explicitly at the existing cache owner) and use it as the Chat cache scope. Keep request-dialect inspection for extracting native cache directives; do not recreate `OpenAiChatCompatible`, embed destination back into the request, or add another cache or compatibility layer.
- **Focused closure proof:** Extend the existing `skald-cache` key tests with two OpenAI Chat Prompts that share model and `prompt_cache_key` but have different effective destinations, and prove their `CacheKey.provider` values and keys differ; also retain the ordinary OpenAI case. Because no workspace production caller currently uses this API, the proof should stay at the existing cache owner rather than introduce a harness or synthetic caller.

## Verified preservation

- The single Chat and single GenerateContent schema variants are used consistently by serde, native clients, Agent conversation rebuilding, external routes, the in-process Wyrd gateway, and Python typed accessors.
- Custom Chat provider identity survives Prompt serialization, Prompt Card persistence, hashing, Workflow graph hydration, and native registry dispatch. The registered external-route journey proves the route ignores that destination and posts the unchanged Chat body directly.
- Vertex destination identity survives serialization, registered graph hydration, native dispatch, gateway model projection, and the server's in-process Vertex ingress/path. `ProviderResponse::GeminiGenerateContent` is correctly reused.
- The local shared-client Wyrd gateway rejects a GenerateContent body for a Vertex model before IO, matching the approved unchanged prior behavior.
- `RawV1` retains its embedded `ProviderName`; no migration, alias, shape-ordered fallback, or compatibility reader remains.

## Verification evidence and limits

The R5 remediation record reports successful final-tree runs of formatting, lints, `git diff --check`, code generation, client/PyO3 boundary checks, Skald/shared/server/gateway tests, Python format/lint/unit/typecheck/integration, TypeScript unit/integration, and the Oracle journey. It also records the focused custom-provider and Vertex native-dispatch tests plus the registered server route journey. I treated those results as evidence claims and checked their current assertions against candidate source.

Per the review-only instruction, I did not execute any recorded command. The recorded green lanes do not cover the Vertex provider-bearing media refusal or distinct custom-provider cache scopes identified above. No live provider, credential, gateway, registry, or Python runtime was exercised in this review.

## Overall result

**FAIL**

The central Revision 14 wire, persistence, routing, gateway, and Python contracts are present, but two bounded provider-identity regressions remain at existing consumers. Both corrections reuse the effective `Prompt::provider` owner and existing test homes; neither requires a new schema, public product decision, mechanism, setting, option, checker, or harness.
