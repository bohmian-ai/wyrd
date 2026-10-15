# Independent structured validation — TASK-001 re-review

## Subject, method and result

Immutable source: `/tmp/wyrd-task-review-9516afcd`. Base
`1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`; candidate
`9516afcdc7d3ffb58d5f20e85c4341d0df3cef23`; prior candidate
`ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`. Live changes are excluded.
The archive has no CodeGraph index. No production edits, builds or test runs
were performed by this validator.

Inputs were the subject/navigation, cumulative and remediation diffs, approved
spec r6, original task, prior ledger/verdict/approval resolution, R1 packet and
appended evidence, nine discovery reports, claim comparison, focused follow-up,
verification notes, and governing repository/architecture authority. The
remediation diff located changed owners; it did not replace cumulative task
acceptance. Adjacent unrelated changes have explicit owner approval. Source
tracing, rather than agreement or reported green checks, establishes the
findings below.

**Validated result: bounded corrections remain.** Retain FIND-001-3, 5, 8 and
13; retain the unsatisfied, explicitly human-directed FIND-001-14 requirement;
add FIND-001-15 for the documentation's missing capture prerequisite and its
associated retained-evidence proof. The approved behavior supplies every
decision needed. No specification revision is required. Two lower-authority R1
mechanical assumptions require task correction: TypeScript introspection cannot
be waived because an accessor is absent, and the owner did not require the
downloaded-Agent stage to use Python. Correct those instructions in the next
remediation packet instead of adding an unnecessary Python runtime binding.

## Proposal disposition

| Discovery/follow-up source | Disposition | Stable boundary |
|---|---|---|
| P-SDK-02; competing closure claims in behavior/system | CONFIRMED | FIND-001-3: local PublicWyrdGatewayCaller lacks the actual Vertex-provider refusal; Run's guard does not cover this public sibling. |
| BREV-5, INVREV-R2-01, REP-R2-001, MAINT-R2-001, P-SDK-01 | REVISED | FIND-001-5: one incomplete schema-language validator; use an existing standards library's offline meta-validation, not another keyword list. |
| BREV-8, P-SDK-03 | CONFIRMED | FIND-001-8: native request-body model is a second, renderable target at shared construction. |
| BREV-13, FUP-13; conflicting private-task-based closure claims | REVISED | FIND-001-13: authored persistence proof is corrected; approved three-SDK derived introspection still lacks its TypeScript projection. |
| INVREV-R2-02, MAINT-R2-002, R2-SYS-001, P-SDK-04, FUP-14 | REVISED | FIND-001-14: user-requested combined actual-loader proof remains missing; use existing public Rust loader/host seams, not a newly exposed Python host API. |
| P-SDK-05 and behavior docs proof note | CONFIRMED, consolidated | FIND-001-15: setup omits capture policy; framework tests do not read retained rows. One existing documentation/setup proof boundary closes both. |
| Missing manual narrow-width inspection | No independently observed layout defect | Keep as a verification limit and complete the already-requested inspection with the same bounded docs correction. No separate finding. |
| Historical compile-only RED / tests extended after fixes | REJECTED as a present implementation finding | Record honestly; do not manufacture a retrospective behavioral RED or rerun older code to establish blame. |
| FIND-14 denial is Card-scope rather than RBAC | REJECTED as a defect | A representative real pre-dispatch refusal was required; the Card-scope403 qualifies. Separate gateway permission tests prove RBAC. |
| Synchronous TypeScript connect example | REJECTED as a defect | Current public WyrdClient.connect is synchronous; R1's stale assumption must not force await. |
| TS introspection absence and Python host-binding absence require a new product/spec decision | REJECTED | REQ-008 already requires derived metadata on all SDKs; actual Rust Agent loader/caller/run_prompt already supplies the user's requested loader boundary. |

## FIND-001-3 — CONFIRMED — INCORRECT — Important

**Plain statement:** A local Rust Agent can still send a Vertex gateway call
through the public caller, although this revision requires local Vertex gateway
execution to refuse before IO.

**Obligations:** REQ-010/011, AC-008, explicit local-Vertex non-goal
(`spec.md:96–98,739–745,1005–1007`). Original named-target and Jev fixes remain
closed; this is their existing local/provider-boundary remainder, not a new
Vertex feature request.

**Source and reachability:** `skald-agent/src/agent.rs:339–383` accepts the
existing host caller and constructs the route registry;
`skald-agent/src/gateway.rs:78–99` keys it by the Prompt's actual provider,
including Vertex. `wyrd-client/src/gateway_caller.rs:108–128` never checks that
provider and sends it in `wyrd-prompt-provider`. `NativeCall::project:286–294`
projects GenerateContent onto public Gemini ingress. Gateway
`routing.rs:134–165` accepts Vertex Prompt → Vertex adapter, and
`adapter/mod.rs:219–222` accepts that Gemini ingress on Vertex. The resulting
authorized local call reaches actual server Vertex dispatch. The Run-only
refusal at `observe/invoke.rs:93–99` is bypassed by the public direct Rust
Agent host seam. A Google Prompt whose deployment is spelled `vertex` is a
different, correctly supported case.

**Selected correction:** put the local-only refusal on the existing
PublicWyrdGatewayCaller before projection/transport, based on typed
`call.provider == ProviderName::Vertex`. This caller is the actual local/server
trust boundary. Reuse its existing terminal BadRequest/Agent error projection;
no new public error code or route API is required. Do not put a global Vertex
guard on Agent, Prompt compilation or the route registry: the separate
ServerWyrdGatewayCaller must retain existing server/judge Vertex behavior.
Preserve arbitrary deployment spelling, native Vertex and direct-client
conversion. The existing Run guard may retain its earlier public validation
error, but it cannot substitute for the public caller's invariant.

**Focused closure:** extend
`gateway_caller::public_gateway_call_context_and_errors` in the existing
wyrd-client integration binary with actual Vertex-provider calls and no
gateway/upstream IO. Cover a direct declared route and the existing runtime
route-override seam without reconstructing provider from a deployment name.
Keep Google→deployment-named-vertex success and existing server Vertex
regressions. Exact existing caller command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(=gateway_caller::public_gateway_call_context_and_errors)'
```

If an additional direct-Agent test is added in the existing integration owner,
record its actual exact selector; do not invent an already-passing selector.

## FIND-001-5 — REVISED — INCORRECT — Important

**Plain statement:** Invalid output schemas still register because the shared
validator ignores many standard schema keywords.

**Obligations:** REQ-006/007/008/011, AC-006, R1-AC-05. This continues the same
schema-validity cause, not one finding per provider or keyword.

**Producer → consumers:** `skald-spec/src/prompt.rs:235–327` checks a short
keyword list and returns success for every other keyword at307. For example,
`{"type":"object","properties":{"n":{"type":"number","minimum":"wrong"}}}`
passes the entire walk. `$ref:7`, `minLength:-1`, `uniqueItems:"yes"`, malformed
conditional schemas and dependent-schema locations also escape. These are
invalid standard keyword values, not unknown native options. Portable compile
(`authoring.rs:361–368`), native `Prompt::new` (`prompt.rs:398–407`), ergonomic
builder finalization (`skald-prompt/src/builder.rs:313–322`) and Python native
request rederivation all converge on this owner. PromptSpec deserialization
(`wyrd-spec/src/card/prompt/spec.rs:65–78`) compiles the accepted value and does
not repair it. Standalone and inline registration therefore persist the schema;
rendering forwards it upstream, causing a late provider failure instead of the
required local authoring refusal. Existing tests cover only the implemented
subset.

**Selected minimal correction:** replace the hand-maintained schema-language
walk at the existing pure `check_output_schema` acceptance boundary with
standards-backed **offline meta-validation**, retaining the separate
destination checks (including strict OpenAI object-root), field-path errors,
authored values and unknown native options. Do not compile/fetch user `$ref`
targets, introduce a schema store, or add per-SDK/server validators.

The dependency choice is concrete and reversible: the workspace already owns
`jsonschema` with `default-features=false` (`Cargo.toml:119`), currently locked
to0.18.3. The locally installed0.46.4 exposes
fallible cached, bundled draft-specific meta-validators, for example
`jsonschema::draft202012::meta::validate(&schema)` (and corresponding built-in
older-draft modules). Its
`src/lib.rs:1510–1590,2652–2710` proves this is meta-validation, not user-schema
instance-validator compilation. Select the bundled validator for a recognized
built-in declaration, using the library's normal unversioned semantics otherwise;
an unknown/custom meta-schema is a local unsupported-schema error. Do not use
the general custom-meta retrieval path or supply a fetching registry. Modern
reference keywords have actual implementations. Its MSRV1.83 is below this
workspace's1.94. Select the normal workspace dependency update to0.46.4,
preserve disabled HTTP/file/default features, and use the fallible bundled
meta-validation API rather than the panic-returning convenience `is_valid`.
This completes the
existing JSON Schema contract; it does not add a dialect/configuration surface
or remote-reference support. Preserve built-in declared-draft handling and
currently valid unversioned/nested/local-reference schemas in proof. Do not
replace the current incomplete walker with another private keyword traversal.

Only two current consumers use the old JSONSchema type/API: server served
OpenAPI test (`pg_openapi_contract.rs:1108`) and MCP principals test
(`tests/bifrost/mcp/principals.rs:14,53–57`). Adapt those test-only calls to the
same library's current validator API as required by the dependency update;
do not relocate their behavior into foundations. The pure schema-validation
dependency belongs on skald-spec, the owner of the invariant, not a broad
shared configuration owner. No new public decision is needed for this ordinary
dependency/API update.

**Focused closure:** extend the existing shared authoring and builder tests
with malformed known numeric/string/reference keywords, invalid nodes under
`$defs`, conditional and dependent-schema keywords, portable and native forms,
and valid matching local-reference/nested schemas. Assert field-specific refusal
before mutation, no authored rewriting, normal content rendering and strict
destination behavior. Extend the existing real registration negative test for
standalone and inline no-write refusal. Commands use actual existing selectors:

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::output_schemas_are_validated)'
mise exec -- cargo nextest run --locked -p wyrd-skald-prompt --lib -E 'test(=builder::builder_tests::builders_refuse_invalid_output_schemas)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_registration_route::invalid_prompt_authoring_is_refused_before_writes)'
```

Confirm the target/features of the registration selector using the existing
mise environment owner/nextest list before execution. The source name is exact;
this report is not evidence of execution. Run only the focused served-OpenAPI
and MCP principal tests affected by the dependency adaptation; record their
actual exact selectors. Add normal dependency/codegen checks for this write set,
not a broad journey aggregate.

## FIND-001-8 — CONFIRMED — INCORRECT — Important

**Plain statement:** The native Prompt constructor validates one model string
but can still render a different model stored inside the request body.

**Obligations:** REQ-005/007/008, INV-004/007, AC-006, R1-AC-08.

**Producer → consumers:** `Prompt::new` (`skald-spec/src/prompt.rs:398–421`)
checks only its model argument. A typed OpenAI Chat request with body model
`{{target}}` plus canonical argument `fixed` succeeds; extraction at710–727
derives `target`; `bind_mut:535–543` and the full recursive replacement
at634–650 rewrite it. Rendering returns a body whose target is caller data
while Prompt.model remains `fixed`. `skald-prompt::Prompt::from_native:69–85`
wraps this exact public value. `Agent::run_prompt` renders it, and
`request_builder::request_from_conversation:120–134` explicitly preserves the
template's model; native OpenAI `send_native:453–469` serializes it unchanged.
Responses, Anthropic and Jev body-model slots share this construction cause.
Authored compilation, builders and normal Python model setters are already
correct; they cannot close this public sibling. GenerateContent's model is
URL-owned and has no such body slot.

**Selected correction:** enforce the already-declared single literal target
invariant in shared Prompt construction before derivation: validate the actual
model-bearing native request field with the existing check_model and refuse a
body model inconsistent with the canonical literal Prompt model. Keep
GenerateContent's model-less body, content variables and literal slash IDs.
Use the typed request fields already used by authoring/model insertion; no
provider-dispatch guards, model mapping/catalog, content-field exclusions or
new runtime option. Do not mutate an invalid supplied request silently to hide
two independently supplied targets. Existing request-rederivation setters and
builder finalization continue through this same owner.

**Focused closure:** extend actual existing
`authoring::tests::model_is_literal_configuration` with direct native
constructors for Chat/Responses/Messages/Jev whose body model is templated or
conflicts while the second argument is literal. Refuse before variable
derivation/provider IO; preserve matching slash models and model-less Google.
Add the missing assertion to the existing public Agent/native fixture owner
only if needed to prove dispatch cannot bypass construction; record its actual
selector, without introducing a harness.

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::model_is_literal_configuration)'
```

## FIND-001-13 — REVISED — MISSING — Important

**Plain statement:** TypeScript preserves authored Prompt JSON and hashes but
does not expose the derived Prompt variables or response schema promised for
all three SDKs.

**Obligations:** REQ-008 (`spec.md:584–608`), REQ-011, AC-004/009, R1-AC-13.
The approved text explicitly requires derived introspection on hydrated objects
in all three SDKs. R1's claim that no production accessor is needed was an
incorrect private implementation assumption, not an approved exception.

**Producer → consumers:** shared Prompt already derives variables,
media_variables and ResponseType (`skald-spec/src/prompt.rs:30–57,398–423,
657–702`). WyrdState::prompt and HydratedAgent::prompt already expose that
runtime (`wyrd-client/src/state.rs:85–151,986`). The standalone N-API projection
(`native/src/cards.rs:441–446`) throws it away via
`state.prompt(...).and_then(|_| state.card(...))`. The Agent sibling
`hydrated_agent:619–639` also emits only the stored envelope. TypeScript
`PromptCard` (`src/index.ts:2360–2373`) adds only ref; both
WyrdState::agent/prompt projections (`2737–2765`) expose no variables/media/schema
metadata. The four-form TS test correctly stops at JSON/hash; it cannot prove
an absent public capability. Rust/Python read the actual shared derived fields.
Stored authored JSON intentionally excludes these fields, so re-parsing the
stored spec in TypeScript would duplicate the compiler and is not a correction.

**Selected correction:** correct the R1 assumption and extend the existing
N-API lifecycle result projection and existing hydrated Prompt interface with
thin read-only derived metadata from that already-hydrated Rust value, on both
standalone and Agent-navigation paths. Project text variables, media variables,
response schema/name with a null/plain-text representation consistent with
existing Rust/Python metadata. Use the SDK's existing camelCase convention,
types/declarations/documentation and source-driven native generation. Preserve
registered envelope/ref/spec/hash and durable wire schema; no new endpoint,
stored duplicate field, parser, graph owner or Prompt runtime is needed. This
implements an approved SDK projection, not an expensive new API decision.

**Focused closure:** extend the existing TS four-form journey after real
register/GET/hydrate to assert shared-derived variables and schema/name for
portable/native standalone and Agent-inline navigation, including plain text
and existing portable media names. Keep authored JSON/hash assertions. Existing
Rust/Python parity assertions remain, without re-running their full journeys
solely for a TS projection edit.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/register-and-hydrate.test.ts -t '^prompt forms round trip as authored$'
```

Use normal native build prerequisites, then TS type/lint/native-declaration
and codegen checks applicable to the actual edit. No new private binding test
harness or language-specific derivation.

## FIND-001-14 — REVISED — human-directed MISSING coverage — Important

**Provenance:** the owner explicitly requested and numbered this additional
test. This review independently validates the remaining mismatch; it does not
retroactively claim the original test request was a discovered bug.

**Plain statement:** The new bundle journey runs a replacement Agent through
Run.invoke, so it does not prove the downloaded Agent's public loader retained
the configuration that its runtime then executes.

**Obligations:** explicit owner sequence, R1-AC-14, REQ-008–010/013,
INV-004/007, AC-004/005/008/011/012. Preserve the exact combined boundary.

**Producer → consumers:** installed driver
`test_local_development.py:602–759` genuinely applies/downloads the registered
bundle, uses fresh processes, retains headers/evidence and restarts the same
tenant with the same bundle. Its child `test_cli_gateway_agents:232–280`
loads WyrdState, inspects declarative Cards and calls state.run(alias).invoke.
`wyrd-client/src/observe/invoke.rs:77–110` then constructs Agent::new from the
hydrated Prompt and explicitly reapplies execution_route. It does not call
Agent::from_card/from_yaml_path. Dropping the route in the actual loader can
therefore leave this test green. This is missing proof of the owner's exact
handoff, not a new claim of stored-data corruption.

**Existing mechanism and selected correction:** correct R1's Python-only
mechanical instruction. The owner requested Agent loading, not Python
specifically. Use the public **Rust Agent loader** on each downloaded disk
Agent in the same installed-CLI journey's existing fresh-process client stage.
The existing Rust client integration target already composes WyrdState,
skald-agent, prompt, runtime, tools and PublicWyrdGatewayCaller; extend that
test owner as a subprocess of the installed driver instead of adding a new
server/harness/product command. Preserve the existing Python observed-invocation
coverage if useful, but do not count it as loader proof.

WyrdState already validates the downloaded graph and supplies its exact pinned
Prompt. Existing LocalPromptResolver/register_prompt_card
(`skald-agent/src/agent.rs:32–121`) can resolve that downloaded runtime Prompt
under its exact registered CardRef inside the isolated fresh process. Load the
disk Agent via Agent::from_yaml_path (`615–639`) and its existing tool resolver;
from_card (`674–710`) must supply the original reference/identity/route.
Inspect the actual loaded runtime/to_card/to_spec before host injection. This
is normal resolver setup from downloaded data, not reconstruction of a missing
Agent route or model. Do not resolve against original authoring inputs or
fetch a replacement Prompt over the network.

Only attach the existing PublicWyrdGatewayCaller using the normal saved
WyrdClient and existing with_subject Run/Card correlation. with_gateway_caller
(`agent.rs:339–347`) supplies host IO without overriding the durable route.
Execute that same loaded instance through existing Agent::run_prompt
(`loop_runtime.rs:202–275`), using its loaded Prompt. That supported path
already dispatches Jev once through call_once, with no general Jev loop/tools
or streaming. No new Python host binding is necessary: its current loader/run
surface lacks caller injection and uses run_with, so enlarging that API solely
because R1 selected a language would add avoidable scope. No route setter,
Agent::new replacement or provider registry override is allowed on the object
under test.

**Focused closure:** all four providers run through actual downloaded Agents
before restart and the same downloaded bundle again after restart. Keep exact
Agent/Prompt refs and runtime route/provider/model assertions, saved auth,
distinct deployment names, slash models, expected answer, actual upstream
provider credentials, no Wyrd credential forwarding, zero native-trap calls,
representative Card-scope403/no-dispatch, and actual run-correlated retained
rows. A loader route-loss regression must fail this combined stage. Preserve
existing direct-request, revocation, key-reuse/decryption and cleanup checks.

Existing outer focused selector (with its recorded native package wrapper):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
```

Record the actual exact Rust integration child selector and its one normal
build prerequisite after extending the existing owner. Do not invent a passing
selector, embed Node/Python in Rust, launch another WyrdTestServer, or require a
whole journey suite. The existing native route roundtrip test remains supporting
proof, not a substitute for this combined path.

## FIND-001-15 — CONFIRMED — INCORRECT / MISSING proof — Important

**Plain statement:** The new setup guide promises recorded gateway calls but
omits the capture-policy command required to record them.

**Obligations:** REQ-011/AC-009, user-directed docsite addition,
R1-DOC-AC-01/02. R1-DOC-AC-03's unperformed visual proof remains a separate
verification limit within the same bounded docs write set, not a layout bug.

**Producer → consumers:** published
`docs/src/content/docs/connect/set-up-the-llm-gateway.md:45–130` starts a fresh
tenant, uploads credentials/deployment, sends a request and promises a
vala.gateway.calls row. No policy is applied. Its framework sibling inherits
this setup and says each call lands in that table. But
`gateway/service.rs:967–978::capture_from(None)` returns Disabled;
`invocation.rs:895–898::facts` returns None under Disabled. Waiting longer does
not create a record for such a call. The installed CLI journey explicitly puts
`examples/gateway-setup/capture-policy.json` at
`test_local_development.py:682–683`, proving its claimed mirrored procedure is
not the published one.

The framework inference tests are real and useful:
`gateway/test_client_http_auth.py` supplies metadata capture in the backends
fixture and tests OpenAI Agents SDK/Pydantic AI inference and upstream headers.
Neither framework test reads a retained row. R1-DOC-AC-02 specifically requests
response **and retained evidence** proof for these examples; a successful
provider response alone does not establish the documentation's final success
check. Consolidate that missing proof with the same capture/setup correction,
not a new production capture owner or another finding per framework.

**Selected correction:** add the existing metadata-only capture policy and
existing `wyrd gateway capture-policy put --file ...` step before the first
documented request; explain that recording depends on it. Reuse the current
walkthrough/config/command, keep payload capture off, and leave opt-in server
defaults unchanged. Framework pages inherit one canonical setup; don't add
competing procedures. Extend their existing actual-inference tests to read
normal retained rows correlated to the tested calls and verify the expected
deployment/model/outcome/attribution rather than unrelated fixture traffic.
Reuse current bounded polling/normal Bifrost reads and refusal proof; no
tracing-only test, new harness, capture queue, default-policy change or live
paid calls. Complete the already-requested rendered sidebar/search/link/code
inspection at desktop/narrow widths in both themes when a browser is
available, and report any environment limit honestly. No unobserved layout
correction is prescribed.

**Focused closure:** drive the corrected documented fresh setup through the
existing installed-CLI selector above, then actual current framework selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_openai_agents_sdk_runs_through_the_gateway tests/integration/gateway/test_client_http_auth.py::test_pydantic_ai_agent_runs_through_the_gateway
mise run docs:check
```

Keep the existing stock-client inference/refusal checks; add only missing
retained-row assertions for their published success checks where not already
proved. Record actual exact selectors for any newly extended owner. No broad
framework/user-journey aggregate is necessary.

## Prior-finding closure and cumulative reconciliation

| Prior item | Source-validated closure assessment |
|---|---|
| FIND-001-1 | CLOSED. Permission::subsumes (`permission.rs:311–313`) preserves explicit gateway grants beside All; insert uses subsumes both ways. Fallible intersection (`954–1003`) accumulates conditional meets, checks the complete representable result, and refuses rather than widens/drops unrepresentable authority. Issuance consumes it before signing. Actual permission/exchange regressions cover both orders, mixed representable and empty cases. No new empty-intersection policy is inferred. |
| FIND-001-2 | CLOSED at original lost-route cause. Agent::from_card retains route, to_spec emits it, route_registry chooses before runtime IO; both run entry owners reuse it; missing caller cannot fall through to native. Concurrent route overrides remain invocation-local. The missing combined actual-loader proof is tracked only by human14. |
| FIND-001-3 | PARTIAL. NativeCall now keeps addressed model for Anthropic/Jev and full deployment/model Google path; Jev run_prompt uses existing call_once. Local actual Vertex-provider bypass remains above. |
| FIND-001-4 | CLOSED. Portable compiler retains media_kinds/conflict refusal; bind_media_mut checks expected kind before source conversion/mutation. Native placeholders remain deliberately untyped. |
| FIND-001-5 | OPEN as above. A validator comment acknowledging a partial structural walk cannot waive complete approved schema validity. |
| FIND-001-6 | CLOSED. Typed Prompt provider propagates through public header/server request to CallPlan::require_compatible_initial; requested-target check preserves direct-client translation and explicit fallback conversion. Header supplies no permission by itself. |
| FIND-001-7 | CLOSED. batches::list derives Granted exclusions plus explicit deployment/model grants from one snapshot before bounded SQL LIMIT. BATCHES_SQL combines these reach predicates, cursor and limit; ordinary audited decide uses the same snapshot afterward. No unbounded scan or second authority added. |
| FIND-001-8 | PARTIAL. Authored canonical model/builders are literal; direct constructor's body-model sibling remains above. |
| FIND-001-9 | CLOSED. engine::AttemptRecord carries admitted provider_name(candidate) separate from deployment; live and retained span mapping share that fact, including fallback. Capture does not reread mutable deployment spelling/config. Focused distinct-deployment/provider assertions cover original counterexample. |
| FIND-001-10 | CLOSED. invocation::admit_call always authorizes its own owned snapshot (`482–486`); stale authorized boolean is gone. Early batch-create check still protects claims. settle/replay reuse normal admission; undispatched refusal releases fresh claim, ambiguous dispatch retains its existing fence. Deterministic interleaving test and release/replay regressions support closure. |
| FIND-001-11 | CLOSED for diagnosed payload boundary. check_media_source (`prompt.rs:1366–1458`) decodes/bounds inline data using existing20MiB budget before replacement, checks kind/MIME/provider restrictions and Google file URI. Literal portable lowering and runtime binding converge here; no upload/fetch service added. |
| FIND-001-12 | CLOSED. Independent standards inventory covers original103 functions plus module/associated/constants/Panics and additional changed symbols; in-place rustdoc inspected at changed owners now states invariants/errors. No remaining missing-item claim is proposed. No docs-only round or optional structural cleanup is required. |
| FIND-001-13 | PARTIAL. Four authored forms' server GET/hydrated JSON/hash preservation is proved in Rust/Python/TS; Rust/Python derived metadata is also checked. Only approved TS derived projection/proof remains above. No storage/hash corruption is asserted. |
| FIND-001-14 | PARTIAL, owner-directed. Real setup/register/download/restart/auth/header/evidence stage exists, but it bypasses actual Agent loader as above. Card-scope403 is a valid representative denial; not proof of role-based denial. |

The cumulative implementation continues to use existing PermissionSet/issuer,
Prompt compiler/binder, Agent/route registry, Public/Server gateway callers,
Cards graph hydration, snapshot/admission, TenantConn SQL and ScribeOutbox.
These owners cover the required outcomes; no replacement graph loader,
transport, orchestration loop, credential store, durable queue or batch
authorization cache is justified. Approved Workflow removal remains complete;
independent Eval DAG/observation vocabulary is not a surviving Workflow Card.
Native capabilities, tenant isolation, credential sealing/endpoint screening,
nonblocking audit, accounted settlement and admitted snapshot lifecycle remain
preserved. Adjacent approved changes are not findings.

| Reconciled obligation | Result / exact remaining reason |
|---|---|
| REQ-001–004,012–013 | PASS for original behavior; human14 adds required combined loader proof to013 acceptance. |
| REQ-005 | FAIL: native effective body-model invariant, FIND8. |
| REQ-006–007 | FAIL: shared schema validity, FIND5; supported content/settings/media/mappings otherwise pass. |
| REQ-008 | FAIL: schema acceptance and TS derived metadata, FIND5/13; authored persistence itself passes. |
| REQ-009 | Declared-route implementation PASS; combined public loader proof remains human14. Do not infer all Python host binding behavior is newly required. |
| REQ-010 | FAIL: public local Vertex refusal, FIND3; named ingress/defaults/override/initial compatibility otherwise pass. |
| REQ-011 | FAIL: remaining validation/TS projection/published evidence issues, FIND5/13/15. |
| INV-001–003,005–006 | PASS in reviewed cumulative source and scoped evidence. |
| INV-004/007 | FAIL only for effective native model invariant (FIND8); runtime identities/overrides/native fail-closed otherwise supported. |
| AC-001–003,007,010 | PASS in reviewed source/recorded proof. |
| AC-004 | FAIL: required TS derived introspection FIND13. |
| AC-005 | Original route behavior PASS; explicit owner14 combined loader proof incomplete. |
| AC-006 | FAIL: schema/body-model acceptance FIND5/8. |
| AC-008 | FAIL: actual local Vertex provider guard FIND3; other protocol projection and Jev single-call proof pass. |
| AC-009 | FAIL: TS/docs completion FIND13/15. |
| AC-011–012 | Original installed setup/restart/list behavior PASS; extended owner14 actual-loader stage incomplete. |
| R1-DOC-AC-01/02 | FAIL: canonical setup omits capture; framework retained-row proof missing, FIND15. |
| R1-DOC-AC-03 | Structural/index/check evidence available; manual narrow-width/both-theme inspection not established. No layout defect inferred. Complete same docs verification; do not claim it passed. |

## Verification limits and correction scope

Reported final-head format/lint/codegen/dependency/typing/OpenAPI/docs and focused
test passes are supplied evidence, checked against test bodies/selectors where
relevant, not new execution by this validator. Missing historic behavior RED
does not alone negate current source or current green proof. No live provider,
release runner, aggregate gate, artificial load or old-baseline rerun occurred.
The browser entry was unavailable to the orchestrator; automated accessibility
does not independently establish human visual inspection.

Corrections are bounded to existing acceptance owners and the smallest focused
tests. Dependency migration has two existing test-only consumers; SDK metadata
is a projection of already-derived Rust data; loader proof uses existing host
seams; docs capture uses an existing opt-in policy. Required static checks follow
the actual write set. Do not prescribe whole journey suites, broad aggregates,
new validation infrastructure or an optional refactor. No production action,
merge, push or deployment is authorized by this ledger.
