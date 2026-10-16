# Provider wire and Agent boundary review

Result: **FAIL** — one new builder regression and one production integration gap proposed for independent validation.

## Immutable subject and scope

- Reviewed checkout: `/tmp/wyrd-task005-review-de69cb69b`.
- Base: `f2a726c90edbf8956c7b91c843379175ee20eac0`; candidate: `de69cb69b68eced72f7d760681d27253e013f8a1`.
- Inspected cumulative provider/Prompt changes and recent `99882cf82...de69cb69b` changes; all source and authority came from the detached checkout. No CodeGraph index exists there. Final HEAD remains the candidate; checkout status is clean.
- Authority: AGENTS.md §§2–3, 7–11; agent-rules; spec-driven-development; maintainer-style; reference router and architecture constraints/patterns; Wyrd design/doctrine; approved verifier-building-blocks r9 REQ-007/008 and AC-003; original TASK-005; prior R1 and supplemental TASK-005-S1 and its findings. The draft agent-runtime-composition spec was not treated as approved authority.

## Boundary coverage

| Boundary | Source and consumer evidence | Assessment |
|---|---|---|
| Portable reasoning effort | `skald-spec/src/authoring.rs:185–225, 707, 732–737, 809–814`; existing native OpenAI effort enum; TS generated `card-types.ts:3419–3437,3998–4000` | Chat lowers the enum to its existing native field. Anthropic and Google refuse rather than discard it. Native Responses continues using its own `reasoning.effort`; no requirement inferred to add a new portable Responses selector. |
| Gemini response JSON Schema | `authoring.rs:847–850`; `skald-prompt/src/builder.rs:397–420`; `skald-spec/src/prompt.rs:288–298,663–676`; gateway `adapter/google.rs:103–112` | Portable and OpenAI-compatible gateway translation select `response_json_schema`; native `response_schema` remains readable. Schema extraction and invalid-schema field attribution recognize the new field. The provider-shaped builder has the conflict described in PROV-01. |
| Gemini tool JSON Schema | `skald-spec/src/request.rs:237–257,304–323`; wire `google_generate.rs:295–335`; gateway `adapter/google.rs:261–291`; Python `python/google.rs:576–589` | Shared construction emits only `parameters_json_schema`; gateway reuses it. Python's existing parameter getter reads either representation. No new standalone Python ReasoningEffort class/export is needed for the JSON authoring surface. |
| Optional native roles | wire `GoogleContent:41–56`; `authoring.rs:1986–2008`; `convert.rs:337–344`; Agent `request_builder.rs:236–245`; Python `python/google.rs:102–105`; public stub `prompt/__init__.pyi:763–774` | Missing request role becomes the documented empty string, serializes absent, remains a string in Python, and maps to user on cross-provider conversion. Existing Rust struct literals still compile in shape. |
| Authored invalid-input mapping | `skald-prompt/src/prompt.rs:1064–1068,1270–1277,1663–1684`; `error.rs:110–114`; `settings.rs:165–180`; Python authoring unit cases | The changed user-authored serde decode paths now map to PromptInvalid; settings already use their dedicated typed decode error. Schema/compiler validation retains field text. No new general JSON-error reinterpretation was introduced. |
| Unaddressed Gemini ingress | server `components/gateway/ingress.rs:219–235,637–656`; `GatewayIngress::authenticate:114–159`; Python `test_gateway_native_clients.py:215–245` | Bare model route is under the same Gemini authentication layer and returns its protocol's 400 only after normal credential handling. Existing refusal integration case checks no provider delivery. No alternate invoke/authorization path. |
| Native and hosted model continuation | server `verification/hosted/blocks.rs:119–136` → Vala `blocks/agent.rs:285–345` → Skald loop and request_builder → server gateway caller's typed answer decode | Production server composition already supports tools on hosted steps. This is distinct from the unimplemented SDK WyrdClient/MCP composition draft. Signed Gemini continuation metadata is lost, described in PROV-02. |
| Sweep evidence | smoke conftest, both live test modules, findings.md; deterministic Gemini/Agent and gateway tests | Sweep records 42 pass / 1 fail and explicitly records unproven paths; it is not a passing whole-product or whole-task gate. See limits below. |

## Proposed material findings

### PROV-01 — REGRESSION: the Gemini/Vertex builder produces both mutually exclusive output-schema fields

**Location:** `crates/skald/skald-prompt/src/builder.rs:401–412`, particularly the new assignment at line 412.

**Obligation:** preserve existing provider-shaped Prompt behavior while fixing schema transport; Prompt owns inference settings/output schema (approved REQ-007), and changed public authoring paths must produce usable provider requests.

**Producer → consumer:** Public `Prompt.gemini` and `Prompt.vertex` accept `GeminiSettings(generation_config=...)` and `response_format`/`output`; their Rust siblings use `GeminiOptions`. `google_settings` clones the supplied config, then adds `response_json_schema` without clearing `response_schema`. For example, an existing config containing `response_schema: {"type":"object"}` plus a JSON-schema `response_format` now emits both fields. Before this change the effective ResponseFormat replaced the existing `response_schema` in this same function. `finalize_prompt` and shared `response_type_of` accept the new request (the latter simply prefers the new field). Both native Google/Vertex transport and the hosted gateway path send the resulting request. The provider refuses the incompatible fields instead of generating an answer.

Google documents JSON-schema output as an alternative requiring omission of `responseSchema`; this is a protocol restriction, not a preference. [Google GenerateContent reference](https://ai.google.dev/api/generate-content), GenerationConfig schema-field documentation.

**Smallest correction:** At the existing builder owner, when effective JSON-schema ResponseFormat replaces the output schema, clear the old mutually exclusive `response_schema` before assigning `response_json_schema`. Preserve native-only settings and all other generation fields. No new validation framework or provider adapter is required.

**Focused closure proof:** Exercise both Gemini and Vertex builders with a settings-supplied `response_schema` plus an effective ResponseFormat/output schema. Assert that the effective schema survives, `response_schema` is absent from serialized native request, and other supplied settings survive. This is an IO-free builder regression; no credential or live call is needed.

### PROV-02 — MISSING: the newly composed hosted Gemini tool loop cannot preserve required thought signatures

**Location:** changed integration owner `crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs:119–136`; shared source of invalid continuation state `crates/skald/skald-spec/src/wire/google_generate.rs:121–125,170–178`; replay consumers `crates/skald/skald-agent/src/request_builder.rs:166–177,236–245`.

**Obligation:** approved REQ-007/008 and AC-003 require hosted step Agents to use the existing provider/Prompt loop and built-in tool harness. A built-in tool call must be able to return its result to the selected model. This does not require SDK WyrdClient composition, MCP tools, or any other draft feature.

**Producer → consumer:** HostedBlocks now composes AgentExecutor with HostedTools and ServerWyrdGatewayCaller. A Google/Vertex Agent step can invoke a tool through this production path. On the response, ServerWyrdGatewayCaller decodes buffered JSON into the typed ProviderResponse. GooglePart's function-call variant holds only GoogleFunctionCall, whose stored fields are `name` and `args`; neither the part nor the call can retain `thoughtSignature`/`thought_signature`. Serde's permissive decode discards it. `assistant_message` copies that already-lossy content into the conversation; `rebuild_request_messages` then sends it back alongside the tool result on the next iteration. There is no later raw-response recovery path. Native-route Agents use the same lossy typed response. Native SDK-through-gateway raw relay does preserve bytes, so its one-call pass does not establish this typed loop's correctness.

For Gemini models that require the signature on the first functionCall part, the second request is invalid and the step fails after the tool ran. Google explicitly documents this 400 behavior and the requirement to replay the signed part. [Google thought-signature protocol](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures).

This wire limitation existed at the base, but the finding is the **new task-owned hosted built-in-tool composition relying on it**, not a demand to fix unrelated native Agent debt. The checked-in GEM-2 row calls the path unconfirmed; source establishes the metadata loss and the approved production consumer. A claimed live pass with one default model cannot prove preservation for a signed response because no assertion inspects the replayed signature.

**Smallest correction:** Extend the existing shared Gemini content representation so native response continuation signatures survive decode → conversation → request serialization, preserving the provider's part ordering. Use the current assistant-message/rebuild path; do not add another gateway caller, raw history store, runtime-composition feature, or duplicate loop. The same existing wire owner serves both hosted and native consumers.

**Focused closure proof:** Script a Gemini response containing a function call and a nonempty signature; run one existing tool loop through the typed response path and assert the second upstream request contains the original signature on the original function-call part plus the tool result. Cover the current provider spellings where decoded. A deterministic mock upstream/fixture is sufficient; no new live-provider run is needed to prove byte preservation. Keep native gateway relay tests unchanged.

## Prior claims and verification limits

- PRM-1/2/3 and GW-1 have coherent source-level corrections for their reported ordinary cases. PROV-01 is a sibling builder consequence of PRM-3's migration, not a claim that the portable path still emits the old schema field.
- S1 intentionally keeps failing cases failing and records attribution. Its JEV-1 failure is not silently considered solved; the existing Agent loop rejects SystemOne before dispatch. The draft agent-runtime-composition spec is not a reason to demand SystemOne dispatch in this TASK-005 review.
- OBS-1 remains a recorded fixed-message provider-error limitation. No new provider-message-disclosure requirement is invented here.
- Part 1 forces and decodes one function call; it does not return the tool result through the typed Wyrd Agent path. Part 2's local-tool completion proves a live case finished, but does not assert signed continuation preservation. Native Prompt tool/structured/stream variants and expected provider rejections are expressly unproven. Those omissions alone are not new requested features.
- No credentials, live calls, broad gates, builds, or tests were run by this reviewer. Findings are source-backed proposals and the 42/1 result is recorded evidence, not a fresh run. Primary Google documentation was consulted only for protocol restrictions.

Skipped live/provider execution and unrelated draft features. Risk: signed Gemini tool continuations and mixed builder schema settings remain unsupported by the candidate paths described above.
