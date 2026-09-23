---
id: SPEC-verified-change-contract
revision: 35
status: approved
---

# Verification contract

## Objective and user value

Give humans and agents one reusable Verifier Card model. This change replaces
registrable Drift and Eval Cards with typed Verifier implementations and ships
production-ready continuous Drift and Eval. The same model can later verify
proposed code changes through code review, CI, APIs, MCP, Python, LLM judges,
and Workflows; those executors are not delivered by this change.

A Verifier is a reusable Card that declares one typed implementation. The
initial implementations are Drift and Eval; a Trigger activates a bound
Verifier, and its result may dispatch configured Operators.

## Scope

- `Verifier` as the single registrable verification Card kind.
- Retirement of `Drift` and `Eval` as registrable Card kinds. Their authored
  specs become the `drift` and `eval` variants inside `VerifierSpec`.
- Removal of the unused `vala.eval.runs`, `vala.eval.assertions`, and
  `vala.drift_alerts` tables and their obsolete owning APIs/schemas.
- Retirement of the unused `vala-core::alert_router` webhook configuration and
  its otherwise unconsumed `vala-core` crate, schemas, and dedicated checks.
- YAML authoring and normal Wyrd Card loading for Verifiers.
- Production-ready Drift and Eval Verifier implementations across their
  server, Bifrost, and first-class SDK surfaces.
- Initial common Verifier, binding, activation, run, result, and Operator
  contracts required by Drift and Eval.
- Durable continuous Verifier runs, queryable results, and Notify/HTTP Operator
  delivery.
- Tenant-managed Operator connections backed by encrypted Postgres state.

## Non-goals

- Change Request, Claim, and Evidence contracts or workflows.
- Field-level specs, registration, execution, and journeys for code review,
  CI test review, Python, MCP, API, standalone LLM-judge, and Workflow
  Verifiers. Each belongs to a follow-up change.
- General-purpose arbitrary code execution, a Verifier marketplace,
  optimizer, or automatic Verifier selection system.
- A Verifier-specific DAG or general-purpose external workflow language.
- Treating verification as approval, merge, deployment, or authorization.
- UI layout or component behavior.
- Offline Eval scenario Data Card registration and dataset-backed evaluation.
- Workflow Operator invocation. Its existing action shape remains parseable,
  but a verification binding cannot activate it until the separate server
  invocation change supplies an executable path.

## Initial delivery boundary

This change implements the initial common Verifier contracts, adaptation of
the existing `DriftSpec` and `EvalSpec`, the missing Drift/Eval server
machinery, and production-ready end-to-end Drift/Eval journeys. It does not
define field-level specs for future implementations. Those specs and their
executors are follow-up work and do not block approval of this change.

Production-ready means Drift and continuous Eval are not registration-only
contracts, in-memory demos, or disconnected engine tests. Their supported user
workflows MUST run through real SDK, server, durable work, Bifrost persistence,
status, result, Notify/HTTP Operator delivery, restart, authorization, and
tenant-isolation boundaries. Accepted reliability ceilings are explicit:
Eval run enqueue is best-effort after its observation commits, so a committed
observation may not receive a run if that post-commit step fails or the process
crashes; separate result-table acknowledgements can leave partial rows; and
an ambiguous external Operator send may be delivered more than once.

Only Drift and Eval MAY appear as registrable `VerifierImplementation`
variants or in generated public schemas in this change. Eval's existing internal
LLM-judge task remains part of the initial Eval implementation; it does not
make the standalone LLM-judge Verifier implementation part of this delivery.

## Definitions

- **Verifier Card**: the reusable, versioned declaration of one verification
  implementation and its typed configuration.
- **Verifier implementation**: one closed, YAML-authored variant, initially
  Drift or Eval.
- **Verification binding**: one inline Service component, Service, or standalone
  Agent declaration that names the Verifier, when it runs, and zero or more
  Operators performed independently after a failed verdict.
- **Runtime-active binding owner**: the exact Service or standalone Agent Card
  version whose existing Card-bound machine principal successfully completed
  an API-key exchange or workload `jwt-bearer` authentication within the
  configured verification inactivity timeout. Service component bindings
  inherit the activity of their containing Service version.
- **Verification Result**: one Verifier run's immutable Bifrost summary and
  verdict, identified by exact tenant, run, binding, subject, Verifier Card
  version, and input identity.
- **Verification run status**: `queued`, `running`, `completed`, `cancelled`,
  `timed_out`, or `errored`.
- **Verifier verdict**: `passed`, `failed`, or `inconclusive`; present only for
  a completed run.
- **Trigger activation**: the binding's effective `runs_on` condition that
  creates a Verifier run. Drift uses a due schedule occurrence; continuous Eval
  uses a successfully committed observation. There is no separate activation
  queue or Card created at runtime.
- **Operator dispatch**: one durable Postgres work row per configured Operator
  created by the generic runner after a binding-created Verifier run completes
  with `failed`. It owns its own claim, retry, and delivery status; no separate
  Alert table or delivery queue is created.
- **Attesting Eval task**: a task whose executor successfully returns an
  `AssertionResult`. A false assertion is attesting evidence; a skipped task or
  an executor/input failure is not.
- **Internal SYSTEM result writer**: one server-only tenant principal, persisted
  in the existing tenant machine-principal store with `kind: system`, a
  server-minted UUIDv7, and the fixed name `verification-results-writer`. It is
  used only to publish Verification Result batches and has no public
  credential, Card, role grant, refresh, workload, delegation, or
  principal-management path.
- **Operator connection**: one tenant-owned, provider-specific Postgres record
  containing nonsecret delivery coordinates and an encrypted credential. Cards
  carry only its provider-scoped name; connection reads never return secret
  material.

## Required behavior

### Verifier Card

- **REQ-045**: Wyrd MUST make `Verifier` the registrable verification Card kind.
  It MUST use the normal `apiVersion`, `kind`, `metadata`, and `spec` envelope,
  normal CardRef identity, composite registration, schema generation, and
  `wyrd apply` loader behavior.
- **REQ-109**: `Drift` and `Eval` MUST cease to be registrable Card kinds.
  New authored Cards, CardRefs, binding refs, generated schemas, SDK/CLI/MCP
  surfaces, and relationship extraction MUST use `kind: Verifier` and its
  `implementation.kind: drift | eval`. The server MUST reject new registrations
  with `kind: Drift` or `kind: Eval`; no compatibility alias, parallel
  registry path, or second monitor Card is introduced. Nothing using the old
  Card kinds has shipped; this change provides no compatibility registration
  or historical-card migration path. The v1 catalog therefore contains 15
  registrable native kinds, with `Verifier` replacing both `Drift` and `Eval`.
  `AGENTS.md`, `architecture/wyrd-design.md`, and
  `architecture/wyrd-doctrine.mdx` MUST publish that same catalog and
  `verified_by` binding model; `publishes_to` MUST NOT remain as a verification
  subscription contract.
- **REQ-046**: `VerifierSpec` MUST contain exactly one adjacently tagged
  `implementation` whose `kind` selects a closed variant with variant-specific
  typed fields. It MUST NOT use a generic parameter or configuration object
  where typed fields can express the contract.
- **REQ-047**: The initial registrable `VerifierImplementation` variants MUST
  be `Drift(DriftSpec)` and `Eval(EvalSpec)`. They MUST incorporate the current
  Drift and Eval contracts rather than replace them.
- **REQ-110**: The Drift payload MUST retain the existing `method`, `signal`,
  `condition`, `profile`, and nested PSI/SPC/Custom configuration and
  validation, with the agreed removals of `DriftMethod::External`,
  `DriftSignal::External`, `DriftSignal::EvalScore`, and `DriftSpec.details`.
  The only valid method/signal pairs are `Psi` + `Distribution`, `Spc` +
  `Distribution`, and `Custom` + `Metric`, each with its matching profile.
  Registration MUST reject all other pairs, including the currently accepted
  `Spc` + `Metric`; it MUST NOT reinterpret that pair as Custom drift.
  PSI and SPC runs require a version-pinned baseline Data Card and a persisted
  server-fitted profile before analysis; Custom retains its authored scalar
  baseline and requires no fitter. The user authors the strategy, not the
  fitted distributions. `condition` MUST be `Statistical` for all three
  executable pairs; `Above`, `Below`, and `Outside` remain typed vocabulary
  but registration rejects them in this delivery because the three profiles
  already own their thresholds. The SPC public contract remains exactly
  `SpcProfile { sample_size, weco_rule, alert_threshold }`: `sample_size = 0`
  uses the existing row-count adaptive chunk size and any authored value MUST
  be at least 2; the eight-positive-integer `weco_rule.rule_string`, existing
  zone assignment, trend rule, and `alert_threshold` filtering retain their
  current `vala-drift` semantics. Runtime observations are ordered by
  `created_at`, then `record_id`, before the same consecutive chunk-mean
  scorer is applied; incomplete trailing chunks retain the scorer's current
  behavior. This change MUST NOT replace that contract with a fixed four-rule
  WECO policy, an X-bar/S dual-chart result, a 25-subgroup readiness gate, or
  a new SPC result shape. No client-side binning or subgroup aggregation is
  added.
- **REQ-111**: The Eval payload MUST retain the existing `EvalSpec` fields
  `dataset`, `tasks`, `workflow`, `sampling`, `pass_gate`, and
  `context_capture`, and the existing nested task/DAG/result semantics.
  `dataset` remains a valid authored reference for future offline Eval but
  dataset-backed execution is outside this change. Eval assertions and
  LLM-judge tasks use the current Vala Eval execution engine, not a new
  verifier-specific Eval engine.

The initial Verifier implementations and input/output boundaries are:

| Implementation kind | Typed spec | Verification input | Typed engine output | Delivery |
|---|---|---|---|---|
| `drift` | existing `DriftSpec` with REQ-110 removals | Exact subject and fixed window of existing `DriftRecordObservation` rows; PSI/SPC also load exact `FittedBaseline` | Existing `DriftReport` mapped to one common verdict plus feature details | Production in this change |
| `eval` | existing `EvalSpec` | Exact subject and committed existing `EvalRecordObservation`, with declared trace context when needed | Existing `EvalReport` task outcomes and `EvalWorkflowSummary` mapped to one common verdict plus task details | Production in this change |

Future code-review, CI-test, Python, MCP, API, standalone LLM-judge, and
Workflow kinds receive their field-level specs and executors in follow-up
changes. They are not variants of this change's public `VerifierSpec`.

- **REQ-056**: One Verifier Card declares one implementation. Users compose
  independent judgments by attaching multiple Verifier Cards. This change
  MUST NOT introduce a Verifier DAG or another workflow mechanism.
- **REQ-072**: The initial Drift Verifier server implementation MUST resolve
  the registered `Data` Card referenced by `DriftSignal::Distribution`, read
  its registered Parquet artifact as Arrow, fit the baseline with the
  user-authored
  `DriftProfile`, and persist the returned `FittedBaseline` against the exact
  tenant, Verifier Card version, and resolved Data Card identity. PSI and SPC
  evaluation MUST load that persisted baseline before scoring runtime
  observations. Pandas, Polars, and Arrow authoring paths MUST all register a
  Parquet artifact usable by this flow; Wyrd MUST NOT reinterpret an arbitrary
  non-Parquet Data artifact as a baseline.

### Run API and observation authoring

The agreed Rust, Python, and TypeScript user interfaces and examples are
locked in [run_api.md](architecture/logic/run_api.md). This section fixes
their shared behavior without creating another Run registry or ingest path.
The three exact `observe.eval`, `observe.drift`, and `observe.record` insertion
flows are listed in its "Input and queue boundary" section.

- **REQ-123**: All three SDKs MUST expose the run API shown in `run_api.md`:
  Python `state.start_bifrost(...)`, `state.run(card=None)`,
  `run.for_card(alias)`, and
  `scope.observe.drift(...)` / `scope.observe.eval(...)` /
  `scope.observe.record(table, value)`; TypeScript `await
  state.startBifrost(options)`, `state.run(card?)`, `run.forCard(alias)`, and
  the corresponding `scope.observe` methods (with async `record`); Rust
  `state.start_bifrost().await?`, `state.run()`,
  `state.run_for_card(alias)?`, `run.for_card(alias)?`, and
  `scope.observe().drift(...)` / `scope.observe().eval(...)` /
  `scope.observe().record(table, value).await?`. Python and TypeScript expose
  `observe` as a member; Rust uses an `observe()` accessor. Python startup and
  shutdown are synchronous; TypeScript startup and shutdown are async. The
  Rust async surface is required and a synchronous entry point may use the
  existing blocking Bifrost boundary outside an async runtime. Startup
  options MUST pass through the existing language-specific Bifrost
  constructor options, including their environment defaults, without a new
  configuration type. `WyrdState` MUST own the connected
  `wyrd_client::Bifrost` facade, which owns the existing bounded queue and
  transport. Callers MUST NOT pass a separate Bifrost client or table to
  `run()`.
  A run generates one client-side invocation `run_id` and defaults to the root
  Service Card. The language-idiomatic initial-Card argument and later
  `for_card` / `forCard` selection both resolve a hydrated Card alias and
  return an immutable view of that invocation that automatically supplies its
  exact subject `card_ref` on every observation; sibling views cannot retarget
  each other. Unknown or out-of-graph aliases fail. Run creation and scope
  selection MUST NOT register a server Run, authenticate by themselves, or
  execute a Verifier.
- **REQ-151**: Python `Run` MUST be a synchronous context manager. Entering it
  MUST best-effort attach the selected CardRef and invocation ID to Python's
  execution-local OpenTelemetry context under the exact Bifrost attributes
  `wyrd.card_ref` and `wyrd.run_id`, set both attributes on an already-active
  recording span, and ensure one idempotently registered span processor copies
  both values from the parent context to every span started inside the scope.
  Exiting MUST restore the prior context, including nested Card scopes, and
  MUST NOT suppress a user exception. Normal context propagation MUST work
  across `await` and asyncio task creation without storing one shared attach
  token on the immutable Run. A framework using the global provider MUST need
  no setup beyond `with state.run(...)`; the Python SDK MUST expose an
  idempotent `wyrd.otel.install_run_correlation(provider)` escape hatch for a
  framework-owned private provider. Missing OpenTelemetry packages, an
  unsupported or absent provider, no active recording span, invalid runtime
  context, processor failure, and attach/detach failure MUST all fail open as
  no enrichment: they MUST NOT fail Run construction or entry/exit, application
  execution, or explicit Wyrd observation emission. Unknown Card aliases,
  authorization, validation, and Wyrd writes remain fail-closed. The client
  MUST inject only CardRef and run ID; tenant, principal, Card UID, and request
  identity remain server-derived. The context manager MUST NOT start or end a
  span, flush or close Bifrost, perform network IO, or create a server Run.
  This requirement covers spans only; it does not promise ambient log or
  metric enrichment. Shared Rust owns Run identity and Card selection, while
  the Python SDK owns its foreign-runtime OpenTelemetry context and provider
  integration. OpenTelemetry remains an optional dependency.
- **REQ-124**: `observe.drift` MUST accept a Python mapping, dataclass, or
  Pydantic model; a TypeScript plain serializable object; or a Rust `Serialize`
  map/struct. The value represents one event's feature-name-to-scalar
  measurements. Only existing `FeatureValue` kinds—boolean, signed 64-bit
  integer, finite float, and string—are accepted; nested values, nulls,
  non-finite or unrepresentable numbers, and silent JavaScript omission or
  coercion fail before queue admission. The Python Pydantic path MUST pass
  `model_dump_json()` output directly as its one JSON payload to the Rust
  boundary; it MUST NOT first call `model_dump()` or serialize that output
  again. Python mappings and dataclass instances MUST use strict JSON
  serialization with string keys and non-finite numbers rejected. The SDK
  MUST NOT require Pydantic as a runtime dependency merely to recognize this
  input. `observe.eval` MUST accept the same language-native input families
  as JSON-serializable task context plus the optional per-emission fields in
  REQ-129. Neither ordinary call requires a
  caller-authored record ID, run ID, CardRef, timestamp, or Verifier reference;
  neither synchronously executes a Verifier or returns a score or alert.
- **REQ-125**: The implementation of each `observe.drift(...)` or
  `observe.eval(...)` call MUST normalize its input into the existing
  `DriftRecordObservation.features` or `EvalRecordObservation.context`,
  construct the canonical record with generated record ID, emission time, and
  optional session/trace/media information, and carry the scoped subject
  `card_ref` and invocation `run_id` only through existing per-row Bifrost
  correlation. Neither canonical record nor its serialized user row carries
  a duplicate `run_id`. Each call MUST complete its own fixed,
  query-ready projection into row JSON conforming to its canonical table
  schema **before** inserting through the Bifrost facade held by `WyrdState` via the
  existing explicit-table `WriterPool::insert` seam already used by
  `bifrost::observe::record`. No `use_table_by_name`
  or shared active-table mutation occurs in an observation call.
  The queue builds an Arrow `RecordBatch` from the rows and fixed schema,
  serializes Arrow IPC, and publishes through Gate/Scribe. Custom input types
  MUST NOT infer Bifrost table schemas, enter the queue as arbitrary nested
  rows, create a new transport, or perform client-side Drift aggregation.
  Shared `wyrd-client` and `wyrd-queue` MUST NOT deserialize, dispatch, or
  project by Verifier kind; they own only generic client, queue, transport,
  and publication mechanics. For Drift, its observation boundary MUST
  deserialize the flat feature-object JSON into the existing validated
  `BTreeMap<FeatureName, FeatureValue>` rather
  than introducing another public feature type or a handwritten coercion
  language. It MUST project one flat `vala.drift.observations` row per feature:
  repeated `record_id`, optional `session_id`, `created_at`, feature `series`,
  nullable `num_value`, and nullable `str_value`. `Cat` and `Bool` use
  `str_value`; `Int` and `Float` supply finite, exactly representable
  `num_value` and the same canonical string representation used by baseline
  fitting for categorical PSI. Each row retains the run's subject CardRef
  and run ID as queue correlation, not as user-schema fields. Raw rows carry
  no Verifier or binding ID
  and are not duplicated for multiple bindings. The separate Eval physical
  projection remains to be fixed by its table-schema decision.
- **REQ-126**: Ordinary `observe.drift` and `observe.eval` calls MUST enqueue
  without a per-observation `flush()` or server-verdict wait. Existing
  background publication, bounded admission/drop telemetry, and graceful
  `WyrdState.shutdown()` drain across every table producer apply. Admission
  to the in-memory queue MUST NOT be described as a durable Scribe
  acknowledgement; tests and finite jobs MAY use `flush()` as an explicit
  durability barrier. No implicit run-scope network call, idle refresh, or
  extra heartbeat is added.
- **REQ-127**: `start_bifrost` MUST connect the state-owned Bifrost facade and
  describe both fixed system input tables, `vala.drift.observations` and
  `vala.eval.observations`, before observations may be emitted. A missing,
  unauthorized, or incompatible fixed table MUST fail startup. Bifrost MUST
  retain each resolved table name and its user schema for the connected
  writer's lifetime and supply that schema on explicit-table inserts, without
  per-observation describe requests or client-authored table schemas.
  `observe.drift` and `observe.eval` MUST stay synchronous projection-and-
  enqueue calls in Rust, Python, and TypeScript. The constructor's optional
  active `table` argument retains its existing Bifrost meaning; it MUST NOT
  select a run's destination or change either fixed system table. Repeated
  `use_table_by_name` calls are not a schema cache: they describe remotely and
  mutate the shared active table, so the run observation path MUST NOT use them.
  The existing WriterPool already reuses its producer for each table; no
  second queue, transport, or per-table producer cache is created. Shared
  `wyrd-client` and `wyrd-queue` MUST remain Verifier-kind agnostic. Explicit
  startup describe requests MAY invoke the existing card-bound token
  exchange and therefore activate the exact principal under REQ-105/106;
  startup MUST NOT introduce an independent heartbeat or idle refresh.
- **REQ-133**: `start_bifrost` / `startBifrost` MAY succeed only once per
  `WyrdState`; a second start MUST return an already-started error rather than
  replace the live writer or silently ignore different options. Shutdown MUST
  drain the same writer. If shutdown fails after an ambiguous batch outcome,
  the caller MUST be able to retry shutdown on that same handle; the state MUST
  NOT create a replacement writer that could strand pending rows. After a
  successful shutdown, that state is closed to Bifrost writes and cannot be
  restarted; a new `WyrdState` is required for a new writer lifetime.
- **REQ-128**: `observe.record(table, value)` MUST require the destination
  table name on the call, not on `state.run()`, and infer no table from the
  CardRef or run ID. It MUST accept a language-native serializable row and
  supply the scoped CardRef/run ID correlation without requiring a caller
  schema, `TableConfig`, or Bifrost handle. The row and destination schema
  MUST be representable by the existing JSON-to-Arrow queue, including the
  fixed-size ID conversion in REQ-132; other unsupported binary or nested
  table columns retain Bifrost's explicit Arrow batch path.
  On first use of an already
  registered user table, Bifrost MUST describe it and cache its name and user
  schema before enqueue; later writes reuse that schema and the existing
  pooled producer. Concurrent first uses of the same table MUST converge on
  one schema/producer. Unknown, unauthorized, or unavailable tables MUST fail
  before queue admission. Python's synchronous `record` MAY block on first
  describe; TypeScript and Rust `record` MUST be async for that first IO.
  Inserts MUST route by explicit table and cached schema without mutating
  Bifrost's shared active table. One schema per table name MUST remain
  authoritative for the writer's lifetime; a conflicting schema MUST NOT
  silently replace an existing producer's schema. A server-side schema
  change requires a new writer after shutdown or receives a schema-fingerprint
  refusal. Generic `record` MUST obey ordinary user-table permissions and
  MUST NOT write reserved/system-managed tables or select a Verifier binding.
  All table producers share one Bifrost transport and bounded client budget;
  no per-Card, per-Agent, per-run, or per-table Bifrost client is created.
  Table description MUST NOT be represented as row validation or a durable
  ingest acknowledgement: generic row/schema validation may complete when
  the queue seals a batch, and Scribe checks the registered fingerprint.
- **REQ-129**: The public `observe.eval(context, options)` surface MUST accept
  required JSON-serializable `context` and optional per-emission `session_id`,
  `media`, `trace_id`, and `span_id` in each first-class SDK. Python and
  TypeScript MAY express the optional fields as keyword/options arguments;
  Rust MUST expose one typed SDK-only options value rather than positional
  arguments for each optional field. This is an authoring wrapper around the
  existing `EvalRecordObservation`, not a second durable observation type.
  Inside `eval(...)`, the SDK MUST construct that canonical record, generate
  `record_id` and `created_at`, and attach the scoped run's `run_id` and
  subject CardRef through Bifrost correlation. `eval_ref` MUST be
  deleted from the canonical record and MUST NOT appear in the wrapper or
  projected rows. `run_id` MUST likewise be removed from the canonical Eval
  record and projected user row; the scoped run supplies it as Bifrost
  correlation. Explicit trace/span IDs take precedence; otherwise the SDK
  MUST attempt to take valid IDs from the active OpenTelemetry span when its
  runtime exposes one, never from process-wide environment variables. An
  absent active span leaves both IDs absent;
  `span_id` without `trace_id` is invalid. The `eval(...)` call MUST project
  the fixed `vala.eval.observations` rows before handing row JSON to the
  state-owned Bifrost facade and its cached schema. Neither Bifrost nor the
  queue interprets Eval-specific fields.

### Registration and runtime wiring

- **REQ-090**: A Service component, Service, or standalone Agent MUST declare
  continuous verification through `verified_by: Vec<VerificationBinding>`.
  Each binding MUST contain one required `verifier`, one required `runs_on`,
  and zero or more `on_failure` Operators. This inline binding is part of the containing
  Card spec; it is not a registrable Card or separate resource.
- **REQ-091**: `VerificationBinding.verifier` MUST be a `Ref` resolving to one
  exact Verifier Card. `runs_on` MUST be an `InlineableRef<TriggerSpec>`.
  Each `on_failure` entry MUST be an `InlineableRef<OperatorSpec>`. Existing
  untagged reference semantics apply unchanged: a scalar local path is loader
  input, a CardRef object names a registered Card, and an inline mapping embeds
  the typed spec.
- **REQ-092**: Registration MUST resolve and UID-pin every referenced Verifier,
  Trigger, and Operator through normal composite-registration rules, validate
  the expected Card kinds and tenant authorization, traverse references inside
  inline specs, and derive relationships before durable Card persistence or
  read response. Unresolved, wrong-kind, unauthorized, and cross-tenant
  references MUST fail closed.
- **REQ-093**: `TriggerSpec` MUST contain one flattened closed activation. The
  initial variants MUST be `schedule`, valid for a Drift-backed Verifier, and
  `observations_ready`, valid for an Eval-backed Verifier. It MUST NOT reference
  a Verifier or Operator or contain Drift thresholds, Eval pass-rate criteria,
  or another verdict matcher.
- **REQ-094**: `OperatorSpec` MUST contain one flattened existing
  `OperatorAction`: `notify`, `http`, or `workflow`. A referenced Operator Card
  and an inline `on_failure` mapping MUST use the same `OperatorSpec` shape.
  `workflow` remains a parseable Card action, but registration MUST reject it
  in `on_failure` with a stable unsupported-action error until the separate
  server Workflow invocation change is available. An unsupported action MUST
  NOT create a dispatch that can never be delivered.
- **REQ-095**: One verification binding is one subscription. Its subject is the
  containing Service, standalone Agent, or Service component occurrence. Its
  Verifier is `verifier`; its activation is the effective inline or referenced
  `runs_on`; and its failure reactions are the effective inline or referenced
  `on_failure` Operators. The same Verifier, Trigger, and Operator Cards MAY
  be reused by any number of bindings.
- **REQ-096**: The normative runtime direction MUST be
  `binding activation -> Verifier run -> Verification Result -> zero or more
  on_failure dispatches`. A Verifier run MUST NOT create another Trigger.
  The generic runner, not a Drift or Eval engine, creates one dispatch per
  configured Operator after a failed binding-created result. A Notify Operator
  sends an alert/notification as its action; no Alert resource or table starts
  an Operator. Direct Verifier invocation remains analysis-only; manually
  invoking a binding follows its automatic path.
- **REQ-102**: For one subject occurrence, duplicate bindings to the same exact
  Verifier version MUST be rejected. One schedule occurrence creates one run
  per active binding. Reusing one Trigger Card from two Services therefore
  creates two subject-scoped runs, not one mixed-subject run. One Eval record
  creates one run for each distinct active binding to which that record routes.
- **REQ-103**: `verified_by` replaces `publishes_to` for Drift and Eval
  verification routing. The removed verification meaning MUST NOT survive as a
  compatibility alias. Observation routing uses the observed subject's exact
  active verification bindings under the authenticated Service, standalone
  Agent, or Service component occurrence; clients do not select a Verifier
  with `drift_ref` or `eval_ref`.
- **REQ-104**: `BindingId` MUST be a typed UUIDv7 public and durable identity.
  On the first projection insert, the server MUST mint it and persist it under
  the unique natural key `(data_tenant_id, owner_card_uid,
  subject_occurrence_key, verifier_uid)`. `owner_card_uid` and `verifier_uid`
  already identify exact immutable Card versions. The subject occurrence key
  is the reserved owner value for a Service-level or standalone-Agent binding,
  and the component alias for a Service-component binding; component aliases
  MUST be unique within the Service. Re-applying the identical Card projection
  MUST preserve the stored ID, while a new owner Card UID, component alias, or
  Verifier UID creates a new ID. Reordering bindings MUST NOT change it.
  Runs MUST also bind the effective Trigger and every Operator Card identity
  or, for inline definitions, their canonical spec digests. Users MUST NOT author a
  separate binding name or identifier.
- **REQ-105**: Registering a Service, standalone Agent, or verification binding
  MUST NOT make its bindings runtime-active. A successful existing card-bound
  Service or Agent API-key exchange or workload `jwt-bearer` authentication
  MUST activate the exact owning Card version. A later successful exchange of
  either durable machine credential MUST renew its activity. Delegation, OIDC
  login, human refresh, Card-free automation, SYSTEM issuance, cached bearer
  use, user authentication, another Card version, and authorization scope over
  a component MUST NOT activate the owner.
- **REQ-106**: Runtime activity MUST extend the existing machine-principal
  authentication lifecycle. The server MUST persist the principal's last
  successful qualifying exchange time on its existing machine-principal state,
  using the server clock and the same transaction that issues the token.
  `wyrd-client` MUST continue to re-exchange a stale access token automatically
  when an authenticated request needs one; token expiry or an otherwise idle
  `WyrdState` MUST NOT initiate an exchange by itself. Serving a request with
  a still-valid cached token MUST NOT update activity. This change MUST NOT
  add an activation endpoint, heartbeat protocol, activity table, background
  refresh timer, per-request touch, or per-observation activity write.
- **REQ-107**: A binding owner is runtime-active only while its service
  principal is enabled and its last successful qualifying machine exchange is
  within `WYRD_VERIFICATION_INACTIVITY_TIMEOUT_SECONDS`, which MUST default to
  `86400`. A process remaining open without another successful token exchange
  does not extend activity; once the timeout passes, the owner becomes
  inactive without changing Card lifecycle or credential status. Suspending
  or deleting the principal makes it inactive immediately.
- **REQ-108**: Before creating a binding-driven Verifier run, the scheduler
  or Eval post-commit enqueue MUST restrict new work to runtime-active exact
  owners. An inactive occurrence MUST create no activation or run and MUST NOT
  be backfilled after later authentication. Reauthentication starts eligibility
  with the next schedule occurrence after that authentication. A later
  qualifying exchange while the owner remains active MUST NOT reset or postpone
  its existing schedule cursor. Multiple exact Card versions used for A/B
  operation remain independently active through
  their distinct service principals; replicas sharing one exact principal
  share its activity. Service-level and component-occurrence bindings inherit
  their exact containing Service principal's activity, while a standalone
  Agent binding uses that Agent's principal activity. Activity is an
  admission gate for new binding-created runs, not a reason to cancel an
  already-admitted run whose input was frozen while active.

The contract mapping is:

```rust
pub struct VerificationBinding {
    pub verifier: Ref,
    pub runs_on: InlineableRef<TriggerSpec>,
    #[serde(default)]
    pub on_failure: Vec<InlineableRef<OperatorSpec>>,
}

pub struct TriggerSpec {
    pub description: Option<String>,
    #[serde(flatten)]
    pub activation: TriggerActivation,
}

pub struct OperatorSpec {
    pub description: Option<String>,
    #[serde(flatten)]
    pub action: OperatorAction,
    pub budget: Option<OperatorBudget>,
}
```

`TriggerActivation` remains a `kind`-tagged closed union. Flattening makes the
inline binding body and the corresponding Card's `spec` identical. It does not
add a second Trigger or Operator wire shape.

The initial authored shape is:

```yaml
spec:
  components:
    - ref: churn-model.yaml
      verified_by:
        - verifier: churn-drift.yaml
          runs_on:
            kind: schedule
            cron: "*/5 * * * *"
            tz: UTC
          on_failure:
            - kind: notify
              channel:
                kind: slack
                connection: primary-workspace
                channel_id: C0123456789
                text: "Churn drift exceeded its configured threshold."
```

Shared Cards use the same fields without another binding shape:

```yaml
verified_by:
  - verifier: churn-drift.yaml
    runs_on: every-five-minutes.yaml
    on_failure:
      - notify-ops.yaml
```

An Eval binding uses `runs_on.kind: observations_ready` and has no schedule.
If `on_failure` is empty or omitted, failed results remain durable and no
reaction runs. Duplicate Operator identities within one binding are rejected.

A tenant with a PagerDuty Global Integration may instead configure a Notify
action such as:

```yaml
on_failure:
  - kind: notify
    channel:
      kind: pager_duty
      connection: primary-pagerduty
      route: model-oncall
      severity: warning
      summary: "Churn drift exceeded its configured threshold."
```

`connection` is a nonsecret tenant-scoped connection name, not a credential or
URL. Slack `channel_id` is the exact destination conversation. PagerDuty
`route` is an authored, nonsecret value placed in
`payload.custom_details.wyrd_route` for the tenant's PagerDuty service routes;
Wyrd does not map it to an escalation policy.

### Continuous verification runtime (locked initial architecture)

The registration and runtime control flow in
[verification-control-flow.html](architecture/verification-control-flow.html)
is the locked initial architecture for this change. The requirements below
state its observable contract; no older activation queue, Alert table, or
multi-table transaction design survives as an alternative.

- **REQ-073**: PSI and SPC Drift registration MUST persist the Verifier Card
  and a pending `drift_baselines` row before returning, without waiting for
  fitting. That row is the fitter's work/status record; no additional baseline
  queue is required. The server MUST resolve the
  exact registered Data Card and its Parquet artifact, fit the existing
  FittedBaseline with the authored DriftSpec profile, and expose pending,
  building, ready, or failed status. A failed or interrupted fit MUST remain
  visible and receive bounded retry through the same durable row. Custom Drift uses its authored scalar
  baseline and needs no fitted-profile job. A PSI/SPC run MUST NOT start before
  its exact fitted baseline is ready.
- **REQ-074**: The authorized Verifier status surface MUST expose Drift baseline
  state, exact Data Card identity, and a structured fitting error when present.
  Readiness is server-managed status, not a mutation of the authored Card.
- **REQ-075**: `observe.record`, `observe.drift`, and `observe.eval` MUST remain
  client abstractions over existing Bifrost observation ingestion. The
  existing `DriftRecordObservation` and `EvalRecordObservation` are the sole
  canonical logical input record types; their obsolete `drift_ref` and
  `eval_ref` fields and duplicated `run_id` fields MUST be removed, not
  replaced by new public record types.
  Runtime callers provide measurements or Eval context, while the run supplies
  subject CardRef and run correlation. The server selects matching active
  Verifier bindings from that authorized subject identity.
- **REQ-076**: The existing bounded client queue, Gate, and Scribe path MUST
  persist Drift inputs to vala.drift.observations and EvalRecordObservation
  inputs to the new Bifrost table vala.eval.observations. The existing
  EvalRecordObservation format MUST be carried by the existing generic
  wyrd-client/wyrd-queue path over the canonical IPC publication path to
  Gate and Scribe; there is no new observation envelope, ingest route, or
  downstream projector. Each `observe.drift(...)` / `observe.eval(...)` call
  MUST adapt its normalized logical record to the agreed fixed table schema
  before queue insertion; the existing explicit-table `WriterPool::insert`
  accepts row JSON bytes, the cached user schema, and per-row correlation,
  not an arbitrary feature dictionary as a table definition. Bifrost
  resolves and retains the fixed user table schemas at startup under REQ-127.
  The existing queue appends `card_ref` and `run_id`, builds Arrow columns
  from the described schema, seals Arrow IPC, and publishes through
  authenticated gRPC. Gate admits the authorized table write; Scribe checks
  each asserted subject against the signed Card scope, resolves `card_uid`,
  and stamps tenant and publisher identity.
  Scribe checks the registered table schema fingerprint, deduplicates the
  batch, and acknowledges only after its durable ingest boundary. An Eval
  run is considered for each matching active `observations_ready` binding
  only after the observation commits; Drift consumes committed observations
  in a later fixed-window analysis. Neither client queue admission nor a
  first-use table describe is a Scribe acknowledgement.
- **REQ-132**: The existing schema-driven JSON-row-to-Arrow builder MUST
  accept the existing typed IDs' lowercase-hex JSON values for described
  `FixedSizeBinary(16)` and `FixedSizeBinary(8)` columns, decode them to the
  exact byte widths, and reject malformed or wrong-width values. Eval
  observation `trace_id` and
  `span_id` MUST remain nullable binary columns matching `vala.traces.spans`
  for typed lookup and joins. This is generic Arrow datatype support in the
  existing Bifrost queue, not Eval-specific projection, a second publisher,
  or a second transport.
- **REQ-118**: A Drift or Eval input observation MUST retain the existing
  Bifrost identity split: authenticated Service/Agent `principal_id` is the
  publisher, and managed `card_uid` is the observed subject Card (the Service
  itself or its exact Model/Agent/component target), never the Verifier. Gate
  MUST authorize that subject `card_ref` through the publisher's signed Card
  scope. One logical input is stored once for that subject, without
  per-binding duplication or `verifier_uid` / binding columns on the raw
  observation. Matching active bindings create separate `verifier_runs` rows
  that freeze their exact Verifier and binding identities. A null or stale
  target MUST NOT be inferred from the service principal alone or retargeted
  when bindings later change.
- **REQ-116**: The unused built-in Bifrost table definitions
  `vala.eval.runs` and `vala.eval.assertions`, and the unused Postgres
  `vala.drift_alerts` table and its SQL API, MUST be removed in this change.
  They MUST NOT survive as alternative Eval results, mutable Drift alert
  state, compatibility paths, or generated public table definitions. Nothing
  using these tables has shipped, so no data migration or retained-data
  compatibility is required. Their old tests are replaced by coverage of
  `vala.eval.observations`, canonical Verifier results, and Operator dispatches.
- **REQ-120**: The existing process-local `/v1/eval/runs` pull protocol that
  requires `CardKind::Eval` MUST be retired with the unshipped Eval Card
  surface. This change MUST NOT retain it as a hidden second Eval execution
  path or repurpose it as continuous Eval. Future offline dataset evaluation
  may define its own Verifier-backed route when that journey ships.
- **REQ-077**: Once Scribe acknowledges an Eval observation, the server MUST
  attempt an asynchronous, idempotent insert of one verifier_runs row per
  matching active binding, keyed by tenant, binding, and record identity. The
  row MUST freeze `input_record_id` and `input_event_time`, where
  `input_event_time` is the exact server-managed `wyrd_event_time` assigned to
  the committed observation, not the client-authored `created_at`. The
  insert MUST NOT be part of Scribe's batch-fence transaction, delay or roll
  back the Bifrost acknowledgement, or add an outbox in this change. If enqueue
  fails or the process stops before it completes, Bifrost retains the record,
  no Eval run is guaranteed, and the server emits a structured tracing error.
  This best-effort loss is accepted for the initial delivery.
- **REQ-078**: Postgres owns `verification_bindings`, `drift_baselines`,
  `verifier_runs`, `operator_dispatches`, and tenant Operator connections:
  exact identities,
  scheduling cursors, claims, retries, and result pointers. Bifrost owns raw
  observations and analytical results. Bifrost MUST NOT be polled as a work
  queue. The generic VerificationRuntime contains the scheduler, Verifier
  runner, and Operator worker; only the Drift baseline fitter is
  implementation-specific background machinery. These five control tables
  MUST live in the `wyrd` Postgres schema and their migrations and typed SQL
  operations MUST be owned by `wyrd-sql`. Registration uses those operations
  through the caller-owned `TenantConn` so the Card, card-bound principal,
  binding projection, baseline work row, and connection validation share one
  transaction. Vala consumers use the `vala-sql` re-export rather than import
  `wyrd-sql` directly. All five tables MUST carry `data_tenant_id`, enable and
  force RLS, use the existing `wyrd.current_tenant()` policy, and be reachable
  by tenant runtime paths only through `TenantConn`; cross-tenant maintenance,
  if required, remains an explicit `OperatorPool` operation. No second Vala
  control schema, database, or repository layer is introduced.

The locked storage split is:

| Store | Existing/reused | Added by this change |
|---|---|---|
| Postgres control state | `wyrd.cards`, tenant machine principals in `wyrd.auth_service_accounts`, Scribe's `vala.scribe_batch_commits` fence | `verification_bindings`, `drift_baselines`, `verifier_runs`, `operator_dispatches`, encrypted tenant Operator connections, and one credentialless UUIDv7 SYSTEM principal per tenant; `last_authenticated_at` on the existing machine-principal row |
| Bifrost analytical state | `vala.drift.observations`; existing Scribe/Oracle table lifecycle | `vala.eval.observations`, `vala.verification.results`, `vala.drift.result_features`, `vala.eval.result_items` |
| Object storage | Registered Data Card Parquet artifact | No new baseline artifact format; the fitted profile is Postgres control state |

The removed `vala.eval.runs`, `vala.eval.assertions`, and
`vala.drift_alerts` tables are not part of this storage split. The result
dashboard starts from
`vala.verification.results`; Drift and Eval dashboards join their detail
table on `(data_tenant_id, result_id)`.
- **REQ-079**: A `verifier_runs` row MUST freeze the tenant, exact Verifier
  Card UID/version, subject, input record or window, origin, and effective
  Trigger and Operator identities. A binding-created run MUST freeze its
  non-null owner Card UID and binding identity. A direct run MUST store both as
  null rather than substituting the subject, Verifier, or caller. Every manual
  API-created run MUST also freeze the authenticated caller as
  `requested_by_principal_id`; scheduled and observation-created runs have no
  manual requester. The authorization audit remains the canonical permission
  decision record. The generic runner claims the run with
  bounded lease and token-fenced settlement. For Eval, it MUST use the frozen
  `input_event_time` to constrain the Bifrost input read to the corresponding
  UTC `wyrd_event_time` partition and `input_record_id`; it MUST NOT derive the
  partition from client `created_at`, search a moving lookback window, or scan
  every historical partition. It then loads that exact Card and dispatches its
  typed implementation. Engine errors retry the same run with
  bounded backoff and attempts; a valid failed verdict is a completed result,
  not an engine error. Expired claims are reclaimable. Terminal engine errors
  produce no verdict and no Operator dispatch.
- **REQ-080**: One scheduled Drift occurrence creates at most one run per due,
  ready, runtime-active binding. A due time fixes the comparison window: an
  hourly occurrence due at 01:00 queries [00:00,01:00) even if claimed at
  01:03. PSI bin counts, SPC aggregation, and Custom window means are computed
  on the server from tenant- and subject-scoped Bifrost observations selected
  for that binding's exact window and feature configuration before scoring
  with the existing Vala Drift engine. Insufficient or invalid input
  yields inconclusive, not a no-drift pass.
- **REQ-081**: The generic scheduler MUST use a short Postgres claim of the
  due binding row, create a unique verifier_runs row, and advance its cron
  cursor in the same transaction; the transaction releases the row lock
  before verification executes. It MUST filter on the existing service
  principal's last successful qualifying machine exchange and generic Verifier readiness
  before creating work. Inactive or unready due occurrences create no run and
  are not backfilled. After an outage, missed schedule occurrences are skipped
  rather than materialized in a catch-up batch; the next future cron boundary
  becomes the cursor. A claimed run retains its original fixed window.
- **REQ-112**: Composite owner registration MUST persist its resolved
  `verification_bindings` projection in the same registration transaction as
  the containing Card and its existing card-bound principal. A new binding's
  scheduled cursor is null until the first successful qualifying exchange by
  that exact Card-bound machine principal; that exchange sets the next future
  cron boundary. Eval bindings
  have no schedule cursor. Runtime activity is the existing
  `wyrd.auth_service_accounts.last_authenticated_at` value, not a new
  heartbeat or activity table. Existing principal uniqueness MUST permit two
  distinct Card UIDs/versions used concurrently for A/B operation.
- **REQ-082**: A manual Drift Verifier run MUST accept an explicit bounded
  observation window, return its durable run ID without waiting, and leave
  the binding's cron cursor unchanged. Direct Verifier invocation is
  analysis-only. Manual invocation of a binding uses its configured
  on_failure Operators. Both routes use the same runner and Drift scorer.
- **REQ-083**: Eval has no cron schedule. A successfully enqueued Eval run MUST
  load its committed `EvalRecordObservation` from `vala.eval.observations`,
  apply authored `EvalSpec.sampling`, and execute the existing deterministic,
  LLM-judge, workflow-aggregation, `pass_gate`, and `context_capture` behavior.
  Sampling is decided before trace lookup or task execution. A sampled-out
  record MUST settle `completed` with verdict `inconclusive`, persist one
  canonical result containing the existing zero-count `EvalWorkflowSummary`,
  persist no Eval item rows, and create no Operator dispatch. For a sampled-in
  record that requires trace data, temporary absence uses the existing
  `EvalStatus::AwaitingTrace` lifecycle and requeues the generic run until its
  bounded trace deadline. Trace absence at that deadline settles `timed_out`;
  trace-source execution failures use the normal bounded retry path and settle
  `errored` when exhausted. Neither terminal path writes a Verification Result
  or dispatches an Operator.
- **REQ-130**: Continuous Eval MUST use the existing `ScenarioScoring` /
  `EvalExecutor` task path, including its existing `JudgeTaskExecutor` and
  `SkaldJudgeInvoker`. Online records and later dataset-backed offline
  scenarios MAY differ in how records are obtained, but MUST converge on
  that same scoring and judging path; this change MUST NOT create separate
  online/offline judge executors. Dataset-backed offline execution remains
  outside this change's acceptance scope.
- **REQ-131**: The existing Eval `MediaRef` MUST remain the canonical
  record media item, extended with a required binding `id` matching one
  declared `${media:id}` prompt variable and an explicit supported media
  `kind`; it retains its URI and optional MIME type. Public SDK projections
  MUST distinguish this Eval media type from the existing Prompt media type.
  For an Eval run, the server MUST preserve record media when building the
  existing `MediaBindings`, validate named bindings against the resolved
  judge Prompt, and have the existing Skald judge invoker bind media to a
  per-call Prompt before invoking the model. A URI is a durable locator,
  not image/document content in the LLM request: the server MUST resolve
  only authorized, supported object-storage URIs under the run's tenant,
  read and validate bounded bytes and MIME/kind, and supply actual media
  through Skald's native provider multimodal input. It MUST NOT pass a
  private URI as prompt text, hand a private URI directly to a provider,
  add a second judge executor, or add provider-file-ID/upload lifecycle in
  this change. Missing, inaccessible, oversized, unsupported, or mismatched
  media is an input/execution error, not a failed assertion or pass-gate
  verdict. The same bindings must reach deterministic test invokers and the
  production Skald invoker; provider-specific media encoding stays in Skald.
- **REQ-084**: Only an Eval task whose executor successfully produces an
  `AssertionResult` attests. `passed: false` is a valid assertion outcome,
  including a valid structured judge judgment, and is evaluated only through
  the authored `pass_gate`; it is not an execution error and does not
  independently fail the Verifier. With at least one attesting task, an
  authored gate maps its existing verdict to common `passed` or `failed`; an
  absent gate maps the completed run to `inconclusive`. A completed execution
  with zero attesting tasks, including an all-skipped workflow, is
  `inconclusive` regardless of gate. Executor and input failures—including
  unresolved required context, malformed judge output, and missing, invalid,
  unauthorized, unsupported, mismatched, or oversized media—MUST remain on
  the existing execution-error path and MUST NOT become
  `AssertionResult { passed: false }`. Retryable failures use the run's bounded
  retry policy; exhausted or terminal failures settle `errored` with no
  verdict, result rows, or Operator dispatch. Only common `failed` dispatches
  Operators.
- **REQ-085**: Every completed Drift or Eval run MUST write one canonical
  summary and overall verdict to the new Bifrost table
  vala.verification.results, identified by `data_tenant_id` and `result_id`. Drift
  feature scores/thresholds/verdicts MUST be written to the new
  vala.drift.result_features table; every `EvalReport.outcomes`
  `TaskRunOutcome`, including `Skipped`, MUST be written as one row to the new
  vala.eval.result_items table. The common summary's `details` column MUST
  contain the serialized `EvalWorkflowSummary` for Eval. For Drift it MUST
  contain the serialized `DriftReport` when scoring produced one and MUST be
  null when a valid completed inconclusive execution ended before scoring,
  including an empty Custom window or invalid stored Custom metric input. A
  Drift result without a report writes zero feature rows. Non-finite numeric
  values in a produced report use the canonical JSON null projection and the
  nullable feature score/threshold columns. A sampled-out Eval run writes its
  zero-count summary and zero item rows; an all-skipped Eval run writes its
  summary and `Skipped` item rows. Errored and timed-out runs write neither
  result table.
  The common table MUST have no Drift- or Eval-specific summary columns.
  The exact five physical Arrow schemas are fixed by
  [table_schema.md](architecture/logic/table_schema.md). Each detail table
  joins to the summary on (`data_tenant_id`, `result_id`). Existing Drift and
  Eval engine result types are mapped into these tables, not replaced by new
  semantic result types. Verification dashboards query the summary; Drift
  and Eval dashboards join their corresponding detail table. Raw
  observations are not result-detail tables.
- **REQ-119**: Every canonical result and Drift/Eval detail row MUST have
  managed `card_uid` equal to the exact Verifier Card UID, selected by its
  UID-bearing `card_ref`; its server-stamped `principal_id` identifies the
  internal writer, not the observed subject. Managed `run_id` MUST be the exact
  verification run ID. The analytical payload MUST also
  carry exact `subject_card_uid`, nullable `owner_card_uid`, nullable
  `binding_id`, `result_id`, and the
  source record identity or fixed Drift window. This permits queries by
  Verifier, subject, owner, binding, run, and time without overloading the
  single managed `card_uid`. A baseline Data Card, effective Trigger, and
  Operators remain typed references in Verifier/binding/control state; they
  are not substituted for the result row's Verifier Card identity.
- **REQ-121**: Bifrost row identity MUST keep writer and subject separate.
  For `vala.drift.observations` and `vala.eval.observations`, `principal_id`
  MUST identify the authenticated client Service/Agent publisher and managed
  `card_uid` MUST identify the observed subject Card; raw rows do not carry a
  Verifier UID or binding ID. For
  `vala.verification.results`, `vala.drift.result_features`, and
  `vala.eval.result_items`, `principal_id` MUST identify the tenant-scoped
  internal SYSTEM writer and managed `card_uid` MUST identify the exact
  Verifier Card; `subject_card_uid`, nullable `owner_card_uid`, and nullable
  `binding_id` MUST remain separate columns. Binding-created results populate
  both owner and binding; direct results set both to null. One client
  observation may feed multiple
  binding-created runs without changing or duplicating its subject Card
  identity. All five
  tables retain Bifrost's physical `data_tenant_id`; it is the tenant key for
  result/detail joins, not a caller-authored `tenant_id` field.
- **REQ-122**: All five verification Bifrost tables in this change—
  `vala.drift.observations`, `vala.eval.observations`,
  `vala.verification.results`, `vala.drift.result_features`, and
  `vala.eval.result_items`—MUST partition by UTC day on Bifrost's managed
  `wyrd_event_time`, not by Verifier, subject, binding, or tenant ID. This
  changes the existing Drift observation table's hourly layout to daily;
  the other four tables are new. `vala.verification.results`' declared Bloom
  columns MUST be `result_id`, `subject_card_uid`, and `binding_id`, in
  addition to Bifrost's schema-present managed Bloom floor (`run_id`,
  `card_uid`, `principal_id`).
  `vala.drift.result_features` and `vala.eval.result_items` MUST Bloom-filter
  `result_id` for equality lookups into the details. All rows produced for one
  result MUST carry the same server-chosen result event time, distinct from
  the input observation time or Drift window end; separate ACKs therefore do not split
  a successful result across day partitions. An hourly Drift cron window
  remains hourly; it does not determine the physical partition size. The
  result/detail join remains (`data_tenant_id`, `result_id`); Bloom filters and partition pruning are
  performance aids, never identity or authorization boundaries. Bifrost's
  existing default `wyrd_event_time DESC` sort applies; this change adds no
  new partition transform, index service, or custom join mechanism.
- **REQ-086**: The server MUST send the result tables as Arrow batches through
  `wyrd_client::Bifrost` over its existing authenticated Arrow/gRPC path back
  through the Wyrd server's Gate to whichever Scribe owns the batch. A runner
  MUST NOT assume a Scribe is active in its own server process or write
  directly to local Scribe state. Tenant provisioning and upgrade migration
  MUST idempotently create exactly one internal `system` principal row named
  `verification-results-writer` for each tenant in the existing tenant machine
  principal store. Its server-minted `PrincipalId` MUST be UUIDv7 and remain
  stable after provisioning. `system` is the sixth `PrincipalKindTag` wire
  value and a tenant-plane `PrincipalKind`, not a second identity hierarchy.
  The row has no bound Card or user-managed lifecycle. It is not a Card, API
  key, refresh token, role grant, workload binding, or user-manageable
  principal. Public create, list, get, update, suspend/delete, credential,
  refresh, workload, delegation, and token-exchange operations MUST reject or
  omit it as appropriate; it remains representable in token and audit wire
  contracts so internal writes are attributable.

  Immediately before each result-publication attempt, the runner MUST mint a
  normal five-minute-or-shorter Wyrd access token through the existing tenant
  token issuer using that tenant's persisted SYSTEM
  principal. The token MUST contain the run tenant, `kind=system`, no roles or
  credential attribution, delegation chain, or bound root Card, and signed Card
  scope containing exactly one UID-bearing `Verifier` CardRef: the run's exact
  Verifier version. Verification MUST reject
  a missing or non-UUIDv7 persisted principal, a mismatched tenant, a bound root
  Card, any role, an empty or multi-Card scope, a non-Verifier scope member, or
  a scope member without managed Card UID. Public API-key, refresh, JWT-bearer,
  delegation, workload-binding, principal-management, and token-exchange paths
  MUST reject creation, credentialing, impersonation, delegation, or refresh
  of `system`. Expiry or retry mints a new short-lived token; no credential is
  persisted.

  SYSTEM minting evaluates no end-user permission and emits no authorization
  audit row. Gate's result-table admission is the one
  `bifrost_record:write` authorization decision and uses the canonical audit
  path. No second issuer, token format, credential table, or identity store is
  introduced.

  A valid SYSTEM result token receives only the existing
  `bifrost_record:write` permission. Gate MUST reserve exactly
  `vala.verification.results`, `vala.drift.result_features`, and
  `vala.eval.result_items` for this principal kind. A write to one of those
  tables requires `kind=system`, that permission, and the exact signed Verifier
  scope; every other principal, including wildcard administrators, is denied.
  SYSTEM is denied every other table. Gate MUST enforce and audit this as its
  one canonical `bifrost_record:write` decision. Scribe MUST continue to derive
  tenant and `principal_id` from authenticated authority, authorize every row's
  Verifier CardRef against signed scope, and stamp the managed Verifier Card
  UID; neither tenant nor Card UID may be trusted from Arrow payloads.
  `Verifier` is therefore an eligible scoped Bifrost target only for this
  internal path. The global `SYSTEM_OWNER` tenant, platform audit principal,
  and audit-publisher identity MUST NOT be reused.

  Every non-empty required detail batch is written before the canonical
  summary batch, and each is separately acknowledged. A result with zero
  details—such as sampled-out Eval or pre-scoring inconclusive Drift—writes no
  empty detail batch and requires only the summary acknowledgement. Their writes
  are not atomic across tables. Only after every required batch is
  acknowledged may the runner settle verifier_runs as completed and create
  Operator dispatches. Partial result rows may be visible after a failed
  write or crash; this initial change accepts that limitation and does not
  add a per-table crash-recovery protocol or claim atomic result visibility.
- **REQ-087**: Scribe acknowledgement means its existing WAL, batch fence,
  and active rows accepted that sealed batch; it does not mean Iceberg
  publication. A retry of an unacknowledged result batch MUST preserve the
  same sealed Arrow payload, table, and batch ID so Scribe can deduplicate it.
  A fresh write_batch call creates a new batch ID and MUST NOT be described
  as deduplicated replay. If a process crash loses an unacknowledged payload,
  the run may remain partial and end errored; it MUST NOT dispatch an Operator
  as though all result tables were acknowledged. No stronger cross-table
  crash guarantee is claimed.
- **REQ-097**: For a completed binding-created run with failed verdict, the
  generic runner MUST settle the run and insert one operator_dispatches row
  for each distinct configured Operator UID or inline-spec digest in the
  same Postgres transaction. Unique (tenant, run, Operator) identity makes
  settlement retry idempotent. Passed and inconclusive completed runs,
  including sampled-out, ungated, and all-skipped Eval, plus cancelled,
  timed-out, and errored runs create no dispatch. Direct analysis-only runs
  create no dispatch.
- **REQ-098**: A separate generic Operator worker MUST poll committed due
  operator_dispatches rows, claim each with a short transaction and expiring
  lease (using the existing Postgres skip-locked claim pattern), then execute
  its referenced or inline supported OperatorSpec. Each dispatch
  has independent status, bounded retry, and terminal error; one failure
  does not block siblings or rewrite the Verifier result. There is no
  direct Verifier-to-Operator call, separate broker, Alert table, or
  Alert-polling loop.
- **REQ-099**: The Verifier runner MUST only create durable
  `operator_dispatches` tasks for a completed binding-created run with a
  `failed` verdict; the independent Operator runner MUST claim each committed
  task and invoke its supported Notify or HTTP action. A Notify action sends
  to its configured destination without creating an Alert resource. A later
  passing Verifier run does not mutate or resolve an earlier failed result or
  dispatch. Provider acceptance, rather than a downstream Slack read receipt
  or PagerDuty incident/page, is the delivery success boundary.
- **REQ-138**: Every Operator invocation MUST use one bounded, immutable
  failure context derived from the acknowledged Verification Result. Its only
  fields are dispatch, run, result, and binding IDs; exact Verifier and subject
  Card identities; the `failed` verdict; completion time; and a bounded
  human-readable result summary. It MUST NOT include raw Eval context, media,
  Drift feature rows, arbitrary result detail, or secret material. Notify
  rendering and HTTP URL/header/JSON-body templates may access only these
  fields; registration MUST reject unknown template fields. A later Workflow
  invocation MUST receive this same context, not a second payload contract.
- **REQ-139**: The server MUST resolve every external Operator credential at
  invocation from Postgres under the exact authenticated run tenant, provider,
  and named connection. A tenant may not name or read an arbitrary process
  environment variable or another tenant's credential through an Operator
  Card. Each Notify channel MUST name a nonsecret `connection`; existing HTTP
  auth variants MUST retain their scheme/header shape but replace every `env`
  selector with that tenant-scoped connection name. The runner resolves and
  decrypts the latest active credential immediately before each attempt, so
  all replicas observe the same durable connection and rotation does not
  require a Card revision or server rollout. Missing, disabled, wrong-provider,
  and wrong-tenant connections fail closed through indistinguishable safe
  errors. Secret values never enter Card specs, dispatch/result rows, status or
  connection-read responses, logs, traces, audit payloads, or diagnostics.
- **REQ-147**: Postgres MUST persist each Operator connection under a
  server-minted UUIDv7 connection ID with tenant, closed provider
  (`slack | pager_duty | http`), immutable validated name, provider-specific
  nonsecret configuration, `active | disabled` status, encrypted credential,
  encryption-key version, creator/updater principal identities, and timestamps.
  `(data_tenant_id, provider, name)` MUST be unique and tenant access MUST use
  `TenantConn` with forced RLS. Postgres MUST never contain plaintext secret
  material. Each secret version MUST use tenant-scoped envelope encryption: a
  cryptographically random data-encryption key, authenticated encryption with
  canonical tenant/connection/provider/name/version context, and a wrapped key
  bound to an externally held tenant-scoped key-encryption key and key version.
  Root or key-encryption material MUST remain outside Postgres in the deployment
  secret/KMS boundary. Rotation replaces the encrypted secret on the same
  connection identity; key rotation MUST support rewrapping or re-encryption
  without exposing plaintext through a public surface. Decrypted bytes exist
  only for the bounded delivery attempt in a redacted secret type and are not
  cached across attempts. The implementation MUST reuse `wyrd-crypt`'s
  AES-256-GCM and operating-system randomness, extended to authenticate the
  canonical associated data
  `(domain, data_tenant_id, connection_id, provider, name, secret_version)`;
  no second cryptography package or bespoke cipher is introduced. Each secret
  version gets a fresh 256-bit DEK and nonce. That DEK is separately wrapped
  under the exact tenant KEK version using the same authenticated primitive
  with a distinct domain tag.

  The deployment key provider is the existing external-secret resolver and
  its `SecretRef::Vault` boundary (the configured backend may be Vault, AWS
  Secrets Manager, or Google Secret Manager). Server configuration MUST supply
  one external key prefix and one active positive key version; the resolver
  reads a 32-byte KEK from
  `<prefix>/<data_tenant_id>/<key_version>`. Multi-tenant production MUST use
  this external provider and MUST fail startup if it or the active key is
  unavailable; environment-sourced KEKs are development-only, while a
  restrictive file-mounted KEK MAY be used by an explicitly single-tenant
  deployment. Postgres stores only the key version and wrapped DEK. KEK
  rotation publishes a new external version before making it active; new
  writes use it, existing rows are rewrapped in bounded tenant-scoped work,
  and an old external version is retained until no row references it. Rewrap
  exposes only the DEK inside the process and does not decrypt the credential.
- **REQ-148**: Tenant administrators MUST manage Operator connections through
  typed server-owned operations: create; list/get redacted metadata; update
  nonsecret configuration, status, or the write-only secret; and disable. The
  HTTP surface MUST provide `POST /v1/operator-connections`,
  `GET /v1/operator-connections`,
  `GET /v1/operator-connections/{connection_id}`,
  `PATCH /v1/operator-connections/{connection_id}`, and
  `DELETE /v1/operator-connections/{connection_id}`. Provider and name are
  immutable after creation. An omitted secret on PATCH preserves it; a supplied
  secret atomically replaces it. DELETE disables and retains the row for Card,
  dispatch, audit, and rotation lineage rather than physically deleting it;
  PATCH may re-enable a valid connection. Reads expose ID, provider, name,
  redacted nonsecret configuration, status, and timestamps only. Rust, Python,
  TypeScript, CLI, and MCP MUST project the same typed operations; MCP writes
  require explicit write scope. Reads require existing `operators:read` and
  mutations existing `operators:write`, with the canonical transactional audit
  decision. No read-secret or plaintext export operation exists.
- **REQ-150**: Operator-connection writes MUST use one closed provider-tagged
  wire union rather than a free-form configuration map. Create shapes are:
  `slack { name, workspace_id, bot_token }`,
  `pager_duty { name, integration_key }`, and
  `http { name, origin, auth }`, where HTTP `auth` is exactly
  `bearer { token }`, `basic { username, password }`, or
  `header { name, value }`. `origin` is a normalized HTTPS
  scheme/host/effective-port tuple with no path, query, fragment, or userinfo.
  PATCH uses the matching provider-specific update shape plus optional status:
  omitted fields are preserved; a supplied Slack token, PagerDuty integration
  key, or complete HTTP auth value replaces the encrypted secret atomically;
  a supplied HTTP auth value may also replace its scheme/header authority.
  Provider and connection name never change. Redacted reads return the common
  ID/provider/name/status/timestamps and only `workspace_id` for Slack, no
  PagerDuty configuration, or HTTP origin plus auth scheme and custom-header
  name; they never return a token, key, username, password, or header value.
  `OperatorConnectionId` is the typed UUIDv7 used by path parameters and
  responses.

  Operator Cards use those exact authorities: Slack carries
  `{ connection, channel_id, text }`; PagerDuty carries
  `{ connection, route, severity, summary }`; HTTP auth is omitted for an
  unauthenticated request or is one of `bearer { connection }`,
  `basic { connection }`, or `header { name, connection }`. Thus a Card never
  selects an environment variable or contains a credential. There is no
  unauthenticated HTTP connection record because an auth-less HTTP Operator
  has no secret to resolve.
- **REQ-149**: Registration of a binding or Operator using a connection MUST
  verify that the exact tenant/provider/name exists, is active, and is
  compatible without decrypting it. Slack connections bind one workspace bot
  token; PagerDuty connections bind one Global Integration key. An HTTP
  connection additionally binds one normalized HTTPS origin and the exact
  auth scheme and, for custom-header auth, case-insensitive header name.
  Registration and every delivery attempt MUST require the Operator's HTTP
  auth variant/header and effective URL origin to equal that stored authority;
  a mismatch fails before secret decryption. Every effective templated HTTP URL and redirect MUST
  retain that credential authority and pass the repository resolve-screen-pin
  SSRF policy before credentials are attached; origin-changing redirects are
  rejected. Operator-authored headers MUST NOT set `Authorization`, `Host`,
  `Content-Length`, `Transfer-Encoding`, `Connection`, or `Idempotency-Key`.
  Missing, empty, malformed, or unauthorized credentials are terminal;
  transient Postgres, key-provider, or decryption-service unavailability uses
  REQ-142's bounded retry path.
- **REQ-140**: Slack Notify MUST use one tenant-scoped bot-token connection per
  Slack workspace and the authored `channel_id`, calling Slack
  `chat.postMessage` with `chat:write`. The app must be permitted to post in
  that conversation; Wyrd does not require `chat:write.public`, manage channel
  membership, or store one incoming webhook URL per channel. The Operator
  runner MUST inspect Slack's JSON `ok` field, not only its HTTP status.
  `ok: false` permission, token, channel, or payload errors are terminal;
  rate limits and transient provider errors follow REQ-142.
- **REQ-141**: PagerDuty Notify MUST send an Events API v2 trigger event using
  the tenant's named Global Integration connection and authored `route`. The
  event MUST carry that route in `payload.custom_details.wyrd_route` so the
  tenant can match it in PagerDuty Service Routes, plus the configured
  severity and summary, an exact subject source, and a stable `dedup_key`.
  If `dedup_key` is authored,
  the rendered value is frozen for this dispatch; otherwise it is the
  dispatch ID. PagerDuty owns service routing and escalation. Wyrd MUST NOT
  provision one integration key per team/service, use a PagerDuty REST key,
  or claim that Events API acceptance proves an incident or page was created.
  PagerDuty deduplication may group repeated trigger events; it is not an
  exactly-once delivery guarantee.
- **REQ-142**: External Notify and HTTP attempts MUST have finite per-attempt
  timeouts, a finite total attempt budget and dispatch deadline, and a server
  ceiling over authored Operator/HTTP timeouts. The Operator runner retries
  connection failures, timeouts, temporary credential-store unavailability,
  HTTP 408/429/5xx, and provider-declared
  transient failures with bounded backoff; it honors `Retry-After` only within
  the dispatch deadline. It fails terminally for missing or unauthorized
  credentials, invalid destinations or templates, ordinary 4xx, and provider
  permission/configuration errors. Every retry uses the same dispatch ID and
  frozen destination and payload. The effective templated HTTP URL MUST be
  screened and connection-pinned under the repository's SSRF policy before
  each send; redirects and response bodies remain bounded. HTTP supplies the
  dispatch ID in an `Idempotency-Key` header that the destination may honor;
  an authored header MUST NOT override it. An
  ambiguous external send may be delivered more than once; no Slack
  destination deduplication is promised. REQ-146 fixes the numeric ceilings
  and worker capacity.
- **REQ-143**: Until the separate server Workflow invocation change lands,
  the `workflow` Operator action is a typed but non-executable placeholder.
  It MAY be registered as an Operator Card for authoring, but it MUST be
  rejected in a verification binding's `on_failure` during registration.
  Neither the Operator runner nor a direct Verifier run may report an
  uninvoked Workflow as delivered.
- **REQ-144**: The unused `vala-core::alert_router` signed-webhook
  configuration and its `vala-core` crate MUST be retired with their schemas,
  generator, workspace registration, and dedicated schema-drift check. No
  production code consumes that config and this change introduces no Alert
  router, signed-webhook destination, or second Operator delivery path.
  The removed schema-drift check protected only this retired config, so its
  guarded property is no longer reachable. Shared `SecretRef` and `TlsConfig`
  remain because they have other consumers.
- **REQ-100**: Authorized status surfaces MUST expose Drift baseline
  readiness, binding schedule/readiness and last activation, Verifier run
  status and result pointer, and each Operator dispatch/delivery status.
  The manual Drift route and manual binding invocation MUST be available
  for tests and users. Status and result reads MUST preserve tenant isolation.
- **REQ-134**: The existing `GET /v1/cards/by-uid/{kind}/{card_uid}` is the
  Verifier baseline-status read; it MUST return the current server-derived
  `card.status.verification.baseline` for PSI/SPC, including state
  (`pending | building | ready | failed`), exact baseline Data Card identity,
  and structured fitting error when present. The same Card read for a
  Service or standalone Agent with `verified_by` MUST expose its derived
  `card.status.verification.binding_ids` so a caller can address each inline
  binding without a new listing resource. Neither field changes authored
  `spec` or makes a binding a Card.
- **REQ-135**: The only new Verification HTTP operations in this change MUST
  be `GET /v1/verification/bindings/{binding_id}`,
  `POST /v1/verification/runs`, and
  `GET /v1/verification/runs/{run_id}`. Binding GET MUST return exact owner,
  subject, and Verifier Card identities, the current principal-activity gate,
  readiness and reason, nullable `next_run_at` (null for Eval), nullable
  `last_activated_at`, and nullable `last_run_id`. Run GET MUST return run ID,
  execution status, nullable `requested_by_principal_id`, nullable `result_id`,
  structured execution error when present, and the current independent
  delivery status of each configured Operator dispatch. A manual run returns
  its authenticated requester; scheduler- and observation-created runs return
  null. It MUST NOT duplicate the authoritative verdict or
  Drift/Eval detail rows from Bifrost. A run may point to an acknowledged
  result before it becomes visible to an analytical query.
- **REQ-136**: `POST /v1/verification/runs` MUST accept one tagged target:
  `binding { binding_id }` or direct `verifier { verifier_uid,
  subject_card_uid }`. The initial input variant is `drift_window { start,
  end }` with a validated bounded UTC half-open window `[start, end)`; Eval
  remains observation-triggered in this change. The server MUST return
  `202 { run_id }` after durable enqueue, without waiting for scoring.
  Binding target uses its frozen Trigger/Operator configuration and MAY
  dispatch on failed verdict; direct Verifier target is analysis-only and
  MUST NOT dispatch. Every manual call MUST be authenticated, authorized with
  `evals:run` plus exact target scope, transactionally audited, and persisted
  with that caller's `principal_id`; the credential identifies the requester,
  not a binding owner Card. Request retries MUST use the existing HTTP
  `Idempotency-Key` contract, not a new activation resource. Invalid target,
  window, unauthorized subject, tenant mismatch, or unready baseline MUST
  fail before enqueue with a structured Wyrd error.
- **REQ-137**: Rust, Python, and TypeScript MUST project the same typed
  `get_binding`, `start_run`, and `get_run` operations through the shared
  `wyrd-client` Verification capability; TypeScript uses `getBinding`,
  `startRun`, and `getRun`. Python uses its existing synchronous SDK boundary;
  Rust and TypeScript await these network operations. Baseline status uses
  the existing Cards read
  capability, and analytical results use the existing Bifrost query API with
  `result_id` and the registered result/detail tables. No SDK implements a
  separate status engine or adds a result/dispatch transport. MCP MUST expose
  `cards.get`, `verification.get_binding`, `verification.start_run`, and
  `verification.get_run` as typed projections of those same server
  operations. `verification.start_run` requires an explicit write scope;
  result data uses the existing `bifrost.query` tool. No separate MCP
  result or Operator-status tool is introduced.
- **REQ-101**: The initial change MUST prove the entire registered
  Service/Agent-to-Operator journey for PSI, SPC, Custom Drift, deterministic
  Eval, and LLM-judge Eval through real SDK, server, Postgres control state,
  canonical client queue/IPC observation ingest, Bifrost query and Arrow/gRPC
  result writes, scheduling or post-commit enqueue,
  result persistence, and status. Cron, manual activation, worker lease,
  retry, fail-open Eval enqueue, partial result write, Operator fanout,
  notification delivery, restart, authorization, and tenant isolation are
  required evidence. Offline dataset/scenario evaluation is excluded from
  this initial journey.
- **REQ-113**: Implementation MUST reuse the existing Card envelope,
  composite registration, UID-pinned `Ref`/`InlineableRef` resolution,
  card-bound principal/token exchange, `WyrdState` request-driven re-exchange,
  `DriftRecordObservation` and `EvalRecordObservation` with their obsolete
  implementation-ref fields removed and the existing Eval `MediaRef` extended
  only for named native media binding, bounded
  wyrd-client/wyrd-queue IPC publication, Gate/Scribe append and batch fence,
  Oracle reads, `vala-drift` fitting/scoring, and `vala-eval` planning/executor
  and result types. Only the missing binding projection, generic scheduling,
  run/dispatch control state, Drift baseline fitting orchestration, Eval
  post-commit enqueue, media binding through the existing judge path,
  result-table writers, and public status/manual surfaces are added. No new
  downstream projector, observation envelope, Eval engine, generic
  broker, or Alert persistence path is permitted.
- **REQ-115**: `wyrd-server` MUST supervise one generic
  `VerificationRuntime` containing Scheduler, Verifier runner, and Operator
  worker capabilities. All worker concurrency, queue claiming, engine calls,
  external delivery, and shutdown drain MUST be bounded. A process restart
  MUST reclaim expired Postgres leases and expose pending, retrying, and
  terminal failures through status and structured telemetry; it MUST NOT rely
  on process-local-only run or dispatch state. No new network-serving role or
  kind-specific scheduler is created. The Drift baseline fitter is the sole
  implementation-specific background helper.
- **REQ-145**: Verification MUST reuse the existing RBAC permissions. Composite
  registration and binding changes require `cards:write`; a binding that names
  an `on_failure` Operator also requires `operators:invoke`. Fixed-table
  describes during `start_bifrost` and lazy dynamic-table describes require
  `bifrost_table:read`. Drift, Eval, and generic record admission require
  `bifrost_record:write` plus the existing signed Card scope for the exact
  observed subject; generic record retains the reserved-table refusal.
  Verification binding and run status require `cards:read`; reading analytical
  results additionally requires `bifrost_query:read`. Direct and binding-backed
  manual Verifier runs require `evals:run` plus scope over the exact target.
  Operator-connection metadata reads require `operators:read`; create, secret
  rotation, metadata/status update, disable, and re-enable require
  `operators:write`. No connection-management operation is authorized by
  `operators:invoke` alone.
  Each boundary that evaluates one of these permissions MUST append its allow
  or deny through the canonical transactional audit path. The durable binding
  freezes the already-authorized Operator; the tenant-scoped SYSTEM worker
  executes that frozen dispatch without reevaluating an end-user permission.
  Scheduler ticks, claims, leases, retries, Scribe commits, and worker mechanics
  evaluate no principal permission and MUST NOT emit authorization audit rows.
- **REQ-146**: The initial VerificationRuntime MUST use one scheduler task, a
  shared Verifier/baseline execution ceiling of 16 globally and 4 per tenant,
  and an external Operator execution ceiling of 16 globally and 4 per tenant.
  A worker MUST acquire both applicable permits before claiming durable work.
  An external Operator dispatch has three total attempts, a 30-second timeout
  per attempt, a five-minute deadline from dispatch creation, and retry delays
  of 30 seconds then two minutes; `Retry-After` is honored only when clipped to
  that deadline. These are server ceilings and cannot be raised by a Card.
  Shutdown MUST stop new claims immediately, allow 30 seconds for in-flight
  work, then cancel remaining work and release or expire its fenced lease for
  retry with the same durable identity. The server MUST restart an unexpectedly
  exited runtime task and report unhealthy while a required capability is
  absent. Existing tracing and metrics MUST expose queue depth, active work,
  attempts, failures, and latency; this change adds no new telemetry service or
  process-local work registry.
- **REQ-152**: Verification coordination MUST use PostgreSQL as its clock.
  PostgreSQL MUST write and evaluate runtime activity, schedule eligibility,
  run and dispatch availability, claim and lease expiry, retry/backoff
  availability, worker deadlines, and coordination-relevant creation/update
  times with `statement_timestamp()`. Immediate timestamps are assigned in the
  owning SQL statement; relative deadlines are computed there from a bound
  duration; and expiry or eligibility is compared there. When Rust needs a
  database deadline decision, the same statement MUST return the verdict or
  remaining interval rather than exposing a timestamp for comparison with a
  process wall clock. An explicitly authored or cron-derived future schedule
  remains an allowed bound instant, but its "next future" anchor MUST be a
  PostgreSQL timestamp returned by the owning statement. Rust `Instant` remains
  the clock for process-local timeouts, sleeps, and latency, while producer
  clocks remain authoritative for event facts such as observation time,
  verification execution start/end, and JWT `iat`/`exp`. Verification MUST NOT
  add a clock abstraction, skew tolerance, synchronization setting, safety
  margin, or permanent source checker.
- **REQ-114**: Before this change is complete, the owning Card and runtime
  architecture authorities, generated schemas, and public documentation MUST
  describe the new Verifier-only verification model. In particular, the
  current 16-kind catalog, `publishes_to` monitor routing, and standalone
  Drift/Eval/Trigger firing prose in `architecture/wyrd-design.md` and
  `architecture/wyrd-doctrine.mdx` MUST be replaced by the approved
  `verified_by` binding and locked generic runtime semantics. No stale
  architecture may remain as an apparent alternative to the control-flow
  diagram. `architecture/bifrost-design.md` MUST also recognize the five
  daily-partitioned verification tables instead of claiming every built-in
  table except audit is hourly. The Run identity section MUST also reflect
  `run_api.md`'s one invocation ID with exact per-observation CardRef scope,
  rather than claiming that every Card switch requires a distinct Run ID.
  Operator documentation MUST also describe tenant-bound connections,
  their encrypted Postgres management operations,
  Slack bot-token/channel-ID delivery, PagerDuty Global Integration routing,
  the unavailable Workflow invocation, and removal of the unused alert router.
- **REQ-089**: The supported Drift and continuous Eval paths MUST have no
  process-local-only run state, registration-only relationship, or provisional
  storage path standing in for the documented server workflow. A successfully
  enqueued run MUST reach completed or a visible bounded retry/terminal state.
  This does not promise an Eval run when its best-effort post-commit enqueue
  fails, nor atomic visibility across the separate Bifrost result tables.

- **REQ-061**: Drift and Eval MUST produce the same common Verification Result
  shape. Future implementations can project that core result without changing
  the meaning of its existing fields.
- **REQ-062**: Every result MUST bind the exact tenant, run identity, subject,
  Verifier Card UID/version, input identity, execution status, and, for a
  completed run, verdict and available summary. Binding-created Drift and Eval
  results bind their owner, verification binding, effective Trigger, and
  observation record or comparison window. Direct results have no owner,
  binding, Trigger, or Operator identity; their run retains the authenticated
  requesting principal. Results MUST NOT fabricate a Change Request, Claim, or
  code Evidence.
- **REQ-063**: Execution status and verdict MUST remain independent. Only a
  completed execution may produce `passed`, `failed`, or `inconclusive`.
  Cancelled, timed-out, and errored executions have no verdict.

## Invariants

- **INV-001**: The shipped continuous user model is an existing Service/Agent
  Card with inline `verified_by` bindings to Verifier, Trigger, and optional
  Operator Cards; runtime work rows are not authored Cards.
- **INV-004**: Missing, stale, unavailable, errored, cancelled, timed-out, or
  inconclusive verification cannot become a pass.
- **INV-006**: Verifier Cards contain no secret material or mutable runtime
  health.
- **INV-007**: Tenant isolation and transactional authorization audit apply
  to the registration, run, result, and Operator surfaces in this change.
- **INV-010**: Bifrost is authoritative for observations and analytical
  results; Postgres is authoritative for runnable work, claims, retries,
  schedule cursors, and operational status.
- **INV-011**: Trigger decides when its Verifier runs; Verifier decides the
  verdict; the generic runner creates zero or more binding-configured
  `on_failure` dispatches only after `failed`; each Operator owns its own
  reaction and delivery status. No separate Alert resource or table exists,
  and Drift/Eval implementations do not grow their own scheduling or
  notification systems.
- **INV-012**: This change reuses existing Drift, Eval, observation, execution,
  and Bifrost contracts inside the new Verifier Card. It MUST NOT replace
  their working algorithms or record semantics merely to implement the missing
  server machinery and plumbing; removing obsolete implementation refs is
  the only intentional logical input-record subtraction. The existing Eval
  media item gains binding identity/kind so its already-authored media can
  reach the existing multimodal judge path.
- **INV-014**: `Verifier` is the only registrable Card kind for verification.
  `Drift` and `Eval` are typed implementations, not parallel Cards, hidden
  resources, or alternate registration paths.
- **INV-015**: The system evaluating a verification time predicate owns the
  timestamp used by that predicate. A PostgreSQL coordination column is never
  written from a Rust wall-clock instant and later compared by PostgreSQL, and
  Rust never evaluates a PostgreSQL-owned deadline against its wall clock.
- **INV-013**: Inline and referenced Trigger or Operator definitions have the
  same semantics. Inline definitions remain part of the containing Card version
  and MUST NOT create hidden Cards. Referenced definitions retain their own Card
  identity and may be shared by multiple verification bindings.

## Acceptance obligations

The executable acceptance scope is common Verifier contract and loader
coverage for Drift and Eval plus the production Drift/Eval journeys below.

- **AC-004**: Card loader and generated-schema evidence cover real Drift and
  Eval Verifier YAML, CardRef/path resolution where permitted, unknown
  implementation kinds, unknown fields, invalid refs, invalid Drift
  method/signal pairs (including `Spc` + `Metric`), and secret rejection.
  Future implementation kinds are rejected rather than emitted as public
  variants. A Workflow Operator Card remains parseable, but binding it to
  `on_failure` is rejected before persistence.
- **AC-012**: Real SDK-to-server journeys register representative Pandas,
  Polars, and Arrow Data Cards as Parquet, register PSI and SPC Drift
  Verifiers, inspect non-blocking baseline status until ready, and manually
  analyze bounded windows without an Operator dispatch. They prove
  server-side PSI bin counts and SPC aggregation, persisted
  vala.verification.results and vala.drift.result_features rows joined by
  (`data_tenant_id`, `result_id`), and queryable results. A Custom metric journey
  proves ready registration without a fit job and server-side window mean.
  Empty and invalid Custom windows MUST persist `completed/inconclusive` with
  null `details` and zero feature rows; scored Drift MUST persist its existing
  report and produced feature rows.
  Invalid/non-Parquet baselines, failed fitting, insufficient data,
  unauthorized access, and cross-tenant reads fail visibly.
- **AC-013**: A real Service binding journey registers a Drift Verifier,
  Trigger, and one or more Operators; emits existing DriftRecordObservation
  through observe:drift and the same wyrd-client/wyrd-queue path used by
  observe:record; proves cron due-window selection, active-principal gate,
  no backfill for inactive/unready/missed occurrences, and one isolated run
  per binding when two Services share a Trigger Card. A failed scheduled
  result creates one independent operator_dispatches row per configured
  Operator; workers poll and claim those rows after the runner commits.
  Notification delivery status is inspectable without an Alert table.
  Direct manual Verifier analysis dispatches none; manual binding invocation
  follows its configured Operator path. Scheduler tests use a controllable
  clock or explicit wakeup, not unbounded sleeps.
- **AC-014**: A real continuous Eval journey registers a Service and Eval
  Verifier with deterministic and LLM-judge tasks, authored pass_gate, and
  observations_ready binding; emits existing EvalRecordObservation through
  observe:eval over wyrd-client/wyrd-queue's canonical IPC path; and proves
  the new vala.eval.observations row receives a Scribe acknowledgement
  independent of the later best-effort Postgres verifier_runs insert.
  The created run MUST retain the committed row's exact `record_id` and
  server-managed `wyrd_event_time`, and its input read MUST demonstrate UTC-day
  partition pruning with those frozen values even when client `created_at`
  falls on a different day.
  Successful enqueue runs the existing Eval executor and persists
  vala.verification.results plus vala.eval.result_items joined by
  (`data_tenant_id`, `result_id`). A workflow that skips a task MUST persist
  its `TaskRunOutcome::Skipped` beside every `Ran` task outcome, while the
  common result's `details` serializes `EvalWorkflowSummary`. A forced
  post-commit enqueue failure MUST preserve the Bifrost observation, return
  successful ingest, emit a structured
  tracing error, and create no Eval run or Operator dispatch. A failing
  pass_gate creates one dispatch per configured Operator; a passing gate
  creates none; an absent gate persists `completed/inconclusive` and creates
  none. A sampled-out record persists one zero-count summary, zero item rows,
  and no dispatch. An all-skipped workflow persists its skipped rows and is
  inconclusive regardless of gate. The LLM judge uses a local mock provider.
- **AC-026**: Rust, Python, and TypeScript Eval journeys MUST emit a native
  serializable context through `observe.eval(...)` with optional session and
  media; they MUST prove the SDK constructs the existing
  `EvalRecordObservation`, generates record/time identity, supplies run/subject
  identity through Bifrost correlation, preserves
  valid explicit or active-span trace IDs, and inserts fixed rows using the
  cached `vala.eval.observations` schema without per-record describe. They
  MUST reject an invalid span/trace pair and malformed media before queue
  admission. The serialized user row MUST omit `run_id`; Bifrost correlation
  MUST supply it exactly once. Non-null trace/span IDs MUST round-trip through
  the existing JSON-row queue into nullable `FixedSizeBinary(16)`/
  `FixedSizeBinary(8)` columns and join by type with `vala.traces.spans`.
  Invalid hex or byte width MUST fail visibly, not silently drop trace
  correlation. No `eval_ref` field or second Eval observation type is permitted.
- **AC-027**: A continuous Eval journey with a media-referencing LLM judge
  MUST carry one named, tenant-authorized object-storage media reference from
  the client record through Bifrost, the existing `ScenarioScoring` and
  `JudgeTaskExecutor`, to the production `SkaldJudgeInvoker`. A local provider
  capture MUST prove the request contains native image/document content,
  not merely a URI or a JSON description; the resulting task outcome MUST
  reach the canonical Eval result and pass gate. Negative tests MUST prove a
  missing binding, inaccessible or cross-tenant URI, unsupported kind/MIME,
  and oversized media produce a visible execution/input error without a
  fabricated failed verdict or Operator dispatch. The later offline
  dataset path is not an acceptance obligation, but no second judging path
  may be introduced for it.
- **AC-015**: Integration tests cover duplicate observation record IDs,
  duplicate sealed Scribe batches, duplicate cron occurrences, worker lease
  expiry, retry exhaustion, and tenant isolation. A result-write test sends
  separate Arrow batches through the existing Bifrost gRPC/Scribe path,
  retries an unacknowledged sealed batch with the identical table, batch ID,
  and bytes, and proves Scribe deduplication. Another test forces one result
  table to acknowledge while another fails: partial rows may be visible,
  the run MUST NOT settle completed or dispatch Operators, and the error or
  retry state MUST be inspectable. A fresh write_batch call MUST NOT be
  misrepresented as an idempotent replay. Zero-detail completed results send
  no empty detail batch and require only the summary acknowledgement.
- **AC-016**: Continuous Eval covers sampled-in execution, sampled-out
  `completed/inconclusive` with a zero-count summary, ungated
  `completed/inconclusive`, all-skipped `completed/inconclusive`,
  AwaitingTrace requeue and deadline-to-`timed_out`, deterministic assertion,
  LLM judge, context capture, false assertion versus propagated executor/input
  error, pass-gate pass/fail, bounded retry-to-`errored`, no result rows for
  timed-out/errored execution, and restart recovery of successfully enqueued
  work. Offline scenario Data Card
  registration and dataset-backed Eval are not acceptance obligations for
  this initial change.
- **AC-017**: Each first-class Rust, Python, and TypeScript SDK MUST have
  gated real-SDK-to-real-server journeys using `WyrdTestServer` and
  repository-managed Postgres (not in-process engine fixtures) for
  a registered Service with a PSI Drift binding, an SPC Drift binding, and
  a continuous Eval binding. Each journey MUST cross registration, exact
  principal authentication, `observe:record`/`observe:drift` or
  `observe:eval`, canonical queue/IPC publication, Scribe acknowledgement,
  runtime execution, Bifrost result query, and status/result retrieval back
  through that SDK. A failed verdict MUST prove configured Operator delivery
  and its status. The three SDKs project the same durable contract; none
  implements its own server behavior. HTTP/MCP journeys cover scoped status,
  direct Verifier runs, and manual binding operations, including unauthorized
  and cross-tenant rejection. Each SDK journey MUST use the locked run API in
  `architecture/logic/run_api.md`, emit both a language-native typed object
  and a mapping/plain object, switch between Model and Agent scopes in one
  invocation, and prove the resulting observations have the correct exact
  subject Card UID and run correlation without caller-supplied Verifier refs.
  Unit/integration evidence MUST cover invalid input rejected before queue
  admission, fixed-schema row conversion, bounded queue admission, Arrow IPC
  publication, and graceful shutdown drain without per-observation flush.
  Python evidence MUST cover a mapping, a dataclass instance, and a Pydantic
  model whose `model_dump_json()` output is handed directly to Rust; all three
  MUST yield the same canonical Drift feature map and tall Bifrost rows.
  Invalid feature names, nested/null values, non-finite numbers, and
  non-representable integers MUST fail before queue admission.
  Rust, Python, and TypeScript evidence MUST also prove that the respective
  `drift(...)` / `eval(...)` call completes kind-specific projection before
  handing generic rows to the queue owned by the state-held Bifrost facade;
  shared `wyrd-client` must not dispatch on Verifier kind. The projected rows
  MUST omit `run_id` and `card_ref` user fields; both are supplied only as
  per-row correlation. Two concurrent scoped runs writing different tables
  MUST retain their own table and correlation without active-table switching.
- **AC-032**: A Python real-SDK-to-real-server journey MUST enter
  `state.run(card="...")` for a registered Service component, invoke
  framework-style code that creates spans without setting Wyrd attributes,
  and export those spans through the stock Python OpenTelemetry SDK and
  OTLP/HTTP exporter to Wyrd's authenticated `POST /v1/traces` endpoint. The
  same Run scope MUST write one caller-owned `vala.datasets.*` row and one Eval
  observation whose trace/span IDs come from the active span rather than
  explicit arguments. After context exit, explicit tracer flush, state
  shutdown, and server publication, persisted Bifrost queries MUST prove every
  span carries the exact shared `wyrd.run_id`, asserted `wyrd.card_ref`,
  authenticated publisher, and server-resolved Card UID; the custom row joins
  to those spans by `run_id`; and the Eval row joins to its exact span by
  `trace_id` and `span_id` while retaining the same run and Card identity. The
  proof MUST extend the existing Python scoped-observation journey rather than
  introduce a second Service fixture. An already-active recording span MUST
  receive the same attributes. Nested root and component scopes MUST share one
  run ID, select their own exact CardRefs, and restore the outer Card after
  exit. Async evidence MUST cover an `await`, concurrent tasks using the same
  immutable Run, and a task created inside the scope. Focused Python tests MUST
  prove that missing `opentelemetry-api`, an API-only/no-SDK provider,
  processor registration failure, span enrichment failure, and detach failure
  do not escape or block explicit observations; a user exception from the
  block MUST propagate unchanged. Unknown aliases MUST still fail before
  entry. Provider registration MUST be idempotent, and an explicitly installed
  private provider MUST receive the same attributes. Context exit MUST NOT be
  treated as a telemetry or Bifrost durability barrier. No test may infer
  automatic log or metric enrichment from this span contract.
- **AC-033**: Postgres integration evidence MUST prove the shared machine-
  activity/schedule path and the shared verifier-run/dispatch path assign
  immediate timestamps, relative deadlines, and due/expiry verdicts from
  `statement_timestamp()`. Lease reclaim, retry availability, inactivity, and
  no-backfill scheduling tests MUST control database coordination state rather
  than advance a process wall clock or backdate a fixture merely to make an
  unrelated predicate pass. Existing runtime journeys MUST continue to prove
  restart reclaim, stale-token fencing, scheduling, activity renewal, and
  dispatch creation. No skew workaround, clock abstraction, or permanent grep
  check is acceptable evidence.
- **AC-025**: Rust, Python, and TypeScript SDK journeys MUST start Bifrost
  through their specified pass-through options, fail startup when either
  fixed system table cannot be described, and emit Drift and Eval without
  per-observation schema IO. Each MUST write `observe.record` to two distinct
  already registered user tables from one state and run, including a scoped
  Card view, without changing an active table or supplying schemas. Evidence
  MUST show one first-use describe per dynamic table, cached reuse, concurrent
  first-use convergence, a pre-enqueue unknown/unauthorized-table refusal,
  server schema-fingerprint refusal after a stale writer, shared bounded
  client admission, and shutdown drain across all table producers. The
  generic record path MUST reject a reserved system table.
- **AC-028**: After registration, Card GET exposes stable binding IDs and current
  PSI/SPC fitting status through `card.status.verification`. The three new
  HTTP operations and their Rust, Python, TypeScript, and MCP projections
  support a manual bounded Drift binding run and a direct analysis-only run;
  both return `202 { run_id }`, can be polled to terminal status, and expose
  a Bifrost `result_id`. A failed binding run exposes every independent
  Operator delivery state through Run GET, while a direct run has none. Both
  manual forms persist and return the authenticated
  `requested_by_principal_id`; direct result/detail rows set
  `owner_card_uid` and `binding_id` to null rather than copying the caller,
  subject, or Verifier.
  Existing Bifrost query returns the authoritative summary and details;
  no separate result or dispatch endpoint is required. Tests cover
  idempotent manual-request retry, invalid windows, unavailable baseline,
  unauthorized and cross-tenant access, repeated Bifrost start, failed
  shutdown retry on the same handle, and refusal to restart a closed state.
- **AC-018**: Composite-registration journeys use real YAML to register a
  Service binding, Verifier, referenced Trigger, and multiple referenced or
  inline Operators. They prove path, CardRef, and inline forms; exact
  UID-bearing reference resolution; derived relationships; duplicate
  Operator/binding rejection; and fail-closed unresolved, wrong-kind,
  unauthorized, or cross-tenant refs. Registration alone does not activate
  a binding or start a run; PSI/SPC bindings remain unrunnable until ready.
- **AC-019**: A real client/server journey proves that initial
  API-key exchange or workload `jwt-bearer` authentication activates only the
  exact Card-bound Service or Agent Card version, request-driven stale-token
  re-exchange renews activity without resetting an active schedule cursor, and
  an idle client does not
  re-exchange solely on token expiry. Delegation, human refresh, Card-free
  automation, SYSTEM minting, cached-token requests, and ordinary observations
  do not activate or renew an owner. Inactivity, suspension, and deletion
  prevent scheduled work; later reauthentication starts at the next future
  occurrence without backfill. Two A/B versions remain independently
  active, replicas of one exact principal do not duplicate runs, component
  bindings inherit Service activity, and ordinary requests or observations
  do not write activity or require a heartbeat.
- **AC-020**: Supporting integration tests MUST exercise the real Postgres
  registration/auth/binding/run/dispatch seams, Oracle/Scribe result and
  observation seams, scheduler claim and lease expiry, baseline fitting,
  Eval post-commit enqueue, and Operator retry/fanout. Unit tests cover
  pure Drift/Eval validation, cron-window calculation, verdict mapping,
  sampling and pass-gate branches, and stable public errors. These lower
  tiers support, but do not replace, AC-012–AC-019 user journeys. The
  relevant `mise` lanes for Rust, Python, TypeScript, SQL-backed tests,
  codegen, format, lint, and boundaries MUST pass without credentials in the
  fast lane; provider-dependent Eval tests use a local mock.
- **AC-021**: Contract, loader, registry, schema, CLI, SDK, HTTP, and MCP
  evidence MUST show new `kind: Drift` and `kind: Eval` registration rejected,
  `kind: Verifier` with `implementation.kind: drift | eval` accepted, and
  existing observation record semantics preserved except removal of obsolete
  `drift_ref`/`eval_ref` and the named/kinded Eval media-item extension in
  REQ-131. No generated public schema
  or current architecture authority may advertise a second registrable Drift
  or Eval Card. Historical-card behavior follows the approved resolution of
  REQ-109: no migration or compatibility registration is required for
  unshipped Cards.
- **AC-022**: Generated built-in table catalogs and SQL ownership checks
  show no `vala.eval.runs`, `vala.eval.assertions`, or `vala.drift_alerts`
  definition or operational API. Real Eval and Drift journeys query only the
  replacement observation/result tables and inspect Operator dispatches
  instead of mutable Drift alert state. The retired `vala-core` alert-router
  module, crate, generated schemas, and dedicated schema check MUST have no
  remaining production or build-system references; shared security types
  with live consumers remain.
- **AC-029**: Local mock-provider journeys through a real server and Operator
  runner MUST prove a failed verdict creates one dispatch per supported
  Operator, that Slack targets the authored channel ID with its tenant-bound
  bot token and checks JSON `ok`, that PagerDuty sends the authored route with
  its tenant-bound Global Integration key and stable retry `dedup_key`, and
  that HTTP uses the approved bounded context and effective-URL SSRF defense.
  Wrong-tenant/missing connections, unsupported Workflow bindings, malformed
  templates, terminal provider errors, rate limits, timeouts, ambiguous
  retries, and independent fanout statuses MUST be visible without changing
  the Verifier result or creating an Alert. Before release, a gated live smoke
  check MUST send to a dedicated Slack test channel and PagerDuty test
  service through the same Operator runner; fast lanes require no credentials.
- **AC-031**: A tenant-admin journey creates Slack, PagerDuty, and HTTP
  Operator connections through the public typed surface, lists and reads only
  redacted metadata, rotates a secret without changing a Card, disables and
  re-enables a connection, and proves every first-class SDK plus CLI/MCP
  projects the same contract. Postgres inspection MUST show UUIDv7 identities,
  forced tenant RLS, ciphertext and wrapped-key material only, authenticated
  tenant/connection context, and key-version metadata; neither responses nor
  logs expose plaintext. Cross-tenant lookup, under-privileged mutation,
  read-secret attempts, wrong-provider references, disabled connections,
  HTTP origin/auth mismatch, credential-bearing forbidden headers, and
  origin-changing redirects MUST fail closed. A multi-replica Operator journey
  MUST observe a Postgres rotation on the next attempt without a Card revision
  or replica rollout.
- **AC-023**: A multi-server journey MUST run the Verifier worker on a server
  without local Scribe ownership and prove its Arrow result batches return
  through `wyrd_client::Bifrost` to Gate/Scribe, are acknowledged, and are
  queryable. It MUST assert input rows carry the client Service/Agent
  `principal_id` and subject `card_uid`, while result/detail rows carry the
  tenant-scoped SYSTEM writer `principal_id`, exact Verifier `card_uid`, and
  explicit subject/owner/binding IDs. A forged tenant or out-of-scope Verifier
  `card_ref` MUST be rejected; no global `SYSTEM_OWNER` token may write a
  customer-tenant result. Provisioning MUST create one stable UUIDv7 SYSTEM
  principal per tenant without a public credential. Tests MUST reject public
  create, list, get, update, suspend/delete, credential, refresh, workload,
  delegation, impersonation, and token-exchange operations for SYSTEM,
  non-SYSTEM writes to any result table, and SYSTEM writes to every other
  table. They MUST also prove the existing tenant issuer/JWT format and the
  single canonical Gate authorization audit. Two bindings for one subject MUST
  remain independently filterable through runs/results while sharing the one
  raw subject observation without Verifier/binding columns or per-binding
  copies.
- **AC-024**: Bifrost catalog/schema tests MUST assert the exact column names,
  order, Arrow types, and nullability in `architecture/logic/table_schema.md`,
  daily `wyrd_event_time` partitioning for all five verification tables, and the
  resolved Bloom-column union for `vala.verification.results` and both
  result-detail tables. A real tenant-scoped query MUST retrieve results by
  Verifier `card_uid`, subject
  Card UID, and binding ID, then retrieve matching details by
  (`data_tenant_id`, `result_id`) across at least two time partitions.
  Physical Parquet evidence MUST show the declared Bloom filters are written;
  an Oracle query plan or scan metric MUST demonstrate time-partition pruning
  and `result_id` row-group pruning where the predicate is selective. The
  result and its details MUST report the same `wyrd_event_time` even when
  their separate Scribe acknowledgements straddle a UTC day boundary. Schema
  evidence MUST also prove nullable `owner_card_uid` and nullable Drift
  `details`, with null owner only for direct runs and null details only for
  completed Drift executions that produced no report.
- **AC-030**: Authorization journeys MUST prove each REQ-145 permission at its
  public or Gate boundary, including allow and deny audit rows, subject Card
  scope refusal, reserved-table refusal, and no audit rows for internal claims,
  retries, Scribe commits, or worker mechanics. They MUST also prove
  `operators:read` versus `operators:write` separation for connection
  management and Gate's closed SYSTEM/result-table matrix. A multi-tenant runtime journey
  MUST saturate one tenant at four Verifier and four Operator executions while
  another tenant still progresses, and MUST show neither global pool exceeds
  16. A slow local Operator endpoint MUST prove the 30-second attempt timeout,
  three-attempt budget, 30-second/two-minute retry schedule, five-minute
  deadline, and terminal status without rerunning the Verifier. Shutdown tests
  MUST prove claims stop immediately, work drains for at most 30 seconds, and
  unfinished durable work is recoverable with the same identity after restart;
  an unexpected worker exit MUST restart and be visible through health,
  tracing, and the required runtime metrics.

## Open material decisions

None. Revision 35 was explicitly approved by the user on 2026-09-22.

## Material authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `changes/active/verified-change-contract/architecture/logic/run_api.md`
- `changes/active/verified-change-contract/architecture/logic/drift.md`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/doctrine/positioning-and-vocabulary.md`
- `architecture/references/domain/evaluation.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/spec-driven-development.md`
- `crates/wyrd-spec/src/vala/eval/task.rs`
- `crates/wyrd-spec/src/card/workflow.rs`
- `crates/skald/skald-workflow/src/run.rs`
- `crates/wyrd/wyrd-cli/tests/fixtures/loader/end_to_end/agent1/eval.yaml`

## Vendor protocol references

- [Slack `chat.postMessage` and channel permissions](https://docs.slack.dev/reference/methods/chat.postMessage/)
- [Slack Web API response contract](https://docs.slack.dev/apis/web-api/)
- [PagerDuty Global Integrations and Service Routes](https://support.pagerduty.com/main/docs/event-orchestration)

## Revision history

- **Revisions 1–10 (2026-09-03 through 2026-09-16):** Earlier draft
  iterations established the Verifier Card, Change Request model,
  Drift/Eval adaptation, verified_by authoring, and service-principal
  activity gate. Their superseded runtime diagrams and Alert/activation
  proposals are not authority for this revision.
- **Revision 11 locked-runtime draft (2026-09-16):** Replaced the old
  continuous-verification runtime and acceptance prose with the maintainer's
  locked control-flow diagram. Recorded vala.eval.observations as the
  EvalRecordObservation input table, best-effort post-commit Eval enqueue,
  canonical Bifrost results with Drift/Eval detail tables, separate
  acknowledged Arrow/gRPC result writes with accepted partial visibility,
  generic runner-to-Operator dispatch handoff, zero-to-many Operators, no
  Alert table, and no cron backfill. Removed offline scenario Data Card
  evaluation from this initial delivery. This remains draft until the
  open material contracts and complete spec revision receive explicit
  human approval.
- **Revision 12 scope and completeness draft (2026-09-16):** Made Drift/Eval
  Card retirement explicit, corrected the initial delivery scope, recorded
  the locked diagram's named Postgres tables and registration/auth/runtime
  transitions, required reuse of existing Wyrd/Vala seams and authority sync,
  and strengthened real Rust/Python/TypeScript tiered journeys. Recorded
  historical-Card retirement, SPC Metric baseline, and the existing Eval
  pull route as unresolved decisions rather than silently inventing
  compatibility or an unusable registration.
- **Revision 13 identity and cleanup draft (2026-09-16):** Retired the three
  unused Eval/Drift tables and old Eval pull path without migrations because
  none shipped. Fixed the Bifrost identity split: client inputs belong to the
  observed subject, server results to the exact Verifier, and the writer is
  independently identified by its principal. Required remote Gate/Scribe
  result writes through wyrd-client under a tenant-scoped internal SYSTEM
  authority, with signed Verifier scope and reserved-table permission.
- **Revision 14 result-layout draft (2026-09-16):** Recorded the agreed
  writer-versus-Card identity matrix and physical `data_tenant_id` join key.
  Fixed hourly Bifrost result partitioning on managed `wyrd_event_time`,
  result-time consistency across separate summary/detail writes, and the
  minimal additional Bloom columns for result lookup, subject/binding
  filtering, and detail joins.
- **Revision 15 daily-layout draft (2026-09-16):** Changed all five
  verification Bifrost tables to daily `wyrd_event_time` partitions, including
  the existing Drift observation table. Hourly schedules remain hourly
  logical windows. Bloom choices and the result/detail join are unchanged.
- **Revision 16 initial-core scope draft (2026-09-16):** Limited this change
  to the common Verifier primitives and production Drift/Eval. Removed
  Change Request and deferred implementation requirements, acceptance tests,
  and open decisions from this change; their field-level specs and executors
  belong to follow-up changes and no longer block this spec's approval.
- **Revision 17 run-API draft (2026-09-16):** Locked the agreed Python, Rust,
  and TypeScript run/observation ergonomics, shared client conversion and
  existing JSON-row-to-Arrow queue seam, no per-observation flush, and
  subject-scoped observation identity. Removed stale `drift_ref`/`eval_ref`
  routing and raw-input per-binding duplication. Left physical observation
  projections open for query-driven design; the overall spec remains draft.
- **Revision 18 run-API correction draft (2026-09-16):** Incorrectly
  generalized Python/TypeScript `observe` member access to Rust; superseded
  by revision 19.
- **Revision 19 Rust-accessor correction draft (2026-09-16):** Restored the
  idiomatic Rust `scope.observe().drift(...)` / `scope.observe().eval(...)`
  accessor after the maintainer clarified that the member-access requirement
  applies to Python and TypeScript, not Rust.
- **Revision 20 Python Drift projection draft (2026-09-17):** Fixed direct
  Pydantic `model_dump_json()` handoff, strict mapping/dataclass JSON input,
  deserialization into the existing typed Drift feature map, and the agreed
  one-row-per-feature two-value-column queue/Arrow projection. Removed Drift
  input projection from the open schema decision; Eval projection remains
  open. The overall change remains draft pending approval and its other
  open material decisions.
- **Revision 21 observation ownership draft (2026-09-17):** Assigned
  Drift/Eval normalization, canonical-record construction, and fixed-table
  projection to their respective `observe.drift(...)` / `observe.eval(...)`
  calls, before queue insertion. It placed queue ownership directly on
  `WyrdState`; revision 22 supersedes that with state-owned Bifrost facade
  ownership. Shared `wyrd-client`/`wyrd-queue` remain Verifier-kind-agnostic.
  Removed the separate Bifrost argument from the run API; retained the
  agreed high-level Rust and TypeScript inputs.
- **Revision 22 state-owned Bifrost draft (2026-09-17):** Recorded explicit
  `start_bifrost`/`startBifrost` constructor pass-through and SDK lifecycle,
  eager fixed-table describe within Bifrost, lazy cached describe for
  dynamic `observe.record(table, value)`, explicit-table routing without
  shared active-table mutation, one schema/producer per table, cross-table
  shutdown drain, and the client-to-Gate-to-Scribe data path. Kept the Eval
  physical input encoding open for an explicit queue-compatible schema
  decision. The overall change remains draft.
- **Revision 23 Eval wrapper and media-judge draft (2026-09-17):** Kept the
  revision-22 state-owned Bifrost lifecycle and existing queue/IPC path.
  Defined one ergonomic `observe.eval` wrapper over the existing canonical
  record, explicit versus active-span trace context, named Eval media refs,
  one shared scoring/judging engine for continuous and later offline Eval,
  and server-authorized URI-to-native-media binding in the existing Skald
  judge adapter. Added real-client and provider-capture acceptance evidence.
  The Eval physical Arrow table schema remains open; this revision is draft.
- **Revision 24 observation insert reuse draft (2026-09-17):** Fixed the
  state-owned Bifrost/WriterPool reuse path for all three observe calls:
  cached described schemas, explicit-table row insertion without active-table
  switching, and `card_ref`/`run_id` only in per-row correlation. Removed
  duplicate run IDs from Drift/Eval logical records and required generic
  fixed-binary trace/span conversion in the existing JSON-row queue. This
  revision remains draft because other material decisions are still open.
- **Revision 25 Drift method/signal validation draft (2026-09-17):** Fixed
  the three valid Drift method/signal pairs and required registration to reject
  the legacy `Spc` + `Metric` combination. Removed its baseline question from
  open decisions; the remaining material decisions keep this revision draft.
- **Revision 26 public Verification API draft (2026-09-17):** Reused the
  existing Card read and Bifrost query surfaces, fixed three new Verification
  HTTP operations and their SDK/MCP projections, made Operator delivery
  visible through run status, and fixed one-start/same-handle-shutdown
  lifecycle semantics. Removed the public-contract item from open decisions;
  the remaining material decisions keep this revision draft.
- **Revision 27 Operator invocation draft (2026-09-18):** Fixed the bounded
  failure context, tenant-bound credential connections, independent
  dispatch-to-Operator flow, finite timeout/retry classification, and
  destination idempotency limits. Replaced per-channel Slack webhooks with
  one bot-token connection per workspace and authored channel IDs; selected
  PagerDuty Events API v2 Global Integration routing with one tenant
  connection and authored route. Kept Workflow actions parseable but rejected
  them in active bindings pending the separate server invocation change, and
  retired the unused `vala-core` alert-router crate and its dedicated
  generated/check artifacts. Numeric operational bounds and the remaining
  persistent schemas are still open, so this revision remains draft.
- **Revision 28 table schemas draft (2026-09-18):** Fixed the exact five
  verification Arrow schemas in `architecture/logic/table_schema.md` as the
  physical table authority. The common result keeps only generic columns and
  serializes `DriftReport` or `EvalWorkflowSummary` in `details`; Drift features
  and every Eval task outcome, including skipped tasks, remain queryable in
  their detail tables. Existing Bifrost retention applies without a new
  verification policy. Runtime authorization and operational bounds remain
  open, so this revision remains draft.
- **Revision 29 Eval input lookup draft (2026-09-19):** Required every
  postcommit Eval `verifier_runs` row to freeze the committed observation's
  `record_id` and server-managed `wyrd_event_time`. The runner uses that time
  to constrain its Bifrost input read to the correct UTC partition rather than
  trusting client `created_at`, using a moving lookback, or scanning history.
  This adds no field to `vala.eval.observations`; runtime authorization and
  operational bounds remain open, so this revision remains draft.
- **Revision 30 runtime authorization and operations draft (2026-09-19):**
  Reused the existing Card, Eval, Operator, and Bifrost permissions with
  canonical audit only at boundaries that evaluate them. Fixed global and
  per-tenant Verifier and Operator capacity, Operator timeout/attempt/deadline
  limits, graceful shutdown, supervision, health, tracing, and metrics. No
  material decisions remain; this revision was explicitly approved on
  2026-09-19.
- **Revision 31 result, identity, and connection correction (2026-09-19):**
  Made Eval terminal behavior explicit: sampled-out, ungated, and
  zero-attestation executions complete inconclusive; execution/input failures
  remain errored or timed out rather than becoming failed assertions. Allowed
  completed pre-scoring Drift results to omit `DriftReport` details and feature
  rows. Replaced deterministic SYSTEM identity with one persisted UUIDv7
  internal principal per tenant and fixed its reserved-table admission matrix.
  Replaced process-local Operator secret references with tenant-isolated,
  envelope-encrypted Postgres connections and typed redacted management
  operations. Separated manual requester identity from nullable binding-owner
  identity on direct runs. These corrections were explicitly approved on
  2026-09-19.
- **Revision 32 planning-blocker resolution (2026-09-19):** Replaced Drift and
  Eval with Verifier in the authoritative 15-kind catalog and binding doctrine.
  Preserved the existing SPC profile and scorer instead of introducing the
  conflicting fixed-rule dual-chart design. Fixed UUIDv7 BindingId persistence,
  `wyrd-sql` ownership of RLS-protected verification control tables, closed
  provider-specific Operator-connection CRUD shapes, exact HTTP credential
  authority matching, and external tenant/version KEK resolution and rotation.
  Synchronized the linked runtime diagram's ingest-time windows, no-backfill,
  post-ack Eval enqueue, and no-Alert flow. This revision was explicitly
  approved through the user's delegated blocker-resolution authority on
  2026-09-19.
- **Revision 33 identity integration (2026-09-21):** Integrated Verification
  with the current tenant-principal and five-minute permission-snapshot model.
  Added one internal-only tenant `system` principal kind for canonical result
  publication while reusing the existing identity store, token issuer, JWT
  format, Gate, and audit path. Fixed binding activity to successful API-key or
  workload-`jwt-bearer` exchange by the exact Card-bound Service or Agent and
  explicitly excluded delegation, human refresh, Card-free automation, SYSTEM
  minting, cached bearer use, and idle expiry. This revision was explicitly
  approved by the user on 2026-09-21.
- **Revision 34 Run context and Python span correlation (2026-09-22):** Added
  language-idiomatic initial Card selection when opening a Run and retained
  immutable `for_card` views for multi-component invocations. Made Python Run
  a synchronous context manager that injects `wyrd.card_ref` and
  `wyrd.run_id` into the active span and framework-created child spans through
  optional execution-local OpenTelemetry context plus an idempotent span
  processor. Fixed nested and asyncio propagation, private-provider escape
  hatch, server-owned identity boundaries, and fail-open behavior when Python
  OpenTelemetry is absent or fails. Required the existing Python Service journey
  to export through authenticated `/v1/traces` and prove persisted joins from
  traces to custom rows by Run ID and Eval rows by trace/span identity. This
  revision was explicitly approved by the user on 2026-09-22.
- **Revision 35 PostgreSQL coordination clock (2026-09-22):** Made the
  evaluator own every verification timestamp: PostgreSQL now owns activity,
  scheduling, claims, leases, retries, worker deadlines, and their predicates;
  Rust retains monotonic in-process timing and producer-owned event facts.
  Prohibited process-clock/database-clock mixing, clock abstractions, skew
  tolerances, and checker-based enforcement, and required behavioral Postgres
  regression evidence for each corrected shared write path. This revision was
  explicitly approved by the user on 2026-09-22.
