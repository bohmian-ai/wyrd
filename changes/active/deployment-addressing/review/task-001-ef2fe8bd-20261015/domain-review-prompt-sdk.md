# Prompt, provider, and SDK domain review

Subject: immutable `/tmp/wyrd-task-review-ef2fe8bd`, candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`, base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. Source locations below are snapshot-relative. This review makes no source edits and runs no builds/tests. Concurrent live branch movement is excluded by the subject contract.

## Boundary and authority coverage

| Boundary | Governing authority | Source inspected / result |
|---|---|---|
| Authored Prompt → compiled request → persisted spec/hash | Spec r5 REQ-005–008, INV-004/007, AC-004/006; design Prompt; AGENTS contracts | `skald-spec/src/{authoring,prompt,request,media}.rs`, `wyrd-spec/src/card/prompt/{spec,validate,hash,mod}.rs`; two forms, pair/default and settings mappings present; validation gaps below |
| Native formats and options | REQ-007/010; native provider ownership | All five generation formats in `AuthoredPrompt::native_request`, native wire owners Chat/Responses/Messages/Google/Jev, provider Google client, builder/coerce/finalize; typed top-level settings preserve extension maps for Chat/Responses/Messages/Google; Jev explicitly rejects unknown fields |
| Agent Card → reusable Agent → model/tool loop | REQ-009–011, INV-005/007, AC-005/007; design Agent | `wyrd-spec/src/card/agent.rs`, `skald-agent/src/{agent,gateway,loop_runtime,python}.rs`; durable required route is present, but reusable runtime drops it |
| Public Agent gateway request → real ingress | REQ-001/010/011, AC-005/008 | Shared `observe/invoke.rs`, `gateway_caller.rs`; server `gateway/{caller,ingress,routes}.rs`; OpenAI addressed projection works, other projections have defects below |
| Rust/Python/TypeScript projections | AGENTS §§2/7/8/9; doctrine Public surfaces; reference PyO3, Python/stubs, TypeScript, errors | Rust SDK `src/lib.rs`; Python `src/observe/mod.rs`, agent package/stub and native Agent loader; TS native `cards.rs`, wrapper `index.ts`, generated `card-types.ts`, NAPI declarations. Observe route overrides delegate to the same Rust owner; native Python Agent route support is missing with its owning runtime |
| HTTP/CLI/MCP authoring and generated schemas | REQ-008/011, AC-009; agent-harness reference | PromptSpec serde/JsonSchema, AgentSpec route schema, server Cards resolve/service, CLI card/registration, server MCP cards catalog: these project existing Card owners rather than providing a separate compiler. No new hosted Agent execution API is justified. Served OpenAPI/runtime MCP execution was not run |
| Verification sources | AGENTS §11; testing/task review contract | Pure authoring tests, native Prompt roundtrips, Agent integration owners, shared gateway-caller integration source, Rust gateway-inference journey source, Python/TS gateway-inference projection. Existing caller mocks still expect the old single-segment Gemini URL, so their success cannot prove current router parity |

Also consulted repository AGENTS, agent-rules, reference router, spec-driven-development, maintainer-style, architecture constraints/patterns, Rust core, PyO3 boundaries, Python API/stubs, TypeScript guide, agent-harness, errors, relevant active design/doctrine passages, approved spec and original task. Stronger current design inline-Prompt materialization governs registration; no objection is made to that approved integration seam or approved adjacent edits.

## Findings

### PROMPT-001 — INCORRECT — reusable Agent discards the declared gateway route

**Location:** `crates/skald/skald-agent/src/agent.rs:611` and `:708`; sibling Python Card constructor `crates/wyrd/wyrd-cards/src/agent.rs:132`.

**Obligation:** REQ-009 requires AgentSpec, AgentCard, runtime and SDK projections to retain/honor execution_route; REQ-010 and INV-007 forbid hidden native fallback. Design Agent requires every model call of the bounded loop to honor the declared route.

**Reachability:** AgentCard deserializes a valid `wyrd_gateway` route → `Agent::from_card` resolves Prompt/tools/config but never reads `card.spec.execution_route` → Agent has no route field and `provider_override` remains None → `run`/`run_with` uses the native default registry. `to_spec` unconditionally writes `ExecutionRoute::Native {}`, so `from_yaml_str`/`from_wire`/Python `from_yaml`/`model_validate_json` followed by save/to_card also changes the durable route. Python's Agent constructor and AgentCard constructor expose no route argument and author Native automatically. Shared `Run::invoke_on` separately installs a registry and is therefore insufficient to fix the reusable runtime/tool-loop path.

**Consequence:** Loading a gateway Agent can call host-native credentials instead of its governed deployment, or fail for missing native configuration; roundtrip changes its route/hash. The complete reusable tool loop cannot express the approved route/override contract.

**Smallest correction:** Retain declared route in the existing Agent owner, preserve it through Card/load/save/projection, and resolve declared/effective invocation route through its existing provider-override seam with host-supplied gateway dependencies. Refuse a gateway route with missing dependencies; never silently use native. Update affected Python constructor/stub projections at their permitted SDK boundary. Keep explicit invocation overrides isolated and keep server-native refusal intact.

**Focused proof:** Load a gateway Agent through existing `from_card`/Python public load, roundtrip and compare route; invoke with controlled native and gateway fixtures and assert only gateway receives every supported tool-loop iteration. Missing gateway caller must fail before native IO. Retain native execution and invocation-local override isolation checks.

### PROMPT-002 — INCORRECT — public Agent gateway caller does not speak the new named native ingress contract

**Location:** `crates/shared/wyrd-client/src/gateway_caller.rs:270–294`, `Ingress`/decode in same file.

**Obligation:** REQ-010/011, AC-005/008 require adapter-based named Anthropic/Google/Jev routing and local/server parity; names must not imply adapters.

**Reachability:** Rust/Python/TS `Run.invoke` → `Run::invoke_on` → `wyrd_gateway_registry` retains the named deployment → `NativeCall::project` loses it. Anthropic writes only `model.model` into the body, whereas server `ingress.rs:397` passes that field to `native_model` and requires `<deployment>/<model>`. Google constructs `/v1beta/models/<model>:generateContent`, whereas server `ingress.rs:519,565` mounts/extracts deployment plus target. Jev has no projection/Ingress/decode arm and takes the generic unsupported branch even though `/v1/systemone` exists. The Google guard also rejects a perfectly valid Gemini deployment named `vertex`, conflating deployment name with provider. `Run::invoke_on` already owns the local Vertex-Prompt refusal independently of deployment name.

**Consequence:** Approved non-OpenAI Agent gateway routes fail (Anthropic invalid-model, Google route mismatch, Jev unsupported), even though direct HTTP native journey calls can pass. With a slash-containing Anthropic model, the dropped intended deployment can instead make a model prefix look like a different deployment.

**Smallest correction:** Project the full addressed target for Anthropic/Jev and include the named deployment in the Google path; add Jev response/error projection through existing native types. Remove deployment-name-based provider inference, retaining the existing Prompt-provider-based local Vertex refusal. Preserve response normalization, auth, correlation, cancellation, redaction, and shared transport.

**Focused proof:** Extend the existing shared caller contract and real SDK Agent gateway journey with non-provider deployment names for Anthropic, Google and Jev; include a Gemini deployment named `vertex` and slash-containing model. Assert exact server ingress/upstream target and response. Existing mocks at `tests/integration/gateway_caller.rs` use the old Google path and must be aligned to the actual router, not treated as proof of parity.

### PROMPT-003 — MISSING — portable media bindings lose their declared image/document kind

**Location:** `crates/skald/skald-spec/src/authoring.rs:413–426`; consumer `prompt.rs:346`.

**Obligation:** REQ-006 explicitly requires typed MediaRef kind validation against its portable block; AC-006 requires mismatched-media rejection.

**Reachability:** A portable `{type:image, source:"${media:x}"}` lowers to a plain placeholder; the computed `MediaKind::Image` is discarded for Binding (the comment explicitly acknowledges this). Compiled Prompt retains only name strings. `bind_media_mut("x", MediaRef::document_base64(...))` chooses the document mapping from the supplied MediaRef and replaces that image slot with a file/document. The reverse mismatch and one name used under conflicting block kinds have the same cause. Native placeholder behavior has no declared portable block kind and must remain usable.

**Consequence:** Binding silently changes the authored content kind and dispatches a different provider request than the Prompt declares.

**Smallest correction:** Preserve the portable binding's expected kind as derived runtime metadata on the existing Prompt owner, and enforce it in its shared binder before mutation. Do not store extra authored fields or invent another media resolver. Reject conflicting declarations for one binding name.

**Focused proof:** Extend pure compiler/binder coverage across OpenAI, Anthropic, Google/Vertex: image↔document mismatches fail before changing request; matching binds preserve order; conflicting same-name blocks fail; native bindings retain existing behavior. Add one public Python binding regression exercising the shared owner.

### PROMPT-004 — MISSING — schema objects are mapped without validating schema validity

**Location:** `crates/skald/skald-spec/src/authoring.rs:360,518–526,608–612,667–668`; `native_request:304–322`.

**Obligation:** REQ-006 requires schema mapping/refusal of unsupported schema features; REQ-008 requires registration/loading schema validation; AC-006 requires invalid-schema rejection before provider IO.

**Reachability:** `response_format` only decodes as a JSON object map. For example `{"type":"not-a-json-schema-type"}` is cloned directly into each protocol's output field. `Prompt::new` derives `ResponseType` without validation; `PromptSpec::validate` checks only extracted variable names. Native schemas likewise decode as Map/Value. HTTP/SDK Card parsing consequently accepts and persists the invalid schema, and dispatch serializes it upstream. The Agent's `parse_structured_output` only parses a JSON object, so it is not a registration/schema validator either. Builder `schema_object` also only tests object shape and `finalize_prompt` bypasses authored compilation.

**Consequence:** Invalid or locally detectable unsupported output contracts register successfully and fail only after provider IO, contrary to the promised authoring contract.

**Smallest correction:** Add shared pure validation at the authored compiler/output-schema owner and reuse it from existing builder finalization rather than maintaining separate acceptance rules. Reject invalid schema syntax and destination-unsupported schema constructs with field-specific structured errors, preserving valid schema bytes and deterministic name. No network schema discovery or model catalog is needed.

**Focused proof:** Pure compiler tests for each output field with invalid `type`, malformed `required`/`properties`, supported nested schema, and a destination-unsupported feature; public Card registration negative test proves rejection before persistence/provider call. Include builder parity at the same validation owner.

### PROMPT-005 — INCORRECT — authored model placeholders can choose another native inference target during rendering

**Location:** `crates/skald/skald-spec/src/authoring.rs:214–225`; downstream `prompt.rs:316–328,382–415`.

**Obligation:** REQ-005 says provider/model are literal configuration, never template variables, and rendering cannot select another inference target; REQ-008 requires identifier/placeholder validation.

**Reachability:** Compiler checks model only for non-whitespace. An OpenAI/Anthropic native or portable Prompt with `model:"{{target}}"` compiles and inserts that string into request.model. `Prompt::new` scans all request strings and derives `target` as a variable. `render` calls `bind_mut`, whose recursive string replacement rewrites request.model, so direct native Prompt/Agent dispatch receives the rendered target. Stored Prompt model/hash remains the placeholder. Google has no body model and therefore behaves differently rather than enforcing the common literal contract.

**Consequence:** Content rendering changes target identity, violating stable Prompt/model intent and making native versus gateway execution inconsistent.

**Smallest correction:** Reject placeholder grammar in authored model at the shared identity/compiler boundary, before request construction; apply the same rule to runtime builder authoring without forbidding valid slash-containing literal identifiers. Preserve rendering of actual content/options.

**Focused proof:** Both forms and each model-in-body provider reject `{{target}}` and `${target}` in model while accepting literal slash models; one native render test asserts the model remains identical to the authored literal.

### PROMPT-006 — MISSING — portable literal media is not validated as bounded base64 or provider-compatible MIME/source

**Location:** `crates/skald/skald-spec/src/authoring.rs:227–228,428–436`; shared sinks `prompt.rs:940–1130`.

**Obligation:** REQ-006 requires valid bounded base64 and rejection of detectable unsupported MIME/source combinations; AC-006 requires no provider IO on these refusals.

**Reachability:** Literal portable media becomes MediaRef and calls the existing binder. OpenAI/Google Base64 branches check only nonempty MIME; they never decode/check payload size. Anthropic copies MIME/data without even that check. Thus an image with `mime_type:"application/pdf",data:"%%%"` compiles into provider image/inline blocks (OpenAI builds a data URL). Google File branch requires MIME but copies arbitrary `uri` unchanged, bypassing URL branch's accepted URI restrictions. MediaRef/MediaSource are plain serde data with no hidden validation. These are new portable authoring entrypoints using insufficient existing helpers, not a request to audit unrelated old media features.

**Consequence:** Invalid media registers and reaches provider IO instead of failing at authoring/binding; source variants produce inconsistent safety/validity acceptance.

**Smallest correction:** Enforce existing provider media constraints in the shared pure media/binder owner for both literal compile and runtime binding. Validate base64 and the repository's existing size budget, MIME/kind compatibility and accepted Google File URI forms. Reuse current media rules and limits; do not fetch/reupload/OCR or introduce a second resolver.

**Focused proof:** Pure portable compiler/binder table for malformed/oversized base64, wrong/empty MIME, unsupported File URI, plus valid image/PDF sources across supported providers; public authoring negative coverage proves refusal without provider IO.

## Failure-path assessment

| Path | Source result | Evidence limitation |
|---|---|---|
| Omitted/mixed/removed authored fields, invalid built-in provider-format pair | Explicit serde/compiler refusals | Static source only |
| Unsupported portable setting, Anthropic missing token budget, nonfinite/range-safe fields | Explicit mapping/refusal in shared authoring owner | Existing pure test source covers representative cases, not proof of every public path |
| Literal media compile and typed binding mismatch | FAIL PROMPT-003/006 | No repro run |
| Invalid schema at registration | FAIL PROMPT-004 | Source proves object-only validation |
| Loaded Agent route, native credential fallback and roundtrip | FAIL PROMPT-001 | No fixture executed |
| Named public Anthropic/Google/Jev Agent call | FAIL PROMPT-002 | Direct HTTP journey is a different path |
| Timeout/cancellation, tools and structured output | Existing bounded Agent loop remains owner; gateway caller races timeout/cancellation and normalizes errors | Route loss prevents treating existing native loop tests as gateway route proof; only static review |
| Per-invocation shared observe override/concurrent isolation | Route parameter is invocation-local and shared SDKs delegate to Rust; no Card mutation in invoke_on | Runtime execution not rerun |
| Local Vertex gateway rejection | Shared invoke checks Prompt provider before IO | Direct caller still contains invalid deployment-name guard (PROMPT-002) |

## Verdict

**FAIL.** PROMPT-001 through PROMPT-006 are source-backed proposed findings requiring independent validation. Source review establishes the two-form/shared-projection direction but does not establish task closure.

Skipped test/build execution as required by discovery; no live providers, generated regeneration, served OpenAPI, runtime MCP catalog, or full gate was run. SQL, credential custody, and authorization internals belong to the other assigned domain reviewers; the concrete native-fallback consequence above is within this route boundary.
