# Independent TASK-001 behavior review

**Result: FAIL.** Source proves several reachable failures in the revised Agent and Prompt contracts. The recorded passing selectors exercise narrower paths and do not establish the full approved acceptance matrix.

## Subject and method

- Base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
- Candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`.
- Immutable source: `/tmp/wyrd-task-review-ef2fe8bd`; all continued source reads used this snapshot. It has no `.codegraph`, so CodeGraph was skipped there. The earlier interrupted attempt is not acceptance evidence.
- Authority: approved `changes/active/deployment-addressing/spec.md` r5; original TASK-001; AGENTS; agent-rules; spec-driven development, maintainer-style and reference routing; relevant active design/doctrine, testing, evaluation, security and Bifrost attribution/durability contracts. Current design's inline Prompt materialization is stronger authority than a private task mechanism; it is not reported as scope drift.
- Review-only static analysis; no tests/builds, implementation changes, staging or commits. Recorded task commands are claims, distinguished below from inspected test assertions. User-approved adjacent changes are not rejected for adjacency.

## Findings

### BEH-001 — Important — INCORRECT: direct Agent APIs lose and bypass the required route

**Location:** `crates/skald/skald-agent/src/agent.rs:611`, `:668`, `:708`; `crates/skald/skald-agent/src/python.rs:355`.

**Obligations:** REQ-009/010, INV-004/007, AC-005. Active design Agent contract requires every model call to honor its declared/effective route.

`Agent::from_yaml_path` → `from_yaml_str` → `from_card` resolves the Prompt and tools, but `from_card` never retains `card.spec.execution_route`. `Agent` has no route state. `to_spec` unconditionally emits `ExecutionRoute::Native {}`. `Agent::run`/`run_with` use the host provider registry, and Python `Agent.run` exposes only input/session/output type, with no invocation route override. `DelegateAgentTool` also calls `run_with` on the reconstructed Agent (`delegate.rs:128`). Thus an authored gateway Agent loaded through supported direct Agent APIs can send model calls with native host credentials, or fail for missing native configuration, and a save changes its route to native. This is an observable bypass, not merely a missing field accessor.

The separate observed path `wyrd-client/src/observe/invoke.rs:75` reads and applies a route, but explicitly rejects Agent tools at `:82`; it does not close the direct Agent/tool-loop contract. Judges implement another route application in `vala-eval/src/orchestrator/judge.rs:197`, showing route behavior lives at caller wrappers rather than the existing Agent owner. Python `AgentCard` authoring at `wyrd-cards/src/agent.rs:124` also silently defaults to native without accepting the required field; include that sibling projection in correction.

**Required correction:** retain the authored route and resolve one invocation-local effective route at the existing Agent execution owner; use the existing gateway caller/registry seam for its complete loop and sub-agent boundaries. Preserve route in from_card/to_spec/save/load and expose the same whole-route override on existing public Agent execution surfaces. Do not invent a new loop or weaken server-native refusal. Newly authored durable Agent contracts must require the route rather than silently synthesize native.

**Focused proof:** load a gateway Agent through direct Rust/Python public APIs, run a two-iteration tool loop against a controlled gateway, and round-trip its Card unchanged. Run default and overridden invocations concurrently; verify only the selected deployment receives each call and a configured native provider receives none. The current T09 constructs two different Agents with manually installed provider registries (`agent_journey.rs:317`) and bypasses the missing declared-route behavior.

### BEH-002 — Important — INCORRECT/MISSING: the public Agent gateway caller drops named native targets and omits Jev

**Location:** `crates/shared/wyrd-client/src/gateway_caller.rs:270–294`.

**Obligations:** REQ-001/009/010, AC-005/008.

`Run::invoke_on` builds `<deployment>/<Prompt model>` in `wyrd_gateway_registry`, but `NativeCall::project` overwrites Anthropic's model with only `model.model` (`:271`) and constructs Gemini's path using only `model.model` (`:285`). Public server ingress requires the full deployment/model address (`gateway/ingress.rs:248`, `anthropic_messages`, and `/v1beta/models/{deployment}/{target}`). A Prompt for `claude-sonnet-4-5` routed to `claude-prod` therefore sends an unaddressed model and fails; Gemini `gemini-team` likewise misses the deployed route. Slash-containing model suffixes can instead be misread as another target. The same match has no `JevSystemOne` branch, so supported Jev Agents always get a local bad-request refusal.

The guard `model.deployment.as_str() != "vertex"` additionally treats an arbitrary deployment name as provider identity: a legitimate Gemini deployment named `vertex` is refused. The existing `Run::invoke_on` already rejects actual Vertex Prompts before calling this producer (`observe/invoke.rs:94`); a naming convention is unnecessary and incorrect.

**Shared reachability:** Rust/Python/TS observed Run invocation consumes this single public caller. Server judges use `ServerWyrdGatewayCaller`, which retains a separate `ModelRef` and has a Jev branch; raw HTTP native ingress is another sibling and must remain supported. The T10 raw HTTP test manually constructs correct addressed requests and therefore cannot detect this Agent producer defect.

**Required correction:** project full deployment/model addresses to the existing native public ingress, reuse the supported Jev request/response projection, and use actual provider/route information for explicit local Vertex refusal. Preserve model suffixes and existing public transport/auth; no alternate endpoint or deployment-name restriction.

**Focused proof:** real SDK Agent/Run calls to named Anthropic, Google and Jev deployments, including slash-containing model suffixes and a Gemini deployment named `vertex`; assert upstream model/deployment and no IO for an actual local Vertex override.

### BEH-003 — Important — INCORRECT: portable media binding erases its declared kind

**Location:** `crates/skald/skald-spec/src/authoring.rs:413–426`; consumer `prompt.rs:346–371`.

**Obligations:** REQ-006, INV-007, AC-006.

`PortableMessage::lower` knows whether a block is `image` or `document`, but turns either binding into the same text placeholder. The source explicitly defers checking the binding MediaRef kind. `Prompt::bind_media_mut` and the OpenAI/Anthropic/Google binders subsequently insert the kind supplied by the caller without comparing it to the authored block. An image block bound to a document therefore changes meaning and can dispatch successfully. Judges' resolved media and public Prompt.bind_media use the same binder, so this is not confined to a constructor test.

**Required correction:** keep the authored binding-kind invariant in the existing shared Prompt/compiler/binder owner and reject cross-kind bindings before dispatch. Do not add another media subsystem. Preserve provider source/MIME restrictions and order.

**Focused proof:** public binding of document→image and image→document fails before provider IO for portable blocks; matching kinds succeed across the supported provider mappings. Test repeated binding names with conflicting authored kinds explicitly. T02 inspects an unbound image placeholder but never supplies a conflicting MediaRef.

### BEH-004 — Important — MISSING: shared authoring accepts invalid response schemas

**Location:** `crates/skald/skald-spec/src/authoring.rs:360`, `:518`, `:608`, `:668`; `crates/wyrd-spec/src/card/prompt/validate.rs:232`.

**Obligations:** REQ-006/007/008/011, AC-006.

Portable `response_format` is only a JSON Map. Compilation copies it into each protocol envelope; no schema validity or destination-supported-feature check runs. Native output schemas likewise only decode their JSON container. `Prompt::new` derives response metadata without validating it. `PromptSpec::deserialize` recompiles then checks placeholder names only; server `validate_request` (`cards/service.rs:1508`) invokes that typed decoder, so it does not add a schema check. For example `response_format: {"type":"not-a-schema-type"}` is accepted as a registered Prompt and sent upstream instead of refused at authoring. Valid JSON is not valid JSON Schema.

The provider-specific wire types preserve this Map/Value. Agent structured-output parsing (`loop_runtime.rs:801`) only parses an object after the provider call, so it cannot validate the declaration before storage or IO. SDK, loader, HTTP and inline Prompt authoring all converge on this missing shared check.

**Required correction:** validate portable/native output declarations in the existing shared synchronous authoring acceptance path, reuse existing schema facilities where applicable, and explicitly reject features the destination cannot carry. Preserve the declared schema rather than weakening it or creating language-specific validators. No network schema discovery.

**Focused proof:** compiler and real public registration rejection of malformed schemas and an unsupported destination feature; valid portable/native schema remains unchanged in GET/hash and reaches its supported output envelope. T02's schema is merely a valid fixture; it has no invalid-schema assertion.

### BEH-005 — Important — INCORRECT: gateway Agent requests do not enforce the requested provider/adapter matrix

**Location:** `crates/shared/wyrd-client/src/observe/invoke.rs:93–109`; `crates/skald/skald-agent/src/gateway.rs:78–91`; server `components/gateway/caller.rs:110`; `wyrd-gateway/src/adapter/mod.rs:205–224`.

**Obligations:** REQ-010's explicit initial requested-target matrix, INV-007, AC-005/008.

The Agent gateway seam carries a request and ModelRef, but not the Prompt's provider as a destination constraint. Neither the public Run path nor server caller checks the selected deployment adapter against that provider. `CallPlan::retain_representable` calls ordinary direct-gateway conversion, which deliberately allows OpenAI Chat bodies to Anthropic/Gemini/Vertex and Gemini bodies to Vertex. Therefore an OpenAI Prompt routed directly to an Anthropic deployment can dispatch through translation, even though r5 permits those cross-adapter conversions only for direct gateway callers or explicit fallback, not as an Agent's incompatible initial target. This also means a local OpenAI Agent may target a Vertex adapter without the existing `prompt.provider() == Vertex` check catching it.

**Required correction:** enforce the approved initial Agent provider/adapter compatibility at the existing gateway admission/dispatch boundary, carrying the necessary typed origin constraint through both public and server Agent callers. Preserve supported direct-client translations and explicitly configured fallback conversion. Reuse the same decision path and avoid requiring administrative reads for invokers. The correction must not simply ban translations globally.

**Focused proof:** OpenAI Agent→Anthropic/Vertex initial target and Google Agent→Vertex initial target refuse before IO; compatible requested targets work; direct OpenAI-client conversion and explicitly authorized supported cross-adapter fallback continue working. No inspected T08/T14/T15 test attempts an incompatible adapter.

### BEH-006 — Important — REGRESSION: granted batches can hide every older visible batch behind an unusable page

**Location:** `crates/wyrd/wyrd-server/src/components/gateway/batches.rs:575–617`; pruning source `:168–197`; SQL owner `wyrd-sql/src/queries/gateway.rs:1000`.

**Obligations:** REQ-002 same invoke coverage for batch discovery/access; AC-002.

For a built-in caller holding only `all`, `invocable()` returns None, so SQL reads batches without deployment access pruning. `list()` fetches only `limit + 1`, then filters out `granted` deployments by `GatewayInvocation::decide`. If the newest `limit` rows are granted, it returns `data: []`, `last_id: null`, `has_more: true`, even when older open batches exist. Stock clients have no returned ID to send as `after`, so cannot reach those visible batches; a sufficiently large run of hidden rows makes discovery unusable. With some visible rows the page still derives continuation from filtered data rather than the scanned cut.

The SQL owner already supports eligible deployment/model restrictions. Current pruning only models grant scope, not open/granted access. File/get/cancel access calls use the authoritative exact decision and must retain that behavior; this finding concerns listing progress, not broadening access.

**Required correction:** make page selection/continuation reflect the same access coverage before taking the page, using the existing snapshot, scoped SQL list and authorization owner. Preserve bounded reads, tenant RLS and per-evaluation audits. Do not solve it by admitting granted rows or returning hidden IDs as public batch objects.

**Focused proof:** create newer granted batches plus an older open batch, list as an all-only caller with a small limit, and paginate to the visible batch without repeated empty/no-cursor pages. Repeat for explicit deployment/model grants and tenant separation.

### BEH-007 — Important — INCORRECT: model template variables can change the native inference target

**Location:** `crates/skald/skald-spec/src/authoring.rs:214–230`; `prompt.rs:220–221`, `:324–328`, `:390`.

**Obligations:** REQ-005 literal identity, REQ-008 validation before use, INV-004/007, AC-006.

Compilation checks model only for trimmed emptiness, inserts it into a body-model request, and `Prompt::new` scans the entire request for variables. Thus `model: "{{chosen_model}}"` becomes a derived text variable; `Prompt::render`/`bind_mut` substitutes it inside the request model. Native Agent/Prompt execution sends that changed body model even though the stored Prompt model and hash declare the template text. Controls and other invalid identifier values also survive registration because PromptSpec validation only checks variable names. Provider is validated separately; model identity is not.

**Required correction:** reject template placeholders and invalid model identifiers at shared Prompt authoring/validation before storage or dispatch, preserving slash-containing literal identifiers. Keep target fields out of content substitution. Use the foundational validation owner without adding an IO dependency or a second language-specific parser.

**Focused proof:** portable and native body-model Prompts with `{{model}}`/`${model}` or invalid controls are refused on public load/registration; ordinary content placeholders still render and literal slash-containing models remain unchanged.

## Acceptance matrix

Implementation and proof are separate columns. `PASS` in proof means inspected source assertions plus recorded focused evidence plausibly cover that obligation; no command was rerun. `FAIL` identifies a source-proven defect above or a specific missing required proof. A task selector passing does not prove scenarios it never drives.

| Obligation | Implementation evidence and result | Verification evidence and result | Overall |
|---|---|---|---|
| REQ-001 | PASS: ModelRef split_once; deployment optional lists/access; deterministic CallPlan; SQL stores deployment/model identities | PASS: T03 named_target_and_allowlist_resolution; T04/T05 named onboarding/unchanged suffix; recorded focused passes | PASS |
| REQ-002 | FAIL: requested/fallback exact decide and nonblocking audits are present; batch pagination BEH-006 | FAIL: T04/T08 cover grants/denial and model visibility, but no hidden granted-page progression | FAIL |
| REQ-003 | PASS: existing CredentialWriter/GatewayAdministration; provider FK and binding preserved; CLI DeploymentFile prevalidation, ordered partial results | PASS: T06 asserts empty/duplicate/invalid no writes, partial failure, rerun, single shape; T13 credential-first/restart | PASS |
| REQ-004 | PASS: external adapter resolves credential provider independently; addressed model suffix preserved; existing screened transport retained | PASS: T05 external model/credential/revoke and T13 header checks; existing gateway security owners remain | PASS |
| REQ-005 | FAIL: provider/model stored once but model may be a render variable (BEH-007) | FAIL: T01/T02 cover pair/shape/slash model but not target template refusal | FAIL |
| REQ-006 | FAIL: eight portable settings and mappings exist; media kind/schema validation BEH-003/004 | FAIL: T02 valid mappings/unsupported settings covered, not mismatched bindings/invalid schemas | FAIL |
| REQ-007 | FAIL: short format/pair/mixed-body/model refusal present, schema acceptance incomplete (BEH-004) | FAIL: native_body_pairs_and_round_trip covers x_custom and shape; schema negatives absent | FAIL |
| REQ-008 | FAIL: authored serialization/hashing and shared codec present; early schema/model validation incomplete | FAIL: T07 round trips and current inline materialization seam inspected; incomplete negative schema/identity proof | FAIL |
| REQ-009 | FAIL: wire route required; observed override applies whole; direct Agent drops route, no direct run override (BEH-001) | FAIL: T08/T14/T15 observed OpenAI override; T09 manually installs registries and bypasses durable route | FAIL |
| REQ-010 | FAIL: native/public/server routes incomplete or incompatible (BEH-001/002/005) | FAIL: T10 raw HTTP succeeds without exercising public Agent projection; initial incompatibility absent | FAIL |
| REQ-011 | FAIL: generated types/typed wire/errors largely project contract; direct Agent API differs and malformed schema accepted | FAIL: T12/T16 codegen/catalog evidence cannot close runtime producer and validation gaps | FAIL |
| REQ-012 | PASS: Workflow Card/runtime/routes/SDK/CLI/UI workspace/operator branch removed; surviving Agent/gateway/judge and independent Eval remain | PASS: T12 retired registration/OpenAPI; deletion inventory and live Eval contract source; retained Agent/judge assertions | PASS |
| REQ-013 | PASS: installed CLI/dev/startup reuse, four-provider config and fresh TS client driver present | PASS: T13 source proves ordinary uploads, list/single/rerun, actual calls/evidence, key reuse and restart; lane selects it | PASS |
| INV-001 | PASS: credential/snapshot and registry/query paths retain verified tenant and TenantConn/RLS; no new cross-tenant identity authority found | PASS: T04/T05/T13 recorded tenant/credential coverage; SQL/FK source inspected | PASS |
| INV-002 | PASS: secrets remain credential source/keyring-bound, off Cards; no caller headers forwarded; registration/loading has no secret IO | PASS: T05/T13 header/redaction/restart tests; CredentialResolver assignment checks | PASS |
| INV-003 | PASS: new route/address decoders reject old fields; weighted selection and retired Workflow owners removed | PASS: T01/T03/T06/T12 negative shape/catalog coverage; no compatibility shim found in inspected owners | PASS |
| INV-004 | FAIL: observed route does not mutate Card, but direct Agent save changes route and model rendering changes target (BEH-001/007) | FAIL: observed unchanged-route assertion does not exercise direct save/load/native target identity | FAIL |
| INV-005 | PASS: server decisions stay Rust-owned; SDK Run wrappers call wyrd-client; no new replacement application runtime | PASS: Rust ownership/dependency/caller source and recorded check:deps; same shared caller affects all SDKs | PASS |
| INV-006 | PASS: GatewayInvocation::decide stages process outbox; existing audit/capture sink retained | PASS: inspected source has no synchronous audit acceptance dependency/new queue; recorded regression evidence | PASS |
| INV-007 | FAIL: gateway→native loss and incompatible requested target/media substitution (BEH-001/003/005/007) | FAIL: missing direct API, conflicting binding and initial-adapter negatives | FAIL |
| AC-001 | PASS: deployment contract/planner/persistence implement required shapes, named identities and defaults | PASS: T03/T04 and SDK gateway-admin source; recorded passes; controlled fixtures | PASS |
| AC-002 | FAIL: grant decision lattice largely correct; batch discovery progression BEH-006 | FAIL: exact granted-page pagination missing; no zero-upstream inference on those listing branches can substitute | FAIL |
| AC-003 | PASS: external credential/model/evidence owners preserved without renaming | PASS: T05 unchanged suffix/revoke and T13 headers/normal evidence path; controlled fixtures | PASS |
| AC-004 | PASS: shared authored codec and server-owned inline Prompt materialization preserve Prompt fields; hydrated typed Prompt navigation updated | FAIL: inspected Python/TS agent-route journeys execute fixture forms but do not prove the complete three-SDK standalone/inline authored JSON/hash/introspection matrix | FAIL |
| AC-005 | FAIL: direct/default/override/tool-loop/native family completion BEH-001/002/005 | FAIL: T08/T14/T15 cover only observed OpenAI declared/backup/granted/malformed override; no complete native/external/tool/timeout/compatibility matrix in those tests | FAIL |
| AC-006 | FAIL: media/schema/literal target invariants BEH-003/004/007 | FAIL: T02 does not prove invalid schemas, cross-kind binding or target-template refusal; HTTP/SDK negative matrix incomplete | FAIL |
| AC-007 | PASS: constrained judge checks route; server-native route refused; uses accepted caller's gateway service path | PASS: T11 both forms/structured verdict/off-route refusal; retained SYSTEM attribution tests and offline route refusal source | PASS |
| AC-008 | FAIL: raw ingress named targets work; SDK Agent projection fails and initial adapters not constrained | FAIL: T10 sends raw HTTP, no public Agent named-family round trip; actual local Vertex rejection guard doesn't prove adapter incompatibility | FAIL |
| AC-009 | FAIL: durable/generated route contract differs from direct runtime/Python Agent API | FAIL: codegen/OpenAPI/catalog checks cannot prove direct route semantics; public consumer completion BEH-001/002 | FAIL |
| AC-010 | PASS: retired product removed across kind/ref/permission/runtime/server/SDK/CLI/UI; independent Eval schema/JSONPath retained | PASS: served retired-kind/path rejection T12 and deletion/source inspection; no demand to remove unrelated workflow vocabulary | PASS |
| AC-011 | PASS: existing installed CLI startup and first tenant workflow reused; fresh saved-config TS calls all four | PASS: T13 plus TS local-development cli_gateway_setup, retained evidence reads and expected upstream headers | PASS |
| AC-012 | PASS: structural file validation precedes mutations; per-item PUT progress/convergence and existing dev key reuse | PASS: T06 partial results/no writes/rerun/single/stdin and T13 restart/decryption; recorded focused passes | PASS |

## Non-goals and regression boundaries

| Constraint / non-goal | Source assessment | Result |
|---|---|---|
| No server Agent submission/status/cancellation replacement | Workflow job/routes removed; observed local invocation and existing verification retained; no replacement server job API identified | PASS |
| No model aliases, model renaming, weighted selection or provider-name deployment default | New ModelRef/deployment/CallPlan use literal named identities; explicit fallbacks retained | PASS |
| No universal external protocol/auth or new cloud integration | External remains OpenAI-compatible; credential sources/headers/managed binding reuse existing owners | PASS |
| Embedding/Predict/raw remain runtime capabilities, not Prompt authoring formats | Authored PromptFormat admits five generation formats; ProviderRequest still retains non-Prompt variants | PASS |
| No portable audio/video | ContentBlock remains text/image/document; native capabilities remain | PASS |
| No new local Vertex ingress; preserve native/server Vertex | Actual Vertex Prompt is refused locally; existing server adapter survives, but OpenAI/Google Agent targeting Vertex violates initial compatibility (BEH-005) | FAIL |
| No deployed data deletion | Defining gateway migration changes metadata shape; no executed database operation; removed product machinery does not delete deployed data | PASS |
| Retain independent Eval DAG/conditions/assertions/pass gates and observed-payload fields | Eval executor/scenario/task and `workflow` schema/JSONPath fields remain; generic terminology is not retired product routing | PASS |
| Preserve reusable Agent callbacks/tools/sessions/journal and gateway/judge behavior | Existing loop remains; direct route handling/round trip is incomplete (BEH-001); public native caller regression BEH-002 | FAIL |
| Reuse complete owners; no alternate graph/loader/application runtime | Registration extends existing server owner for materialized inline Prompt; shared state/hydration consume exact Prompt refs | PASS |
| Approved adjacent changes are subject to correctness, not automatic scope drift | Release/protoc/telemetry/selection/query changes included in cumulative surface inventory; no adjacency-only finding | PASS |

## Proof assessment and limitations

T01–T17 and owner-check exit-zero statements are recorded in the task, not independently executed by this reviewer. Their actual inspected assertions establish selected happy/negative paths, not all statements in their prose. In particular T09 bypasses declared routes, T10 bypasses Agent projection, and T08/T14/T15 only exercise OpenAI observed invocation. Required missing closure is specific: direct Agent route round-trip/execution; named native-family Agent calls; requested-adapter mismatch; media kind mismatch; invalid schemas; target templates; hidden granted-batch pagination; and the complete three-SDK authored/hash matrix. Add focused proof at those existing owners, not a new test harness or broad aggregate remediation requirement.

The final integrated `mise run gate` remains change-review work under r5; its absence during task discovery is not itself a finding. No live cloud/provider calls or infrastructure tests were run. Static review cannot certify runtime green status or every adjacent operational failure path; no speculative hardening or optional refactors are proposed.

**Overall: FAIL — BEH-001 through BEH-007 require independent validation and bounded correction.**

Skipped builds/tests as instructed. Source-proven route and validation gaps remain despite recorded focused passes.
