# Prompt, Agent and SDK domain review

## Subject and result

**FAIL.** Source reviewed is the immutable archive `/tmp/wyrd-task-review-9516afcd`, candidate `9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`, cumulatively from `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`, with the previous-candidate diff used to locate remediation. This is an independent domain discovery report; proposed findings require independent validation. FIND-001-14 retains its human-directed provenance.

Read the approved deployment-addressing r6 spec, original TASK-001, prior R1 correction requirements and appended evidence, AGENTS.md, agent rules, maintainer guide, applicable Prompt/Agent/provider doctrine, and the docsite requirements. The archive has no `.codegraph`; source was located with the installed ripgrep binary. No implementation/source edits or builds were made. Evidence-table PASS entries are implementer-supplied execution evidence, not reruns by this reviewer.

## Coverage and owner map

| Boundary | Source and caller coverage | Assessment |
|---|---|---|
| Authored forms, provider/format, portable content and settings | `skald-spec/src/authoring.rs` AuthoredPrompt compile/native_request/portable_request; PortableMessage::lower; check_format; OpenAI/Anthropic/Google lowering; eight-setting validation | Single shared compiler, native option preservation, canonical provider names, no deployment in authored Prompt; Jev native-only. |
| Derived model/schema/media invariants | `skald-spec/src/prompt.rs` Prompt::new/authored/bind_mut/bind_media_mut/render, response_type_of, check_output_schema/check_schema/check_subschema, check_model, check_media_source, native source mappings; `media.rs` | Kind and encoded-payload corrections exist; partial schema validation and independent typed-request model remain below. |
| Authoring helpers and Python mutation | `skald-prompt/src/builder.rs` all provider builders/finalize_prompt; `coerce.rs` checked_model; `prompt.rs` native constructor/projections, request/model setters and rendered wrappers | Builders/model setter validate literal canonical model; request setter converges on shared constructor. No separate SDK compilation introduced. |
| Retained Agent route and actual dispatch | `skald-agent/src/agent.rs` state/from_card/from_yaml/to_spec/to_card/route_registry, `loop_runtime.rs` run_with/run_prompt/call_once, request_builder, delegate tool's run_with, Python constructor/load/save/run | Route now retained; gateway registry overrides native provider overrides and missing caller refuses before IO. Jev uses one-shot supported path. |
| Public and server gateway callers | `wyrd-client/src/gateway_caller.rs` PublicWyrdGatewayCaller and NativeCall, `observe/invoke.rs`, gateway ingress/routing/adapter initial-target and conversion branches, Vala judge agent_for/invoke | Named non-OpenAI targets corrected and typed provider header enforces initial compatibility. Public Rust Vertex boundary remains below. Server judge native refusal remains separate. |
| Registered four-form preservation | Rust register_and_hydrate authored-form journey; Python integration `test_prompt_forms_round_trip_as_authored`; TS `wyrd/tests/integration/register-and-hydrate.test.ts` | Both forms × standalone/inline compare GET/hydrated JSON and spec hashes; Rust/Python inspect existing derived accessors. TS has no introspection accessor; no new one demanded. |
| Installed CLI bundle lifecycle | Python `test_installed_cli_gateway_setup`, `test_cli_gateway_agents`, subprocess drivers; `examples/gateway-setup/agents` service/Agent/Prompt fixtures; shared WyrdState and Run invocation | Four provider requests, real download, saved auth, restart, correlated retained evidence and upstream-header assertions exist. Public runtime-loader proof is bypassed below. |
| Published Connect guides | `docs/src/content/docs/connect/set-up-the-llm-gateway.md`, `call-wyrd-from-a-stock-client.md`; framework inference tests and SDK source/dependency locks | Source agrees with synchronous current TS connect and framework examples. The documented first evidence query is unreachable without omitted capture setup. Narrow-width visual check remains unperformed in supplied evidence. |

Existing owners were compared before recommending changes: shared AuthoredPrompt/Prompt validation rather than per-SDK/server checks; existing Agent loader and runtime rather than reconstruction inside a test; public caller versus separate server caller for local-only restrictions; existing capture-policy command rather than a new default or publication path. No replacement graph store, loader, transport or framework is required.

## Prior-finding closure in this domain

| Prior item | Source closure assessment |
|---|---|
| FIND-001-2 | Closed for retained state/default execution: Agent::from_card copies execution_route, to_spec emits it, run_with/run_prompt call route_registry before runtime IO, Python roundtrips reach from_card. Missing caller fails before native providers. The combined downloaded-runtime proof belongs to FIND-001-14 below. |
| FIND-001-3 | Anthropic full addressed body, Google addressed URL, adapter-name independence and Jev one-shot projection fixed. Local Vertex restriction is incompletely enforced (P-SDK-02). |
| FIND-001-4 | Closed: portable lowering retains media_kinds, conflicting declarations fail, shared bind_media_mut checks expected kind before source conversion/mutation; native placeholders remain untyped. |
| FIND-001-5 | Partial: shared structural checker now catches selected invalid types/maps/lists, but still accepts malformed schemas (P-SDK-01). |
| FIND-001-6 | Closed in examined call chain: Agent-origin provider propagated to public and server callers, request admission requires compatible initial adapter, direct ingress without header preserves existing conversion and fallbacks remain separately handled. |
| FIND-001-8 | Approved authored, builder and Python setter paths corrected. Public native constructor retains an independently templated body model (P-SDK-03). |
| FIND-001-11 | Closed for original malformed/oversized base64, wrong-kind/provider MIME and arbitrary Google File URI cases: shared binder validates before mutation and literal lowering uses binder; valid native file IDs retained. No new upload/fetch path. |
| FIND-001-13 | Closed for four-form preservation through existing exposed surfaces. TS JSON/hash assertions cover its available contract; adding introspection API would violate the packet's prohibition. |
| FIND-001-14 | Partial, owner-directed: real setup/download/restart/header/evidence coverage is present; Agent loader/run handoff remains bypassed (P-SDK-04). Card-bound 403 exercises a real pre-dispatch refusal; the packet does not require a specifically role-based denial for this added stage. |
| R1-DOC-AC-01/02 | Partial: usable actual request examples and framework inference coverage are present, but first-request retained-evidence instructions omit mandatory existing capture configuration (P-SDK-05). |
| R1-DOC-AC-03 | Published pages/indexes and reported automated light/dark accessibility proof exist. Manual narrow-width inspection explicitly not done; report as a proof limit, not an invented runtime failure. |

Other findings (1/7/9/10/12) belong to their independent specialists; this domain does not claim their closure.

## Proposed findings

### P-SDK-01 — retain FIND-001-5 — Important, MISSING

**Plain statement:** Invalid output schemas still register because the new checker ignores many standard schema keywords.

**Obligation:** REQ-006/007, AC-006, R1-AC-05; invalid/detectably unsupported schemas must fail in shared construction before storage or provider IO.

**Location/evidence:** `crates/skald/skald-spec/src/prompt.rs:235–327`, especially `check_schema` wildcard branch; callers `authoring.rs:225,384–390`, `prompt.rs:398–421`, builders `builder.rs:312–320`.

A portable prompt with provider `google`, model `m`, a user `hi` message and `response_format: {"type":"object","properties":{"answer":{"type":"number","minimum":"zero"}}}` passes the complete checker: `properties` recurses, `type` passes, `minimum` hits `_ => {}`. Equally, `maxLength: -1`, malformed `$ref` or malformed schemas under conditional/dependent-schema keywords escape. This is not an unknown vendor extension: these are schema keywords with invalid shapes. The same hole reaches native authoring and builders through Prompt::new. The invalid JSON is then preserved, registered and sent as requested structured output, leaving the failure to upstream despite the required local refusal. Existing tests cover only the keyword shapes the implementation checks.

**Correction:** finish the existing pure `check_output_schema` validation boundary for standard schema keyword shapes and nested schema locations; reject statically incompatible destination features rather than forwarding them. Keep unknown native options and valid schema bytes unchanged, do not fetch references, and do not add SDK/server copies. This is the same required validity boundary as prior FIND-001-5, not a new feature or schema dialect proposal. Preserve failure atomicity at mutable builder/setter consumers of the shared check.

**Focused proof:** extend `authoring::tests::output_schemas_are_validated` with malformed numeric/string/reference keywords and invalid nested conditional/dependent schemas in portable/native forms, plus existing builder coverage and the real registration refusal test to establish no durable write. Include valid corresponding schemas and exact field errors. No broad journey suite required.

### P-SDK-02 — retain FIND-001-3 local-Vertex remainder — Important, INCORRECT

**Plain statement:** A direct Rust Agent can still send a Vertex gateway call from a local process even though local Vertex gateway execution is explicitly deferred.

**Obligation:** REQ-010/011 non-goal and AC-008 require local Vertex gateway calls/overrides to fail before IO while retaining existing server Vertex execution.

**Producer-to-sink evidence:** A Vertex AuthoredPrompt compiles to `ProviderRequest::GeminiGenerateContent` with provider Vertex (`authoring.rs`). Public Rust `Agent::from_card(...).with_gateway_caller(Arc::new(PublicWyrdGatewayCaller::new(client)))` is the supported local host seam. `Agent::route_registry` (`agent.rs:366–382`) and `wyrd_gateway_registry` (`gateway.rs:91–111`) accept Vertex. `PublicWyrdGatewayCaller::call` (`gateway_caller.rs:108–128`) projects without examining provider and sends `wyrd-prompt-provider: vertex`; `NativeCall::project` (`gateway_caller.rs:286–294`) maps any GenerateContent body to public Gemini ingress. `ingress.rs:642–700` constructs GeminiGenerateContent; `routing.rs:152` accepts Vertex provider on Vertex deployment; `adapter/mod.rs:219–222` explicitly accepts Gemini ingress on a Vertex adapter. Thus this is not merely a dialect rejected downstream: an authorized call to an existing Vertex deployment dispatches through server Vertex, crossing local IO. `Run::invoke_on` has a Vertex guard at `observe/invoke.rs:93–99`, but direct Agent runtime does not go through Run. The public-caller test's deployment called `vertex` carries Google provider, which correctly succeeds and cannot prove Vertex-provider refusal.

**Correction:** enforce the existing local Vertex unsupported rule in PublicWyrdGatewayCaller before transport/native projection. This caller owns the local-versus-server boundary; rejecting Vertex globally in Agent/registry would break the separate supported server judge path. Keep a Gemini adapter deployment arbitrarily named `vertex` working. Preserve server-native Vertex and ordinary direct-client protocol conversion.

**Focused proof:** public caller/Agent test with actual `ProviderName::Vertex` plus attached local public caller, including a route override, must refuse with no gateway/upstream request; retain Google-provider-to-deployment-named-vertex success and existing server Vertex test.

### P-SDK-03 — retain FIND-001-8 native-constructor remainder — Important, INCORRECT

**Plain statement:** The public native Prompt constructor can still let a variable change the request's model because it validates a separate model string.

**Obligation:** REQ-005, R1-AC-08: model identity is literal configuration and rendering cannot choose an inference target.

**Source path:** `Prompt::new` (`prompt.rs:398–421`) checks only its `model` argument and retains `request` unchanged. `extract_text_variables` (`710–727`) scans the complete request. `bind_mut` (`535–543`) and `replace_string_leaves` (`634–650`) substitute every request string, including the wire body's model. `Prompt::render` returns that mutated request; native Agent run_prompt dispatches it through the native provider registry. `Prompt::new(ProviderRequest::OpenAiChatCompletion(request_with_model_{{target}}), "safe")` therefore succeeds, derives `target` and renders its upstream model to supplied `target`. The same applies to typed body model slots on Responses/Anthropic/Jev. This constructor is a public runtime authoring surface used throughout Skald Agent construction tests and `Prompt::from_native`; it is not a hypothetical future caller. Approved YAML/native authored compilation, ergonomic builders and Python model/request setters do not produce this mismatch and are correctly fixed; this is a narrower remaining constructor boundary.

**Correction:** in existing shared Prompt construction, validate the effective native request model before variable derivation, and establish that a model-bearing native request agrees with its canonical literal Prompt model. Reuse existing typed request model handling rather than adding per-provider guards at dispatch or excluding arbitrary fields from content rendering. Preserve model-less GenerateContent and valid slash IDs. No model catalog/alias/renaming is introduced.

**Focused proof:** extend the exact literal-model test with each model-bearing native request variant holding a template while its separate canonical argument is literal; construction refuses before variables/render/provider IO. Test consistent literal models and model-less Google construction. Existing authored/builder tests must remain green.

### P-SDK-04 — retain owner-directed FIND-001-14 — Important, MISSING

**Plain statement:** The new four-provider test reconstructs a runtime Agent instead of executing the Agent loaded through the public Agent loader.

**Obligation:** R1 FIND-001-14 steps 3–5 explicitly require the public Agent loader and loaded-Agent execution, forbid building a replacement Agent or reconstructing missing fields, and require route/provider/model inspection on that loaded Agent.

**Evidence:** `test_local_development.py:238–253` loads `WyrdState`, inspects `state.card(alias)`/`state.agent(alias)` Card holders, then calls `state.run(alias).invoke`. Python state's `agent` (`sdks/wyrd-sdk-python/src/state/mod.rs:461–462`) returns `PyAgentCard`, not `wyrd.runtime.Agent`. `Run::invoke_on` (`wyrd-client/src/observe/invoke.rs:78–100`) constructs a replacement `Agent::new(Prompt::from_native(...))` and explicitly applies the Card route with `with_execution_route`. It does not exercise Agent::from_card/from_yaml or the loaded runtime Agent's retained route; a regression in the loader that drops the route would still pass this complete new test. The existing unit roundtrip test is separate and cannot supply the combined user path requested. Restart repeats that same bypass.

**Correction:** extend the same existing journey's bundle stage to actually pass the downloaded Agent through the existing public runtime loader and execute that runtime instance with host gateway dependencies, preserving exact Prompt reference resolution and route. Reuse the loader and shared host integration rather than adding an SDK-specific runtime or a test reconstruction. Existing Python direct Agent has no gateway caller projection; if host wiring needs adjustment, compose the existing shared Agent loader/caller seam in the existing host owner, rather than add a second runtime API or silently interpret the requirement as Card-holder hydration. Keep the existing real registration/download/fresh-process/server/restart evidence and all provider/header/native-trap checks.

**Focused proof:** same exact T13 selector, all four providers before and after restart; failure of Agent::from_card route retention must fail this test. Compare actual loaded runtime route/provider/model/identities; no manually rebuilt runtime route. Card-scoped 403 and zero-dispatch assertion can remain; no new role-denial requirement is introduced.

### P-SDK-05 — new finding candidate (allocate next unused FIND ID centrally) — Important, INCORRECT docs

**Plain statement:** The gateway setup guide promises recorded calls but never enables the capture policy needed to record them.

**Obligation:** R1-DOC-AC-01/02 require executable first-request and retained-evidence proof using the documented setup; guide must agree with actual steps rather than make constructor-only or unproven claims.

**Evidence:** `set-up-the-llm-gateway.md` steps 1–5 start a fresh dev tenant, put credentials/deployment and invoke; step 6 promises the newest `vala.gateway.calls` row and tells readers to wait if empty. No capture policy is set anywhere on the page. `call-wyrd-from-a-stock-client.md` inherits this setup and repeats that every call lands in this table. `service.rs:967–978` supplies Disabled for an unset policy; `invocation.rs:895–898` returns no capture facts for Disabled; `GatewayCaptureMode::Disabled` explicitly publishes nothing. The installed CLI test enables `examples/gateway-setup/capture-policy.json` at `test_local_development.py:682–683` before invoking, proving that its evidence is not the same as the guide's visible setup. Waiting cannot produce a row when capture was disabled at admission.

**Correction:** add the existing metadata capture-policy file and `wyrd gateway capture-policy put --file ...` step before the first documented inference (or link to a prerequisite that actually applies it before the request). Reuse the exact existing setup command/config from the walkthrough/test; keep payload capture off and do not change the server's opt-in default. Explain briefly that recording depends on this policy. Both pages then inherit a working setup.

**Focused proof:** execute the guide's documented fresh-tenant setup sequence through the existing controlled first-request journey, assert its normal retained row, then docs:check. No new harness or runtime publication mechanism.

## Verification and limits

No tests/builds rerun by this specialist. Reviewed the actual new test bodies and selectors and the appended implementer evidence. Missing historical runtime RED for findings 1/2/3 is reported evidence history, not a standalone defect. The supplied TypeScript preservation limitation is consistent with its actual public accessors. Current `WyrdClient.connect()` is synchronous in TS source, so the docs' lack of await is not a finding. Framework examples correspond to actual inference tests; no claim of live paid-provider verification is made. Manual narrow-width rendering was not supplied or performed, and a11y light/dark success does not independently prove layout; no visual defect is asserted without observation.

Non-goals retained in recommendations: no local Vertex support, no Workflow restoration, no model mapping, no custom secret upload system, no replacement Agent loop, no new TypeScript Agent runtime, no new schema introspection API, no broader deployment or external repo work.
