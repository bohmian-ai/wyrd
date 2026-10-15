---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-deployment-addressing
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012]
depends_on: []
parent_task: null
remediates: []
skills: [wyrd-implement]
---

# TASK-001 Governed Agent execution from simple gateway setup

## Outcome and Value

From a clean local development setup, a tenant can upload existing provider
credential files, configure one or four named deployments with the same CLI
command, author portable or native Prompts once, and execute Agents through
explicit native/gateway routes with authorized pre-run overrides. Captured
results identify the actual target. Workflow is removed as a product, while
Agent, gateway, and verification behavior survives.

This is the single implementation task explicitly requested by the owner. It
owns the complete cross-boundary contract migration and all consumer closure;
its ordered scenarios are implementation sequence, not independently approved
subtasks. All REQ-001–REQ-013, INV-001–INV-007, and AC-001–AC-012 belong here.
It replaces this path's old r1-only task and historical WIP-port instructions.
The approved r5 spec, not those patches or the separate prompt-authoring
branch's draft/task, is authority.

Use `$wyrd-implement`. If editing the Wyrd UI, also use repository `$wyrd-ui`.
Normal private implementation choices remain implementation-owned. No Git
staging, commit, merge, release, or production deployment is authorized by
this task.

## Owners, Scope, Consumers, and Prohibited Changes

### Source-backed reuse and gap map

Paths identify inspected ownership, not a private module/file allowlist. Read
current source again before changing it; discoveries that preserve r5 may
adjust private locations without another plan.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Deployment shape and target planning | `wyrd-spec/src/gateway/deployment.rs::ProviderDeployment`, `ProviderAdapter::serves`; `wyrd-gateway/src/routing.rs::CallPlan::{new,pin}` | Gateway routing tests; Rust SDK `gateway_admin.rs`, `gateway_inference.rs`; CLI gateway journey | Required single model/capabilities, weights, reserved identities, provider-selected targets | Extend existing contracts/snapshot/planner to resolve one deployment by name, optional allowlists/capabilities, open/granted access | No new routing engine or weighted-selection replacement |
| Credential lifecycle and tenant persistence | `wyrd-gateway/src/credential.rs::CredentialResolver::resolve`; server `GatewayAdministration::put_deployment`; SQL `GatewayDeploymentWrite`, `upsert_gateway_deployment`, `gateway_snapshot` | Credential resolver tests; gateway admin SDK/CLI tests; defining `20260924000000_gateway_administration.sql` | Provider matching depends on removed deployment model; persisted row/index/FK assumes single model | Retain credential sources, keyring, RLS and locking; adapt deployment metadata/joins without equating provider and deployment name | No new credential writer, secret source, vault abstraction, or persistence service |
| Invocation, visibility and audit | Server `gateway/invocation.rs::GatewayInvocation::{models,decide}`; `gateway/batches.rs::invocable` | Rust SDK gateway inference/admin journeys; Python/TS gateway admin/inference tests | Context-free All/provider coverage; batch All bypass; discovery enumerates single-model deployments | Decide against resolved deployment access policy and effective subject; share authoritative coverage for requests, fallback and visibility | Existing permission evaluator, Scribe outbox and gateway admission remain owners |
| External endpoint forwarding | `ProviderAdapter::OpenAiCompatible`; `wyrd-gateway/src/adapter/http.rs::HttpProviderDispatch`; `CredentialResolver` | Gateway adapter tests and compatible/native ingress journey owners; SDK upstream-header assertions | Arbitrary deployment naming/new model identity must survive forwarding | Extend existing adapter path; strip only address deployment prefix; preserve model, auth injection, SSRF/pinning and header protections | No model mapping, alias API, external Agent bypass, or new transport |
| Portable/native Prompt construction | `skald-spec::PromptDraft::compile`, `Prompt::{render,bind_media}`, `MediaRef`, `MediaSource`; `skald-prompt/src/builder.rs::{openai_chat,openai_responses,anthropic,gemini,vertex}` | `authoring.rs` tests; builder tests; Python Prompt authoring/media-binding tests | Compiles early to text/native form; model/metadata duplicated; builder duplicates assembly; portable media/settings/schema insufficient | Share pure authored-domain behavior and existing native wire/media/output helpers; dispatch compilation for both forms | No second compiler/render engine or SDK-specific provider assembly |
| Prompt persistence and references | `wyrd-spec/src/card/prompt/spec.rs::PromptSpec::{deserialize,content_hash}`; Agent `prompt: InlineableRef<Prompt>`; shared `Cards` and loader | Prompt codec/hash tests; Rust `register_and_hydrate.rs`; local loading tests | Authored fields lost at decode; derived fields stored; old native-only inline form | Store/hash either authored form and derive introspection; use existing registration/hydration/reference path | No new registry, graph loader, traversal, reference wrapper, or Card hash owner |
| Agent route and invocation override | `card/agent.rs::AgentSpec`; `skald-agent::Agent::{with_provider_registry,run_prompt}` and its bounded loop | Agent construction, loop, callback, timeout, structured-output integration modules; shared-client `Run::invoke` | No serialized route; observed invocation forces gateway; no isolated pre-run override | Extend existing Agent/value/handle seams with required route and per-invocation effective route; retain loop/resources and authorization | No replacement Agent runtime or mutable process-global route setting |
| Public/server gateway caller and judges | `skald-workflow/src/route.rs::{wyrd_gateway_registry,WyrdGatewayCaller,WyrdGatewayCall}`; shared `PublicWyrdGatewayCaller`; server `GatewayWorkflowCaller`; `SkaldJudgeInvoker::{agent_for,invoke}` | Rust gateway inference and verify-in-real-time journeys; Python/TS gateway inference and verification tests | Reusable routing owned by Workflow; derives target from Prompt provider; prefix loss; judge route ignored | Preserve complete gateway-backed Agent execution at surviving Agent/runtime/client/server owners before Workflow deletion | Relocation justified by current dependency on a removed owner; no new HTTP stack, judge engine or evaluation scheduler |
| Workflow product removal | `envelope.rs::{CardKind,Spec}`; `card/workflow.rs::WorkflowSpec`; `OperatorAction::Workflow`; shared `Workflow`, `WorkflowCards`; server `workflow_runs_router` | CLI Workflow journey; Rust SDK Workflow loading/inference consumers; server served OpenAPI; SDK/UI kind readers | Retired kind/runtime/jobs/config/permissions/reference/surface wiring remains reachable | Remove entire product and dependent public entries; keep independent evaluation DAG/observation contracts and reusable execution | Deletion, not a replacement DAG/job platform or Agent hosting API |
| Single/list deployment file | `wyrd-cli/src/gateway.rs::GatewayCommand::dispatch`, `read_document`; shared `Gateway::put_deployment` | CLI `gateway_server_journey.rs`; SDK admin tests | CLI reads one object only | Extend existing command input to object/nonempty list, whole-file validation and truthful per-item outcomes using existing PUT | No combined credential import, generic batch API or cross-request transaction |
| Installed dev/tenant bootstrap | `wyrd-cli/src/server/dev.rs::{DevArgs::run,DevServer::run}` and existing saved-client/sealing lifecycle | CLI `tests/pg_tests.rs`; Python installed-client bootstrap driver; TS local-development saved-config test; `test:local-dev:journey` | No short journey spanning saved auth through four-provider deployment list and inference/evidence | Extend existing installed-CLI/managed-Postgres/fresh-process journeys and readable example docs | No second server launcher/bootstrap protocol; existing one command already provisions tenant |
| Language/headless projections | Shared `wyrd-client`; SDK wrappers/stubs/generated card types; CLI, MCP gateway catalog and served OpenAPI registrations | Python/TS gateway admin/inference modules, MCP gateway journey, `pg_openapi_contract.rs`; Cargo metadata and mise task bodies | All surfaces still project old deployment/Prompt/Agent/Workflow contracts | Update owner projections/generators and runtime catalogs; keep first-class client/runtime boundaries | No durable behavior in bindings; new/materially relocated PyO3 stays in Python SDK |

### Fixed boundaries and exclusions

- Follow r5's exact public fields, provider/format pairs, eight portable
  settings, text/image/document core, response schema rules and batch behavior.
- Required Agent `execution_route` is its default; optional explicit pre-run
  replacement is invocation-local, authorized and recorded. Persisted route
  edits follow Card versioning; overrides do not rewrite the Card/hash.
- Retain all existing credential upload sources and automatic upstream header
  injection. Do not add environment interpolation, credential-list files,
  automatic credential creation, model renaming, aliases or external bypasses.
- No new local Vertex gateway support. Preserve existing native/server Vertex
  behavior and explicit local refusal; never silently choose Gemini/native.
- Remove Workflow product surfaces entirely, preserving unrelated evaluation
  DAGs, observed-data JSONPath fields, Agent loop semantics and verification.
  Do not infer that a name containing workflow denotes a Workflow Card.
- No new server Agent job API, provider discovery, production testing seams,
  unnecessary Cargo features/dependencies, or generalized infrastructure.
- Tenant SQL remains TenantConn/RLS; caller owns transactions. Retain
  credential-provider/host assignment, managed-secret binding, authorization
  before IO, effective subject/actor distinction, audit outbox, and SSRF
  resolve/screen/pin behavior.
- Update the named architecture authority and kind inventories cohesively
  before or with changed contracts. Do not retain an unexplained exception to
  the old 15-kind or Workflow route prose.

## Approach

1. Reconcile the approved Prompt/Agent/deployment/14-kind target with authority
   prose and inspect the live owners before changing code. Establish focused
   failing behavioral tests and keep the acceptance map current.
2. Extend pure contracts and shared Prompt validation/render/compilation for
   both authored forms, portable types/settings, derived metadata and errors;
   close loader/hash/inline/reference consumers through existing owners.
3. Complete named gateway persistence/planning, access-aware authorization,
   listing, credential binding and external forwarding; extend the existing
   deployment command to single/list input without changing credential setup.
4. Extend the existing Agent execution seam with declared/effective routes and
   isolated pre-run overrides; preserve public/server transport, tool-loop,
   cancellation and judge authority/output behavior.
5. Remove Workflow end to end after preserving reusable execution; close all
   first-class SDK, CLI/MCP/UI, generated-contract and example consumers.
6. Prove the installed dev-to-tenant-to-four-provider journey with the actual
   command/config flow and a fresh TS client; close negative/restart/replay
   paths and the remaining language journeys with controlled upstreams.
7. Run focused verification and static/type/generated/boundary proof, inspect
   the full changed surface, and record requirement/evidence closure for
   independent task and integrated-change review.

## Ordered Implementation Scenarios

This mixed task requires TDD for new/changed executable behavior. Execute one
scenario at a time: run its focused RED, implement the minimum GREEN, then
REFACTOR and rerun earlier affected proofs. Test IDs refer to the exact
commands below. Planned tests do not exist yet; add them at the inspected
owner and confirm nonempty discovery before claiming RED/GREEN. Compilation
or environment failure alone is not the intended behavioral RED.

### Scenario 1 — Authored contracts and required Agent route

**Behavior.** Both Prompt forms use provider/model once and reject duplicate,
removed/mixed fields, invalid pairs and invalid schemas. Agent requires one
native/gateway route; no Prompt deployment/route or Agent llm_route/ext_gateway
alias. REQ-005, REQ-007–REQ-009, REQ-011; AC-004, AC-006.

**RED.** Add T01 at the existing Agent contract test owner to assert both
Prompt forms, inline/reference projection and required-route refusal. It must
fail because the current Agent rejects execution_route and Prompt decoder
compiles the wrong form, not merely because a new symbol is absent.

**GREEN.** Extend existing pure contracts/codecs/error projections for the
approved shapes and known-pair validation; keep storage authored and host
resources outside specs. Start closing callers so the behavior can execute.

**REFACTOR.** Consolidate duplicate shape/identity validation through existing
owners without private utility structs or parallel error catalogs. Rerun T01.

### Scenario 2 — Portable multimodal/settings/schema compilation

**Behavior.** Text/image/document blocks and bindings retain order and map
through supported provider formats; all eight settings and structured output
map as specified, with explicit unsupported cases and no secret/media IO at
construction. Audio/video remain native. REQ-006–REQ-008; AC-006.

**RED.** Add T02 at the existing pure authoring test owner with table-driven
valid/invalid mappings, missing/mismatched media, source/MIME restrictions,
slash-containing models and deterministic response schema name. Current
text-only draft compilation must fail these assertions.

**GREEN.** Reuse native wire/media helpers and existing output-schema
conversions in one shared Rust construction path. Preserve source/option
semantics and output-schema derivation; reject unsupported settings/blocks
rather than silently dropping or fetching/converting them.

**REFACTOR.** Remove duplicate builder/provider assembly, retaining native
options and derived introspection. Rerun T01–T02 and affected native Prompt
behavior; record exact commands for any additional named regression tests.

### Scenario 3 — Named deployment resolution and persistence

**Behavior.** Deployment prefix selects one target; optional model/capability
lists narrow access; weights and old single-model shape disappear; price,
fallback and governance use deployment/model identities. Preserve model
suffixes, credential-provider identity, SQL tenancy/FKs and snapshot behavior.
REQ-001, REQ-003; INV-001–INV-003; AC-001.

**RED.** Add T03 at the existing CallPlan test owner: independent same-adapter
targets, defaults/allowlists, missing target/capability, slash suffix and pinning
of provider-held state. Assert the new contract's behavior against the current
weighted/model-matching planner. Use T04 to cross the real persistence/API seam.

**GREEN.** Extend existing deployment/snapshot/planning and SQL owners;
change the approved unshipped defining schema as needed without replacing
credential sealing/locking or widening tenant access.

**REFACTOR.** Delete unreachable weight/single-model machinery and replace its
obsolete expectations with new behavioral assertions. Rerun T03–T04 and T17.

### Scenario 4 — Subject-authorized open/granted requests and visibility

**Behavior.** All-only reaches open, not granted; deployment/exact-model grants
cover their permitted targets. Requested-target denial precedes IO; denied
fallbacks are removed and audited. Discovery/batches respect the same policy
under tenant isolation and effective subject authority. REQ-002; AC-002.

**RED.** Extend T04 as a real SDK/server journey with scoped callers, fallback,
listings and zero-upstream-call assertions; current All/provider coverage and
batch visibility must fail. Supporting coverage-table tests may isolate the
pure decision but cannot replace the journey.

**GREEN.** Use resolved deployment policy at the authoritative invoke decision
and its listing/fallback callers. Retain actor/SYSTEM attribution without
privilege, per-evaluation audit staging and nonblocking outbox behavior.

**REFACTOR.** Remove duplicate/context-free coverage and shared-list bypasses
without weakening management permissions. Rerun T03–T04 and later subject/judge
proof whenever this shared authorization changes.

### Scenario 5 — External deployments reuse credentials and preserve model

**Behavior.** A named OpenAI-compatible deployment resolves the uploaded
credential and injects the configured upstream header, preserves the model,
and never forwards Wyrd credentials. Revocation/assignment/unsafe destination
refuses before upstream IO. REQ-003–REQ-004; INV-001–INV-002; AC-003.

**RED.** Add T05 at the existing gateway inference journey owner with a
controlled endpoint and managed/operator-backed credential cases. Arbitrary
new deployment naming and removal of model/provider coupling expose the gap.
Keep already-correct security branches as regression/verification-only proof.

**GREEN.** Extend current adapter/resolver/server administration paths for the
new identity shape, preserving keyring/provider/host binding, secret redaction,
reserved headers, and screened/pinned transport. Add no model mapping field.

**REFACTOR.** Reuse existing authenticated dispatch and remove only stale
identity assumptions. Rerun T03–T05; do not write another HTTP/secret client.

### Scenario 6 — One command accepts one or multiple deployments

**Behavior.** The same deployment put command reads one object or nonempty
list; validates the whole document/duplicate names before writes; applies
existing per-item PUT; reports ordered progress on server failure; reruns
converge without deleting other names or rotating keys. REQ-003; AC-012.

**RED.** Add T06 through the compiled CLI with file/stdin input, malformed/
empty/duplicate lists, denied/missing-credential later item, progress and
rerun assertions. Current single-object decoder must fail list behavior.

**GREEN.** Extend existing CLI input and existing Gateway PUT lifecycle only.
Keep credential uploads/header injection unchanged and reflect partial
commits truthfully instead of adding cross-request transaction promises.

**REFACTOR.** Share current decoding/redaction/output conventions, remove
unnecessary command variations, and rerun T04–T06.

### Scenario 7 — Registered and inline Prompts preserve authored form

**Behavior.** Both Prompt forms register/GET/hydrate with authored fields,
values and hash semantics unchanged; derived variables/schema remain
available; standalone/inline content and local loader references agree.
REQ-005–REQ-008, REQ-011; AC-004, AC-006.

**RED.** Add T07 through real Rust Cards/server round trips. Current
compile-on-deserialize/storage behavior must change authored portable input
and fail. Include user-facing removed-field refusal before persistence.

**GREEN.** Close existing Card/hash/loader/hydration and SDK construction
paths around the shared authored-domain implementation, without new
registration/hydration services or duplicate references.

**REFACTOR.** Delete superseded authored metadata/decoder reconciliation only
when unreachable; preserve non-Prompt runtime capabilities. Rerun T01–T02,
T07, and later Python/TS parity proofs when shared contracts change.

### Scenario 8 — Effective Agent route and authorized pre-run override

**Behavior.** The same Prompt executes native, named Wyrd, or external
onboarded deployment via Agent route. Overrides replace one run's route,
keep Card/hash unchanged, isolate concurrent runs, obey subject grants and
record default/effective/actual targets. Native override is refused in server
contexts. REQ-009–REQ-010; INV-004–INV-007; AC-005.

**RED.** Add T08 at the real SDK gateway journey owner and supporting T09 at
the existing bounded Agent loop owner. Current Agent/Run.invoke routing has
no such effective-route contract and must fail target/override assertions.

**GREEN.** Extend the existing Agent execution and gateway-backed provider
seam with approved per-invocation route selection. Preserve caller identity,
correlation, bounded tool iterations, sessions, callbacks, timeout/cancellation
and captured authority. Do not mutate the shared Agent/global provider state.

**REFACTOR.** Keep one execution loop and shared transport; consolidate
explicit dependency retention at the surviving owner. Rerun T07–T09 and T04.

### Scenario 9 — Native ingress retains arbitrary deployment targets

**Behavior.** Anthropic/Gemini/Jev public ingress retains named deployments;
Google dialect follows adapter; supported native options are preserved.
Existing native/server Vertex survives and local Vertex gateway refuses before
IO, including overrides. No implicit conversion/route fallback. REQ-010;
AC-008.

**RED.** Add T10 at the current gateway inference journey owner with
non-reserved deployment names and native clients. Current prefix stripping/
reconstruction must fail resolved-target assertions. Vertex local refusal is
retained regression proof, not a manufactured new failure.

**GREEN.** Extend existing request projections/ingress resolution and
capability checks. Preserve supported direct-client translation and explicit
unsupported tool/streaming behavior; do not add local Vertex ingress.

**REFACTOR.** Remove prefix heuristics and duplicated request assembly while
retaining typed provider errors. Rerun T05, T08 and T10.

### Scenario 10 — LLM judges use the same Prompt/Agent route contract

**Behavior.** Portable/native judge Prompts return parsed structured output
and dependent assertions evaluate it. Accepted subject authority and SYSTEM
actor attribution survive; no forced native/server credentials or separate
judge schema/compiler. REQ-008–REQ-011; AC-007.

**RED.** Add T11 at the existing real-time verification journey owner with
both forms, explicit judge Agent route and denied target. Current stored
response metadata/provider-derived gateway setup must fail the new contract.

**GREEN.** Extend SkaldJudgeInvoker through the existing Agent and shared
compiler/transport. Preserve constrained judge tools/turns, result parsing,
accepted authority and nonblocking result/audit paths.

**REFACTOR.** Remove remaining judge-only Prompt/schema/target inference and
reuse surviving Agent primitives. Rerun T02, T04, T08 and T11.

### Scenario 11 — Workflow disappears without losing surviving behavior

**Behavior.** Workflow kind, orchestration/jobs/routes/config/grants/
reference/Service/Operator/SDK/CLI/UI surfaces are removed. Existing Agent,
gateway and verification behavior continues independently; evaluation DAGs
and unrelated observed-payload fields remain unchanged. REQ-012; AC-010.

**RED.** Add T12 against real registration/served OpenAPI/retired HTTP paths,
plus current typed kind/reference validation. Current Workflow registration
and routes must fail the removal assertions. Do not simulate removal by
hiding schema names while runtime routes remain reachable.

**GREEN.** Preserve and adapt reusable routing/caller capabilities before
removing their Workflow owner; remove retired surfaces/unused grants and
wiring end to end. Change current architecture and kind inventories to 14.
Use direct Agent examples, not a replacement orchestration API.

**REFACTOR.** Delete unreachable product-specific code/checks with evidence
of lost reachability; preserve or relocate tests of live behavior. Rerun
T07–T12, language projections and T16 after catalog/schema changes.

### Scenario 12 — Three-language and headless contract closure

**Behavior.** Rust/Python/TS load, register, inspect and execute the approved
forms/routes with matching errors, target evidence, media/settings semantics
and no duplicate durable logic. MCP runtime catalog and served document match
actual routes. REQ-011; AC-004–AC-009; INV-005.

**RED.** Add T14–T15 at existing Python/TS gateway inference owners using
public imports, real server and both Prompt forms. Update T16's real MCP
administration contract. Their current fields/exports or runtime projections
must fail the new API behavior. Run the earlier Rust proofs as counterpart.

**GREEN.** Close authored Python stubs/exports and boundary wrappers, TS/Rust
projections, SDK helpers and MCP handlers through shared owners. Regenerate
outputs, never hand-edit generated types. Keep foreign runtime lifetimes in
Python/Node tests and the production wheel free of test harness features.

**REFACTOR.** Remove duplicated binding-side validation and provider assembly;
keep idiomatic helpers only. Rerun T07–T08 and T14–T16 with generated/type checks.

### Scenario 13 — Easy dev bootstrap to four working gateway deployments

**Behavior.** One dev command starts the actual server and initial tenant;
fresh saved-config TS client authenticates; ordinary credential files plus
one four-entry deployment file configure all adapters; real requests and
retained evidence prove correct targets/headers. Restart reuses tenant and
sealing keys. REQ-013; AC-011–AC-012.

**RED.** Add T13 in the existing installed-CLI Python driver with fresh TS
client execution, following the checked-in walkthrough. It must fail at
single/list setup or new target behavior, not missing downloads/credentials.
Use the existing installed-artifact and managed-Postgres setup, not direct
tenant inserts or an in-process replacement for the tested CLI startup.

**GREEN.** Extend existing dev/CLI/client fixture owners and sample config to
run the same visible commands. Assert readiness, fresh saved auth, all four
upstreams, evidence, restart, rerun and truthful negative outcomes. Include
this scenario in the existing local-dev lane selection. Local provider
fixtures supply keys and protocol replies without production policy bypass.

**REFACTOR.** Keep the walkthrough short and reuse lifecycle/cleanup rather
than create another launcher. Rerun T06, T13 and T14–T15; ensure the TS child
has no test-only direct admin-key constructor or inherited stale credentials.

## Acceptance Criteria

| Spec criterion | Owner and required closure | Primary proof |
|---|---|---|
| AC-001 | Gateway contracts/snapshot/SQL/planner: optional lists, named same-adapter targets, no weight | T03–T04 plus relevant three-SDK assertions |
| AC-002 | Gateway subject authorization, per-fallback audit, discovery/batch visibility and tenant isolation | T04, T08, T11 |
| AC-003 | Existing external adapter/credentials: unchanged model, protected headers, revocation and network safety | T05, T13 |
| AC-004 | Prompt/Card/hash/loader and SDK owners: standalone/inline authored round trips and derived introspection | T01–T02, T07, T14–T15 |
| AC-005 | Agent/runtime/client: defaults, pre-run override, authorization, loop/cancellation, isolation/evidence | T08–T09, T14–T15 |
| AC-006 | Shared compiler and validation: native/portable forms, multimodal/settings/schema matrix and stable negative errors | T01–T02, T07, T14–T15 |
| AC-007 | Judge invocation/verification: structured output, dependent verdict, accepted subject and SYSTEM attribution | T11 with language verification regressions retained |
| AC-008 | Existing native ingress and public/server callers: exact target/adapter, no new local Vertex | T10, existing native/server Vertex regression coverage |
| AC-009 | Authority/docs/examples plus generated/SDK/CLI/MCP/OpenAPI owners | T12, T14–T16 and static/type/generated proof |
| AC-010 | Workflow product owners and all enum/ref/surface consumers: complete removal, live execution preserved | T12; full write-set/dependency/catalog inspection |
| AC-011 | Installed CLI bootstrap, ordinary credentials, list setup, fresh TS client/evidence, readable walkthrough | T13 |
| AC-012 | CLI input/progress and dev lifecycle: validate before writes, partial failure, convergence and restart | T06, T13 |

INV-001–INV-002 are exercised across scoped callers/credentials and T04–T05,
T13. INV-003 is checked by rejected old inputs, T01/T06/T12 and contract
inspection. INV-004/INV-007 require T07–T09 and evidence checks. INV-005 is
proved by shared-owner inspection, language tests and dependency checks.
INV-006 retains existing outbox failure behavior with regression proof and
source inspection; do not invent another writer/queue. All IDs must have a
PASS/evidence row in the implementation report; no unverified acceptance
criterion may be called complete.

## Expected Write Set and Consumer Closure

- Authority: `AGENTS.md` kind inventories, agent rules/references where needed,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, owning docs.
- Pure contracts: `crates/wyrd-spec` gateway/auth/Card/error/schema generation,
  `crates/skald/skald-spec` authored content/media/settings/format behavior.
- Runtime: existing Skald Prompt, Agent, provider/dispatch owners; move live
  execution out of `skald-workflow` before removing that retired capability.
- Server/persistence: gateway administration/invocation/ingress/batches/capture,
  tenant SQL and defining migration, server Workflow route/job removal, Vala
  judge integration and shared-client route consumers.
- Surfaces: shared client/loader/testing seams; Rust/Python/TS SDK packages,
  authored Python typing/exports, CLI/MCP/UI readers, generated type/error
  sources and regenerated consumed outputs. UI work uses `$wyrd-ui`.
- Consumers: all standalone/inline Prompt and Agent fixtures, direct Agent
  examples, support-desk, existing verification/judge examples, the simple
  installed dev/gateway walkthrough. Remove Workflow-only examples/wiring.
- Tests/config: focused existing owner targets plus necessary adjacent tests;
  existing mise lane selections/build/task/dependency wiring affected by
  Workflow retirement and new installed-CLI scenario.

This is not a file allowlist or a requirement to edit every listed area when
already correct. Do not edit other worktrees or the external example repo.
Do not restore historical WIP APIs, edit generated outputs by hand, or add
new registry/graph/transport/runtime machinery without proving the existing
complete owner cannot satisfy r5.

## Verification and Evidence

### Focused commands and test ownership

Cargo metadata was inspected with `--locked --no-deps`: current packages/
targets below exist, with the named integration/module owners inspected in
source. **T01–T15 are planned tests**, not tests claimed to exist or pass.
T16–T17 are existing regression owners. Add planned tests through RED, verify
nonempty test discovery and exact names in source/runner listing, and run
these exact selectors. If a private owner/module is relocated, record the
replacement exact selector before executing it; retain its assertions and
runtime tier. Do not treat a zero-test run as proof.

Run commands from repository root through mise. Postgres wrapper owns its
lifecycle; default crate features are sufficient for these tests. Never use
all-features for task tests. Use the normal shared Python/TS setup/build lanes
before their focused tests and installed-CLI build prerequisites before T13;
no new public Cargo feature is authorized.

**T01 — pure Prompt/Agent contracts, planned**

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::agent::agent_spec_tests::agent_prompt_execution_contract)'
```

**T02 — shared compiler/media/settings/schema, planned**

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=authoring::tests::portable_content_settings_and_schema)'
```

**T03 — named target planning/allowlists, planned**

```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=routing::tests::named_target_and_allowlist_resolution)'
```

**T04 — persisted deployment/access/visibility journey, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_admin::deployment_invocation_access_and_visibility)'
```

**T05 — external forwarding/credential/security journey, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::external_deployment_preserves_model_and_credentials)'
```

**T06 — compiled CLI single/list setup and failure, planned**

```bash
mise exec -- env WYRD_CLI_E2E=1 scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all -E 'test(=gateway_server_journey::deployment_file_accepts_one_or_many)'
```

**T07 — registered/inline authored Prompt round trip, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=register_and_hydrate::prompt_forms_round_trip_as_authored)'
```

**T08 — real Agent routes/pre-run overrides, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::agent_routes_and_pre_run_overrides)'
```

**T09 — bounded Agent loop/isolation supporting proof, planned**

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(=agent_journey::execution_route_isolated_through_tool_loop)'
```

**T10 — named native ingress and deferred Vertex, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=gateway_inference::native_ingress_preserves_named_deployments)'
```

**T11 — Agent judge/structured verification/authority, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::judge_prompt_forms_use_agent_execution_route)'
```

**T12 — served contracts and retired Workflow behavior, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -P journey --run-ignored=all -E 'test(=pg_openapi_contract::agent_contracts_replace_workflow)'
```

**T13 — installed dev/tenant/list setup and fresh TS client, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
```

Use the existing installed CLI/server artifact preparation: `mise exec --
cargo build --locked -p wyrd-cli -p wyrd-server --bins --tests`. Extend the
existing installed-client driver/selected local-dev lane for this test rather
than run a TS child without its required saved bootstrap environment.

**T14 — Python public contract journey, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_gateway_inference.py::test_agent_prompt_routes
```

**T15 — TypeScript public contract journey, planned**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/gateway-inference.test.ts -t '^agent prompt routes$'
```

**T16 — current MCP runtime catalog/administration regression, extended**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=gateway::pg_tests::agent_administers_redacted_gateway_configuration)'
```

**T17 — defining migration/idempotence regression, existing**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_migration::pg_tests::migrations_apply_and_are_idempotent)'
```

These entry points are minimum focused obligations, not limits on meaningful
cases. Tables/parameterized cases may exercise the listed matrices. Add
supporting negative/loop/regression tests when a single entry would obscure
failure localization, recording each new named test's exact focused command.
Retain three-runtime journey closure; do not substitute T09 for T08 or T14–T15.
Before diagnosing any failing run, rerun with WYRD_LOG/RUST_LOG tracing and
read its trace; a missing test subscriber is a harness gap to fix.

### Non-TDD and owner-check proof

Documentation, approved architecture reconciliation, authored typing,
generated artifacts, removal of unreachable wiring, and already-correct
credential/bootstrap/security branches use static, regression, or
verification-only proof rather than manufactured RED. Record why each is
covered and preserve its reachable behavioral assertions.

Run the applicable owner checks after focused behavior is green:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run check:py-wheel-no-testing
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run docs:check
mise run check:examples
```

Run only those needed by the actual write set and preserve required UI checks
if UI code changes. These are task-owned format/static/type/boundary proofs,
not broad test suites. The approved specification assigns the final
integrated aggregate to change review; do not duplicate that aggregate or
complete multi-language journey suites inside this task. Keep new tests
selected by their owning existing lane so integrated review cannot miss them.

Evidence records each REQ/INV/AC, implementation/source location, focused
command, RED reason, GREEN result, regression/static result, generated source
and output, and any sanctioned private correction. Inspect tracked and
untracked diffs for unrelated work and secrets. A red required check blocks
completion regardless of authorship; diagnose the failure itself, never run a
baseline checkout to establish blame.

### Implementation evidence (2026-10-15)

Every selector above was run exactly as written from the repository root;
each selected one test and passed (nonempty discovery confirmed by the
runner's `1 test run` / `1 passed` line).

| Test | Scenario | REQ / INV / AC | Result |
|---|---|---|---|
| T01 | 1, 7 | REQ-005, REQ-007–REQ-009, REQ-011; INV-003; AC-004, AC-006 | PASS |
| T02 | 2 | REQ-006–REQ-008; AC-006 | PASS |
| T03 | 3 | REQ-001, REQ-003; INV-001–INV-003; AC-001 | PASS |
| T04 | 3, 4 | REQ-001–REQ-002; INV-001–INV-002; AC-001–AC-002 | PASS |
| T05 | 5 | REQ-003–REQ-004; INV-001–INV-002; AC-003 | PASS |
| T06 | 6 | REQ-003; INV-003; AC-012 | PASS (first run hit a transient Postgres port answered by an HTTP listener, `SSLRequest: 0x48`; traced rerun passed unchanged) |
| T07 | 7 | REQ-005–REQ-008, REQ-011; AC-004, AC-006 | PASS |
| T08 | 8 | REQ-009–REQ-010; INV-004, INV-007; AC-002, AC-005 | PASS |
| T09 | 8 | REQ-009–REQ-010; INV-004, INV-007; AC-005 | PASS |
| T10 | 9 | REQ-010; AC-008 | PASS |
| T11 | 10 | REQ-008–REQ-011; AC-002, AC-007 | PASS |
| T12 | 11, 12 | REQ-011–REQ-012; INV-003; AC-009–AC-010 | PASS |
| T13 | 13 | REQ-013; INV-001–INV-002; AC-003, AC-011–AC-012 | PASS (inside `mise run test:local-dev:journey`, exit 0) |
| T14 | 8, 12 | REQ-009, REQ-011; INV-005; AC-004–AC-006, AC-009 | PASS |
| T15 | 8, 12 | REQ-009, REQ-011; INV-005; AC-004–AC-006, AC-009 | PASS |
| T16 | 12 | REQ-002, REQ-011; AC-009 | PASS |
| T17 | 3 | REQ-001; INV-003 | PASS |

Supporting test added for judge-route refusal (Scenario 10, AC-007):

```bash
mise exec -- cargo nextest run --locked -p vala-eval --test integration -E 'test(=orchestrator_judge_skald::judge_routes_its_context_cannot_serve_are_refused)'
```

PASS. A server-controlled judge refuses a native Agent route and a
gateway route without a gateway caller, both terminal and before upstream IO.

Judge attribution follows `architecture/bifrost-design.md`: a direct result
carries the caller's pair, a queued result carries the run's subject with
SYSTEM as actor, and the judge's gateway call carries SYSTEM. T11 asserts the
direct-result pair and the SYSTEM call actor.

Route evidence reuses existing columns: the Card default is reachable through
`card_uid`, the effective route is `requested_model.deployment`, and the
deployment that served is `resolved_model` / `resolved_deployment`. No columns
were added.

INV-005 (one shared owner): Python `Run.invoke(route=...)` and TypeScript
`run.invoke(vars, { route })` decode into the Rust `ExecutionRoute` and call
`Run::invoke_on`. Neither adds routing logic, and a malformed route is
`WYRD_SPEC_400_VALIDATION`. INV-006: the audit/capture outbox writer is
unchanged by this task (source inspection); no new writer or queue exists.

Owner checks, all exit 0: `fmt`, `lints`, `codegen:check`, `check:deps`,
`check:tenant-isolation`, `check:py-wheel-no-testing`, `py:format`,
`py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`,
`ts:napi:check`, `docs:check`, `check:examples`. `ts:typecheck` first failed
on two TypeScript test helpers still using the retired deployment shape
(`model`, `routing_weight`). They now deploy named `openai`/`anthropic`
targets like their Python peers, and the support-desk TypeScript journey
passes.

Environment-only failures seen in broader workspace runs, not in this task's
write set: OIDC/Dex identity journeys, saved-user auth, CLI device login,
Vault, kind startup image, cgroup capacity, and live Slack/PagerDuty smoke.
Each needs external infrastructure that is absent locally. The final
integrated aggregate belongs to change review.

### Post-merge evidence (main merged 2026-10-15)

Main's agent-prompt change registers an Agent's inline Prompt as its own
`<agent-name>-prompt` Prompt Card; that behavior is kept, and T07 now proves
the inline form through the hydrated Agent's Prompt Card. Main's Workflow
additions were removed with the rest of Workflow. Main's agent-prompt
fixtures and server tests now author portable Prompts with execution routes.

After the merge, T01–T12 and T14–T17 and the judge-route refusal test rerun
with their exact selectors: all PASS. T13 passed before the merge and was not
rerun. Also PASS: main's agent-prompt journeys
(`register_and_hydrate::agent_prompt_registration_identity`,
`register_and_hydrate::hydrated_agent_prompt_card_navigation`, the
`pg_card_registration_route::` module, 30 tests); the Python
register/gateway journeys (18); the TypeScript register/gateway/support-desk
journeys (25); `py:test:unit` (621); `ts:test:unit` (73); `test:shared` (795);
`test:wyrd` (2378); `test:cli:journey`; and the CLI `card_lifecycle` tests (19,
which expect 10 Cards: the Workflow Card is gone and the materialized inline
Prompt is added). Owner checks rerun clean: `fmt`, `lints`, `codegen:check`,
`check:deps`, `docs:check`, `py:lints`, `py:typecheck`, `ts:typecheck`,
`ts:lints`, `ts:napi:check`, `check:ci-selection`.

`gemini` is refused as a custom Prompt provider and the refusal names
`google`, proved by
`mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=request::round_trip::provider_name_yaml_reads_back_through_buffered_json)'`.

The failing main release jobs are fixed on this branch:
- crate packaging: `wyrd-telemetry` now has a version and is published,
  because `wyrd-tonic` depends on it; the release job's `cargo package`
  command passes locally;
- Python wheels: the protoc download now fails on HTTP errors and retries;
- arm server bundle: Linux arm runners get 16 GiB of swap for the fat-LTO
  link. Only the next release run can prove this.

## Material Stop Conditions

Stop affected work and route to specification authority if correctness needs:

- A different public/persisted shape, setting/media/format mapping, route
  precedence/override semantics, batch atomicity/progress semantics, or scope
  than approved r5; unknown runtime model capability is not permission to
  invent a model catalog or silently drop fields.
- A new credential source, model mapping/alias, direct external route, local
  Vertex gateway extension, server Agent job API, or replacement orchestration.
- Widening tenant/credential/host/permission boundaries, changing subject/actor
  authority, synchronous audit acceptance, or deployed data deletion.
- Removing independent evaluation behavior or a live gate to simplify Workflow
  removal, rather than preserving/reusing its owner and proof.
- Replacing existing complete owner workflows with a new graph/loader/client/
  loop/cache/lifecycle without evidence that an in-owner extension cannot
  satisfy the approved contract.

A missing test fixture, stale private symbol/selector, ordinary compile/lint
failure, or repository-managed dependency setup is an implementation-owned
reversible correction. Fix and document it; do not create a new plan/task or
weaken proof. Do not call the task complete with an uncovered spec criterion.

## Authority Links

- [Approved SPEC-deployment-addressing r5](../spec.md).
- [AGENTS.md](../../../../AGENTS.md) and
  [agent rules](../../../../architecture/agent-rules.md).
- [Protocol authority](../../../../architecture/wyrd-design.md) and
  [doctrine](../../../../architecture/wyrd-doctrine.mdx); r5 explicitly fixes
  the replacement Prompt/Agent/Workflow decisions to reconcile here.
- [Reference router](../../../../architecture/references/README.md),
  [architecture constraints](../../../../architecture/references/doctrine/architecture-constraints.md),
  [ownership patterns](../../../../architecture/references/architecture/patterns.md).
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../../architecture/references/languages/implementation-execution.md),
  [testing workflows](../../../../architecture/references/languages/testing-workflows.md).
- `$wyrd-implement`; additionally `$wyrd-ui` when touching Wyrd UI.

## Planning Evidence

One task by explicit owner request; no dependencies or new implementation
subtasks. Source ownership, Cargo package/target metadata, current test
namespaces, mise test/setup bodies and the Postgres wrapper were inspected.
No implementation, RED/GREEN, build, test, or readiness verdict is claimed by
planning. New named tests above are planned, not executed or collected yet.
