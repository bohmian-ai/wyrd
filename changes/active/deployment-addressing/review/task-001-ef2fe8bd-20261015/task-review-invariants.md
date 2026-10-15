# TASK-001 independent invariant review

**Overall: FAIL.** Six source-validated findings remain. This report concerns immutable snapshot `/tmp/wyrd-task-review-ef2fe8bd`, candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`, base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. Live worktree movement is excluded, as authorized. No other reviewer reports were read.

## Findings

### INVREV-001 — INCORRECT — granted-deployment authority is incompatible with permission normalization and delegation (Critical)

**Location:** `crates/shared/wyrd-runtime/src/permission.rs:886` (`contains_object_scoped`), related existing `insert:866` and `intersection:917`; consumer `crates/wyrd/wyrd-server/src/components/gateway/invocation.rs:723`.

**Obligations:** REQ-002, INV-007, AC-002. `all` alone must not reach granted deployments; delegation must preserve both sides' permission intersection.

**Producer-to-sink evidence:** Role issuance merges every stored grant through `PermissionSet::insert` (`crates/wyrd/wyrd-auth/src/issuance.rs:778-784`). Insert discards a narrower scope covered by an existing grant and removes existing narrower grants when a broader one arrives. Therefore `[gateway:invoke all, gateway:invoke deployment D]`, in either insertion order, becomes only `all`. The newly added `contains_object_scoped` then returns false and `GatewayInvocation::decide` refuses D when granted. A user retaining a built-in invocation role cannot gain the approved dedicated-deployment access by adding its scoped grant.

There is also an authority escalation through the same algebra: delegation issuance at `issuance.rs:230` calls `permissions.intersection(&subject_permissions)`. Intersect actor `gateway:invoke {deployment:D}` with subject `gateway:invoke all`: `narrower(All, Deployment, covers)` produces the deployment grant. The delegated token now passes `contains_object_scoped` on granted D even though the subject's original token fails it. Reverse actor/subject produces the same intersection. This violates the approved permission intersection; SYSTEM attribution is not needed to reach it.

**Sibling reach:** Normal role issuance, token delegation, gateway requested/fallback authorization, `/v1/models`, file/batch access and batch visibility all consume this same effective PermissionSet. Fixing one gateway handler cannot repair lost authority or delegated widening.

**Smallest correction:** Correct gateway invoke authority semantics in the existing Permission/PermissionSet owner so normalization preserves the distinction needed by `open` versus `granted`, and intersection never creates granted access absent from either side. Keep existing token issuance/delegation and one gateway decision owner; do not add separate role lookup or downstream authorization stores. Preserve existing Bifrost and other resource normalization semantics.

**Focused proof:** Pure tests for all plus deployment/model grants in both insertion orders, and intersections all-only versus deployment/model grants in both directions. Real delegated-token journey: subject all-only cannot reach granted D even through a scoped actor; mixed all-plus-specific subject can directly reach D and retains correct visibility. Existing `gateway_permissions_are_typed_and_scope_checked` tests only isolated sets, so its reported pass does not exercise this counterexample.

### INVREV-002 — INCORRECT — reconstructing a runtime Agent erases its declared route (Important)

**Location:** `crates/skald/skald-agent/src/agent.rs:611`, `:708`; Python run surface `crates/skald/skald-agent/src/python.rs:355`.

**Obligations:** REQ-009–010, INV-004/007, AC-005/009.

**Evidence/reachability:** `AgentSpec` correctly requires `execution_route`. However, the runtime `Agent` has no route field. `Agent::from_card` resolves the Prompt, tools and run configuration but never retains `card.spec.execution_route`; `to_spec` always emits Native. `from_wire`, YAML/file reconstruction and Python reconstruction share this owner. `Agent::run` uses the default native registry unless a caller manually installs a provider override. A gateway Agent loaded through this supported API therefore runs natively using host credentials, and reserializing changes its route to native. Python `Agent.run` accepts only input/session/output type, so it cannot supply the required invocation override either.

`Run::invoke_on` in `wyrd-client/src/observe/invoke.rs` and `SkaldJudgeInvoker::agent_for` manually inspect their Card's route and install a registry. These special callers do not repair direct runtime Agent entry points or round trips. The new `execution_route_isolated_through_tool_loop` test similarly hand-installs two registries and never loads/routes an AgentSpec; its pass cannot establish this obligation.

**Smallest correction:** Retain the declared route on the existing Agent owner, preserve it through all construction/serialization paths, and resolve an immutable effective invocation route through the existing registry/gateway caller seam. Project its override through actual runtime SDK entry points, preserving loop callbacks, sessions, cancellation and server-native refusal. Reuse `wyrd_gateway_registry`; do not create another Agent loop or silently infer a route.

**Focused proof:** Load a gateway Agent Card, round-trip it unchanged, run it with a native provider configured and verify only the gateway caller receives requests. Exercise concurrent default/override calls through the real Agent entry point, plus Python `Agent.run` signature/runtime parity. Test a missing gateway dependency fails rather than using native credentials.

### INVREV-003 — INCORRECT — public Agent projection loses named Anthropic/Google targets and omits Jev (Important)

**Location:** `crates/shared/wyrd-client/src/gateway_caller.rs:270-294`; Jev rejection `crates/skald/skald-agent/src/request_builder.rs:37-66`.

**Obligations:** REQ-009–010, AC-005/008.

**Evidence/reachability:** `Run::invoke_on` binds the selected deployment and Prompt model into `WyrdGatewayCall.model`; `PublicWyrdGatewayCaller::call` passes it to `NativeCall::project`. The Anthropic branch writes only `model.model` to its public body. The Gemini branch puts only `model.model` in `/v1beta/models/...:generateContent`. Server `ingress.rs:232` and `:634` now require a complete `<deployment>/<model>` address. Thus Anthropic `team/claude` and Google `team/gemini` Agent calls send bare models and fail before inference. Slash-containing model IDs can instead be misparsed as an unintended deployment/model pair. The Gemini guard also treats deployment name `vertex` as provider identity, so a valid Gemini deployment named vertex is refused. Jev is absent from the public caller's request/response projection; moreover `validate_prompt_loop_request` rejects `JevSystemOne` before dispatch.

**Sibling reach:** Rust, Python and TypeScript observed invocations all share this caller. The server judge caller carries `call.model` separately and does not have the bare-address problem. Direct native HTTP tests `native_ingress_preserves_named_deployments` construct full addresses themselves; they do not exercise the broken Agent producer. The setup journey uses direct protocol calls too.

**Smallest correction:** Project the existing `ModelRef` intact into body-model public ingresses and the full GenerateContent resource, preserving its model suffix and existing encoding safety. Remove provider inference from deployment spelling; retain Vertex refusal based on actual Prompt provider. Complete Jev's existing typed no-tools/no-streaming Agent invocation path and public response projection without adding general Jev tools or another orchestration API.

**Focused proof:** Real Agent→public gateway journeys for named Anthropic, Google and Jev deployments (including Gemini deployment named vertex and slash-containing model), asserting upstream target/model and credentials. Retain local Vertex override rejection before IO.

### INVREV-004 — INCORRECT — Prompt identity remains templatable and can select another native model (Important)

**Location:** `crates/skald/skald-spec/src/authoring.rs:214-225`; downstream `crates/skald/skald-spec/src/prompt.rs:324-327`.

**Obligations:** REQ-005/008, INV-004/007, AC-006.

**Evidence/reachability:** The compiler checks only `model.trim().is_empty()`. A portable OpenAI Prompt with `model: '{{target}}'` compiles, inserts this string into the request's model, and `Prompt::new` derives `target` as a text variable by scanning the request. `Prompt::bind_mut` replaces every request string leaf, including model. A native `Run.invoke([('target','other-model')])` or runtime Prompt render therefore changes the actual model while the stored Prompt identity remains the template. Nonempty controls/unsafe identities also pass this compiler; gateway `ModelId` validation happens later and native routes bypass it. The specification explicitly makes provider/model literal configuration, not variables.

**Smallest correction:** Validate literal model identity at the shared authored compiler before deriving/rendering variables, reusing the existing identifier grammar where layering permits. Reject placeholder-bearing model identity at registration/load; keep slash model IDs legal. Keep rendering limited to authored content/settings locations allowed by contract rather than using mutable target identity as a text variable.

**Focused proof:** Shared compiler and HTTP/SDK negative cases for `${target}` / `{{target}}` model values and controls; native dispatch fixture proves supplied content variables cannot change the Prompt model. Retain slash-containing model positive case.

### INVREV-005 — INCORRECT — portable media bindings discard their required kind (Important)

**Location:** `crates/skald/skald-spec/src/authoring.rs:413-426`; consumer `crates/skald/skald-spec/src/prompt.rs:346`.

**Obligations:** REQ-006/008, AC-006.

**Evidence/reachability:** `PortableMessage::lower` knows whether the authored block is Image or Document, but for a binding preserves only its placeholder string. The code explicitly comments that binding kind is not checked. `Prompt` retains only media variable names; `bind_media_mut` maps the caller's `MediaRef.kind` directly into native media. An authored image `${media:photo}` bound to a document becomes a document block, and the inverse becomes an image, rather than a pre-dispatch refusal. Judge record media uses this exact binder through `SkaldJudgeInvoker::bind_media`; Python Prompt media bindings also reach it. This silently changes the declared content contract.

**Smallest correction:** Preserve the authored expected kind through the shared Prompt compilation/binding owner and reject a mismatched `MediaRef` there before modifying the request. Reuse existing MediaKind/MediaRef and binder; no second media resolver or SDK-specific validation.

**Focused proof:** For each portable provider mapping, image→document and document→image bindings fail before provider IO; matching typed bindings preserve order. Include one real negative SDK/judge path across the binding boundary. Existing compiler test only verifies the placeholder is present.

### INVREV-006 — MISSING — shared authoring compilation does not validate supplied schemas (Important)

**Location:** `crates/skald/skald-spec/src/authoring.rs:360`, `:518-523`, native decode `:292-327`.

**Obligations:** REQ-006–008/011, AC-006.

**Evidence/reachability:** `response_format` is decoded only as `Map<String, Value>` and copied into provider schema fields; native schema fields are likewise decoded into JSON containers. `AuthoredPrompt::compile` calls `Prompt::new`, whose complete body derives response type/media/text placeholders without schema validation. `wyrd-spec/src/card/prompt/validate.rs::validate` validates parameter names only. Therefore `response_format: {"type":"not-a-json-schema-type"}` is accepted on registration/load and later sent upstream. No compiler path validates even JSON Schema syntax, much less destination-unsupported features required to be rejected. Runtime output checking happens after provider IO and cannot establish the registration refusal. This is observable across both stored Prompt forms and all SDKs, not an optional model capability catalog.

**Smallest correction:** Put schema syntax and statically detectable destination compatibility validation in the existing shared pure compiler, using existing schema helpers/dependencies where available. Refuse the offending field without weakening or silently rewriting the requested schema. Preserve native extension options unrelated to schema.

**Focused proof:** Pure invalid-schema and unsupported-schema cases for each destination, plus real HTTP/SDK negative authoring proving rejection before storage/IO. Current `portable_content_settings_and_schema` asserts schema copying and output derivation; it has no invalid-schema case.

## Acceptance matrix

Evidence paths are relative to the immutable snapshot. PASS means inspected source supports the obligation and the recorded focused proof is credible for that bounded behavior; it does not mean tests were rerun. FAIL rows name demonstrated counterexamples or missing required proof. T01–T17 refer to the exact focused commands recorded in the original task, not independently observed executions.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 named deployment/addressing | `gateway/deployment.rs`, `gateway/mod.rs::ModelRef`, gateway `routing.rs`, defining SQL migration | T03/T04/T17; address/model allowlist assertions | PASS |
| REQ-002 authorization/visibility | invocation `decide/models`, batch `invocable/list`; permission algebra loses distinctions | T04 isolated grant cases do not close INVREV-001 | FAIL |
| REQ-003 onboarding/single-list lifecycle | CLI `DeploymentFile::read/put_deployments`; server `put_deployment`; credential SQL FK binding | T06/T13 partial progress/convergence; T16 redaction | PASS |
| REQ-004 external deployment workflow | credential resolver uses credential provider and assignment; adapter forwards chosen model; external endpoint shares transport | T05 checks model suffix/auth/revocation; existing credential/transport proof retained | PASS |
| REQ-005 identity once/literal | authored fields once; compiler permits templated model | T01/T02 lack identity-placeholder refusal; INVREV-004 | FAIL |
| REQ-006 portable content/settings/schema | compiler maps eight settings; binding kind and schema checks absent | T02 covers mapping, not INVREV-005/006 | FAIL |
| REQ-007 native form/pairs | short formats, mixed/body.model rejection, extension-preserving wire owners; schema validation absent | T01/T02 native round trip; invalid schema gap | FAIL |
| REQ-008 storage/derivation/shared compiler | authored serialization/hash, inline materialization and hydration; required validation incomplete | T07/T14/T15 authored journeys; INVREV-004–006 | FAIL |
| REQ-009 Agent execution configuration | wire route required; runtime Agent erases it and lacks direct override | T08/T09 use special/manual registry setup; INVREV-002 | FAIL |
| REQ-010 resolution/failure behavior | native route/special judge refusal exist; public projection strips targets; Jev rejected | T10 tests direct native HTTP, not Agent projection; INVREV-002/003 | FAIL |
| REQ-011 errors/consumer completion | shared catalog and generated projections exist; direct Agent consumer incomplete | T12/T14–T16 do not cover all surviving Agent entry points | FAIL |
| REQ-012 Workflow removal/preservation | Card/Spec/runtime/jobs/SDK/CLI/UI owners removed; Eval task DAG/workflow hint retained | T12 retired HTTP contracts; surviving Agent/judge tests retained | PASS |
| REQ-013 simple dev journey | existing installed CLI harness; example gateway setup; four adapters/restart/fresh saved TS auth | T13 source drives actual CLI/server and retained read path | PASS |
| INV-001 tenant isolation | TenantConn RLS gateway tables, tenant snapshot and credential sealing binding, captured caller tenant | T04/T05 tenant/credential proof; no newly found cross-tenant path | PASS |
| INV-002 credential protection | managed tenant/name/provider binding, operator exact-host assignment, resolver redacted errors, upstream transport | T05/T13 secret headers/redaction/restart proof | PASS |
| INV-003 no compatibility/weights | old fields rejected; ModelRef deployment scopes; Workflow routes/crate gone | T01/T03/T06/T12; static removal inspection | PASS |
| INV-004 stable Prompt/Agent identity | canonical authored hash and observed override are stable; runtime Agent round trip changes route | INVREV-002/004; T08 cannot close runtime gap | FAIL |
| INV-005 Rust/server/shared client owners | server owns SQL/authorization; pure compiler in Skald; language bindings call Run::invoke_on | recorded dependency/type gates plus source ownership | PASS |
| INV-006 nonblocking one outbox | invocation decide stages process scribe outbox; capture existing outbox; no new acceptance writer | source inspection and recorded regression assertions | PASS |
| INV-007 no hidden route/authority/option widening | runtime route discarded; permission intersection can widen granted access | INVREV-001/002/004/005 | FAIL |
| AC-001 onboarding/models/capabilities/same-adapter | deployment/planner/credential/server owners and three SDK gateway tests | T03/T04 and existing gateway-admin journeys | PASS |
| AC-002 coverage/subject/fallback/visibility | shared decide path, but effective-grant algebra counterexample | T04 does not test mixed grants/delegated granted access | FAIL |
| AC-003 external forwarding/security | external credential provider retained independently; unchanged model suffix and screened transport | T05/T13 plus existing transport/credential tests; no fresh execution | PASS |
| AC-004 both forms/hash/inline/introspection | PromptSpec authored codec/hash; server persists inline Prompt independently under current stronger authority; state resolves exact Prompt Card | T07/T14/T15 preserve forms/introspection | PASS |
| AC-005 native/named/external/loop/override | observed OpenAI invocation supports overrides; actual runtime route and non-OpenAI Agent paths incomplete | T08–T09/T14–T15 are narrower than required matrix | FAIL |
| AC-006 compiler/negative/media/schema matrix | mappings implemented; identity/media-kind/schema refusals missing | T01/T02/T07 omit demonstrated invalid inputs | FAIL |
| AC-007 judge structured output/authority | judge resolves Agent route, refuses native in server context, binds SYSTEM caller; dependent assertion execution retained | T11 and `judge_routes_its_context_cannot_serve_are_refused`; attribution follows Bifrost current authority | PASS |
| AC-008 native/public exact target/Vertex | direct HTTP native addresses supported; Agent public projection inconsistent | T10 direct ingress cannot prove Agent producer; INVREV-003 | FAIL |
| AC-009 cross-surface agreement | schemas/docs describe route; runtime Agent/Python run surface disagree | T12/T14–T16 do not establish actual Agent.run contract | FAIL |
| AC-010 Workflow removal/independent Eval | no registrable Workflow/runtime/server jobs; independent Eval workflow schema/JSONPath untouched | T12/OpenAPI/type checks; surviving Eval/Agent tests retained | PASS |
| AC-011 installed CLI/fresh TS/four targets | existing real-process bootstrap uses saved config, four credential files, list command and upstream fixtures | T13 checks requests, authentication and retained gateway evidence | PASS |
| AC-012 list failures/convergence/restart | full list structural validation before sequential PUT; ordered completion/failure/unattempted; restart reuses saved keys | T06/T13 assertions inspect status order, no writes, rerun and decrypted requests | PASS |

## Non-goals and regression boundaries

| Boundary | Source audit | Result |
|---|---|---|
| No new server-hosted Agent job/runtime API | Removed Workflow server owners; gateway caller remains model-call seam | PASS |
| No aliases/model renaming/catalog/provisioning/new cloud | ModelRef preserves suffix; selected deployment auth separate; no new catalog owner | PASS |
| Existing embedding/Predict/raw runtime retained, not Prompt formats | AuthoredPrompt excludes these formats; ProviderRequest owners retain variants | PASS |
| No portable audio/video or local Vertex ingress | Portable ContentBlock only text/image/document; observed local Vertex refused | PASS |
| Preserve reusable Agent/gateway/judge/Eval execution | Owners retained, but routed Agent entry points fail INVREV-002/003 | FAIL |
| No deployed-data deletion, credential rotation, list atomicity claim | Defining migration edited for unshipped contract; list uses per-item existing PUT; no data-deletion execution | PASS |
| Approved adjacent work is not scope drift | Query collector retains existing query/settlement owners; release/protoc/telemetry changes treated as adjacent authorized changes, not rejected for adjacency | PASS |

## Scope, source and verification limits

Navigation covered pure Prompt/Agent/deployment/auth contracts; shared permission algebra and token-issuance consumers; gateway SQL/credential/snapshot/planning/admission/visibility/capture; public/server Agent callers; direct Agent construction/run/serialization; observed SDK invocation and bindings; judge route/cache/media execution; inline registration/hydration; CLI single/list setup and installed dev journey; Workflow removal and retained Eval contracts; generated/public/UI/catalog removal paths and adjacent query/tooling changes. The complete cumulative path/diff lists were used to locate changes. Current stronger inline-Prompt materialization authority was applied; old task private mechanics were not imposed.

No builds/tests were run, as instructed. The task records focused T01–T17 and static gates as passing; those are claims, not newly reproduced logs. Their test source proves narrower cases than several stated acceptance criteria, as identified above. In particular, a passing direct native-ingress test or manually installed provider-registry test does not validate the actual Agent-to-ingress producer. The integrated `mise run gate` remains change-review proof; its absence here is not independently an implementation finding. Full external infrastructure, runtime redaction under all failures, and adjacent release/publishing operations were not executed. No source, task, authority or other reviewer file was edited.

**FAIL**
