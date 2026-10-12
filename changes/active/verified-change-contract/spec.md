---
id: SPEC-verified-change-contract
revision: 70
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

TASK-006 also includes the server startup and distributed-peer remediation
needed to run these journeys from the official image. The operator can start
one Wyrd application container with external Postgres and durable storage,
provision a first tenant through one setup command, and use a client. The same
image supports a peer-enabled deployment whose Oracle read tier grows from one
to two replicas without a manual peer list.

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
- TASK-006 server startup, first-use setup, external Postgres migration and
  serving-role simplification, public routing, local storage URLs, and peer
  discovery/security across one-process and multi-replica deployments.
- Removal of the unearned external authorization-check endpoint and policy-hook
  extension from the server, token exchange, public contracts, and documentation.

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
- Refusing registration of Card kinds that are not yet implemented
  (`Experiment`, `Workflow`, `Policy`, `Mcp`, `Audit`, `Artifact`,
  `Source`). Deferred; REQ-193 only keeps them out of the SDKs.
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
separate result-table acknowledgements can leave a run's detail rows visible
before its summary; and an ambiguous external Operator send may be delivered
more than once.

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
  server-minted UUIDv7, and the fixed name `verification-results-writer`. It
  is an attribution identity only and never appears in a token. It has two
  uses: stamping Verification Result rows written by the server-internal
  result writer, and Verifier input reads under a tokenless, table-scoped
  `bifrost_query:read` authority (REQ-086). It has no public credential, Card,
  role grant, refresh, workload, delegation, or principal-management path.
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
- **REQ-110**: The Drift payload MUST retain `method`, `signal`, `condition`,
  `profile`, and the nested PSI/Custom configuration and validation, with the
  agreed removals of `DriftMethod::External`,
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
  already own their thresholds or control limits. Revision 37 replaces the
  previous SPC profile and scoring decisions with REQ-153–REQ-155. No
  client-side binning or subgroup aggregation is added.
- **REQ-153**: PSI MUST compare baseline and target over the same exhaustive,
  frozen bins. Numeric bins retain their fitted edges and cover values outside
  the baseline range. A categorical fit MUST include one reserved `other` bin
  with zero baseline count; every target category absent from the fitted
  labels contributes to that bin. Baseline and target proportions MUST each
  sum to one before the existing zero-bin smoothing and PSI formula. The
  `other` bin participates in the score, threshold bin count, and reported
  sample/bin evidence. Existing numeric bin strategies, the minimum target
  sample of 100, and authored PSI threshold choices remain; PSI and its
  threshold strategies MUST NOT be presented as a significance test or proof
  of model degradation.
- **REQ-154**: SPC MUST be a two-sided, three-sigma Shewhart X-bar/S chart over
  consecutive rational subgroups of an explicitly authored, fixed
  `SpcProfile.sample_size >= 2`. The public SPC profile contains only
  `sample_size`; registration rejects zero, the old `weco_rule` and
  `alert_threshold` fields, and unknown fields. Authors are responsible for
  supplying baseline rows in process order and a size whose consecutive rows
  form meaningful subgroups; Wyrd does not infer process context or subgroup
  boundaries. Runtime rows are ordered by `created_at`, then `record_id`.
  Baseline fitting requires at least 20 complete subgroups from an
  author-identified stable process; extra trailing rows make fitting fail
  visibly rather than joining a smaller subgroup. A target window requires
  at least one complete subgroup; a trailing partial subgroup makes that run
  inconclusive. Neither path may shift or silently discard rows to make groups
  fit.
- **REQ-155**: For subgroup size `n`, fit the grand mean `x_bar_bar`, mean
  sample standard deviation `s_bar` (each subgroup uses `n - 1`), and NIST's
  `c4(n)`. Freeze X-bar limits at `x_bar_bar ± 3*s_bar/(c4(n)*sqrt(n))` and S
  limits at `max(0, s_bar*(1 - 3*sqrt(1 - c4(n)^2)/c4(n)))` and
  `s_bar*(1 + 3*sqrt(1 - c4(n)^2)/c4(n))`; the chart centers are `x_bar_bar`
  and `s_bar`. A subgroup mean or sample standard deviation strictly outside
  its respective limits signals drift; equality does not. Either chart's
  signal fails the feature and thus the run. The existing scalar feature
  `score` becomes the total count of signaled subgroups across both charts
  and its threshold is zero. Persist typed SPC report evidence containing
  subgroup size/count and, for each X-bar and S chart, center, lower/upper
  limits, and signal count. PSI and Custom reports omit this SPC evidence.
  The shared `vala-drift` scorer owns both direct and server scoring; the
  server only supplies ordered complete subgroup aggregates.
- **REQ-156**: Baseline fitting MUST reject any null or non-finite value in a
  required PSI or SPC feature. The server selects target observations for the
  subject and window by configured series presence, even when a selected
  series has a null value; unrelated observations do not enter the comparison.
  Direct PSI/SPC scoring receives an already selected target batch: every row
  is one relevant observation, and its caller excludes unrelated observations
  before scoring. The direct scorer MUST NOT infer row membership from whether
  configured values are non-null. In either path, a missing configured feature
  in a selected observation, or a null or non-finite value, MUST make the
  affected Drift run inconclusive, with no scored details or feature rows;
  values are not dropped or imputed. A target with no observations or
  insufficient complete samples is likewise inconclusive. Direct and server
  scoring MUST agree after the same observation selection. This is Wyrd's
  explicit data-completeness policy, not a PSI or SPC formula. Client
  `observe:drift` continues to reject explicit null and non-finite values;
  server checks also cover omitted features and historical or externally
  ingested rows. Custom retains its existing null, non-finite, and
  empty-window inconclusive behavior.
- **REQ-157**: Corrected PSI/SPC semantics begin with newly registered,
  immutable Verifier Card versions and newly fitted baselines. The service
  MUST NOT silently rescore existing Verifier versions or stored results
  under revision 37 math. Previously persisted reports remain readable;
  legacy versions without a revision-37 fitted profile cannot start new
  scoring and fail visibly until replaced by a new version. No dual legacy
  scorer or automatic migration is required.
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
| `drift` | `DriftSpec` with REQ-110 and REQ-153–REQ-157 decisions | Exact subject and fixed window of existing `DriftRecordObservation` rows; PSI/SPC also load exact `FittedBaseline` | `DriftReport` with SPC chart evidence, mapped to one common verdict plus feature details | Production in this change |
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
  recording span only when that span does not already carry `wyrd.card_ref`,
  and ensure a Wyrd span processor copies the innermost scope's pair from the
  parent context to every span started inside the scope.
  The Wyrd scope stack lives inside the OpenTelemetry context value itself: a
  tuple of `(card_ref, run_id)` pairs, innermost last, under one private
  context key created at module import when OpenTelemetry is present. Entry
  pushes and exit pops, each by a single context attach; no detach token
  exists and detach is never called. Exit pops only when the top of the stack
  equals that view's own `(card_ref, run_id)`; a mismatched or failed exit
  changes nothing. Exiting MUST restore the prior Wyrd correlation in the
  current context, including nested Card scopes, and MUST NOT suppress or mask
  a user exception. Normal context propagation MUST work across `await` and
  asyncio task creation without storing per-scope state on the immutable Run.
  A framework using the global provider MUST need no setup beyond
  `with state.run(...)`; the Python SDK MUST expose an idempotent
  `wyrd.otel.install_run_correlation(provider)` escape hatch for a
  framework-owned private provider. Registration carries exactly three
  guarantees. (1) No processor pile-up: Run entry automatically registers the
  Wyrd processor on the global tracer provider (and
  `install_run_correlation(provider)` on an explicit private provider) and
  marks that provider object with a private attribute so later entries skip
  it. Registration is best effort; a duplicate processor (for example from a
  concurrent first entry) is harmless because the processor is stateless and
  enrichment is idempotent. (2) Never break the app: every optional-telemetry
  failure, including absent OpenTelemetry packages, an unsupported or absent
  provider, a provider without `add_span_processor`, an unmarkable provider,
  a registration error, no active recording span, invalid runtime context,
  processor failure, and context-update failure, is swallowed as no
  enrichment: it MUST NOT fail Run construction or entry/exit, application
  execution, Card errors, user exceptions, or explicit Wyrd observation
  emission. (3) Correct stamping: the scope stack lives in the OpenTelemetry
  context value as specified above; spans started inside the block receive the
  innermost `(card_ref, run_id)`; nested, async, and same-Run concurrent tasks
  stamp correctly; exit pops only that view's pair; and entry does not
  overwrite an existing `wyrd.card_ref` on the already-active span. Unknown Card aliases,
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
  or the Eval run-request flusher MUST restrict new work to runtime-active exact
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
  An Eval observation creates runs only for `observations_ready` bindings
  whose exact owner's principal is the principal that wrote the observation.
  That writer has just authenticated, so the Eval run-request flusher does no
  separate activity check. Scheduled and Trigger runs keep the activity gate
  above.

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
- **REQ-077**: Eval runs MUST be created through a batched run-request
  outbox, the same shape as the audit outbox. Scribe acknowledges an Eval
  observation batch on its own durable boundary, and the client never waits
  for run creation. After that acknowledgement the server places one run
  request per committed record in its in-process outbox. A background flusher
  writes queued requests to Postgres in one multi-row insert per tenant,
  creating one `verifier_runs` row per matching active `observations_ready`
  binding. Rows are keyed by tenant, binding, and record identity, so a
  repeated request inserts nothing new. Each row MUST freeze `input_record_id`
  and `input_event_time`, where `input_event_time` is the exact server-managed
  `wyrd_event_time` assigned to the committed observation, not the
  client-authored `created_at`.
  - The outbox has no count limit and never drops a request because Postgres
    is slow or unavailable. A failed flush keeps its batch and retries with
    backoff.
  - Graceful shutdown flushes the outbox before the process exits.
  - A hard process kill loses only the requests not yet flushed. The user
    accepted that loss, as for audit. Every loss the process can observe is
    counted and logged.
  - The fixed 256-entry activation backlog and its drop-on-full behavior are
    removed.
  - Scribe does not write `verifier_runs`, and no crate's transaction writes
    another crate's tables.
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
  transaction. (revised in revision 67) A Service or Agent principal's first
  projection also grants it the built-in `wyrd_default` role
  (`bifrost_table:read`, `bifrost_record:write`, `evals:run`; REQ-213) in that
  transaction, so a key issued for it through `POST /auth/issue-key` can
  emit and verify its evidence without a manual grant. Its Card scope still bounds which Cards it may emit for. Re-applying
  the Card never grants the role again, so an administrator's revocation
  stands; existing tenants receive the role row by migration. Vala consumers use the `vala-sql` re-export rather than import
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
- **REQ-086**: Verifier results MUST be written as a server-internal write,
  the same way gateway capture is (REQ-178). The process's one capture writer
  submits the result batches to Scribe in-process when Scribe runs in the same
  pod and otherwise over the mutually authenticated peer plane to a live,
  ready Scribe; the choice follows pod topology (`WYRD_TARGET`). Result writes
  MUST NOT go through `wyrd_client::Bifrost`, the public ingest listener, or
  Gate, and no token is minted for them. The peer ingest RPC is widened to
  carry exactly the three result tables in addition to the two capture tables,
  each submission naming the tenant, table, run, and frozen Verifier Card UID
  explicitly; it refuses every other table and every reserved system tenant.
  Result writes share the capture writer's transport but not its delivery
  policy: a result batch is never dropped. Retryable refusals are retried for
  as long as the run's lease remains, and an unwritten result stays stored
  for the next claimant (REQ-183).
  Gate MUST refuse every public write to `vala.verification.results`,
  `vala.drift.result_features`, and `vala.eval.result_items`, from any
  principal including wildcard administrators.

  Tenant provisioning and upgrade migration MUST idempotently create exactly
  one internal `system` principal row named `verification-results-writer` for
  each tenant in the existing tenant machine principal store. Its
  server-minted `PrincipalId` MUST be UUIDv7 and remain stable after
  provisioning. `system` is the sixth `PrincipalKindTag` wire value and a
  tenant-plane `PrincipalKind`, not a second identity hierarchy. It is an
  attribution identity only: result rows carry it as `principal_id`, and
  in-process reads (below) run as it, but it never appears in a token. It is
  not a Card, API key, refresh token, role grant, workload binding, or
  user-manageable principal. Public create, list, get, update,
  suspend/delete, credential, refresh, workload, delegation, and
  token-exchange operations MUST reject or omit it as appropriate. The token
  issuer MUST refuse to mint any token for it, and token verification MUST
  refuse any `kind=system` claim set. The SYSTEM result-write token, the
  SYSTEM Drift read token, and their issuance and verification paths are
  removed.

  Scribe stamps the submitted tenant as `data_tenant_id`, the tenant's SYSTEM
  principal as `principal_id`, and the submitted Verifier Card UID as managed
  `card_uid`; it MUST refuse a row whose `card_ref` names a different Card. The
  submitted values come from the run's frozen Postgres row, never from a Verifier
  or an Arrow payload. A result write evaluates no permission and writes no
  audit decision, like every other worker mechanic under REQ-145.

  Every Verifier input read runs in-process as that same tenant SYSTEM
  principal under a tokenless read authority. The server resolves the
  principal's stable ID from tenant-owned state and builds an authority whose
  only permission is `bifrost_query:read` scoped by registered table UID to
  exactly the tables the run reads: `vala.drift.observations` for Drift, and
  `vala.eval.observations` plus `vala.traces.spans` for Eval. It carries no
  roles, credential, delegation, Card, or write scope, and every other table
  is refused. Drift resolves the existing table UID in the run's tenant and
  MUST NOT create or register a table; when the tenant has no such table the
  run scores an empty window (Custom completes `inconclusive` with no report).
  The read goes through the ordinary server query service to a local or
  peer-forwarded Oracle, carrying the authority over the peer plane rather
  than as a token. The server MUST NOT assume an Oracle is active in its own
  process or add an Oracle endpoint, client query route, or plan
  serialization. Oracle's table authorization is the enforcement point; it
  records the canonical read decision for each read and the canonical audited
  denial for each refusal. A missing or non-UUIDv7 SYSTEM principal, a
  mismatched tenant, or insufficient table scope MUST fail closed before any
  row is returned. The server-built fixed Drift SQL supplies subject, feature,
  fitted-edge, label, and window filters from the frozen run and fitted
  baseline as escaped typed literals; a Verifier contributes no SQL text.
  Registration authorization is not standing query authorization. This adds
  no identity store, principal kind, user, public permission, token format, or
  public surface.

  Every non-empty required detail batch is written before the canonical
  summary batch, and each is separately acknowledged. A result with zero
  details—such as sampled-out Eval or pre-scoring inconclusive Drift—writes no
  empty detail batch and requires only the summary acknowledgement. Only
  after every required batch of the run's staged result (REQ-183) is
  acknowledged may the runner settle verifier_runs as completed and create
  Operator dispatches. Detail rows may be visible before their summary, but a
  run never has more than one result: every write of its result reuses the
  same staged bytes and batch IDs.
- **REQ-087**: Scribe acknowledgement means its existing WAL, batch fence,
  and active rows accepted that sealed batch; it does not mean Iceberg
  publication. Every write of a run's result, by any replica and after any
  crash, MUST submit the staged Arrow bytes under the staged batch IDs
  (REQ-183), so Scribe's batch fence absorbs every repeat and each result
  table holds exactly one copy of the run's rows. A result is never rebuilt
  for a new write.
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
  key source defined below. Rotation replaces the encrypted secret on the same
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

  The Wyrd server owns Operator key selection and reads the KEK from its
  configured deployment key source. Multi-tenant production MUST use Vault KV
  v2 over HTTPS, with a 32-byte tenant KEK at
  `<prefix>/<data_tenant_id>/<key_version>`. Server configuration MUST supply
  one active positive key version and, for Vault, the key prefix. Multi-tenant
  production MUST fail startup if Vault or any active tenant's active key is
  unavailable. Environment-sourced KEKs are development-only; a restrictive
  file-mounted KEK MAY be used by an explicitly single-tenant deployment.
  Operator credentials are supplied through write-only connection operations,
  not through deployment key configuration. Postgres stores only the key
  version and wrapped DEK. KEK rotation publishes a new key version before
  making it active; new writes use it, existing rows are rewrapped in bounded
  tenant-scoped work, and an old version is retained until no row references
  it. Rewrap exposes only the DEK inside the process and does not decrypt the
  credential.
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
  `POST /v1/verification/runs`,
  `GET /v1/verification/runs/{run_id}`, and the synchronous
  `POST /v1/verification/execute` defined by REQ-167. Binding GET MUST return exact owner,
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
- **REQ-137** (revised in revision 64): Binding status, manual run start,
  and run status are operator control-plane operations. They stay on HTTP and
  MCP and MUST NOT be exposed by the Rust, Python, or TypeScript SDKs (see
  REQ-189). Baseline status uses the existing Cards read capability, and
  analytical results use the existing Bifrost query API with `result_id` and
  the registered result/detail tables. MCP MUST expose
  `cards.get`, `verification.get_binding`, `verification.start_run`, and
  `verification.get_run` as typed projections of those same server
  operations. `verification.start_run` requires an explicit write scope;
  result data uses the existing `bifrost.query` tool. No separate MCP
  result or Operator-status tool is introduced.
- **REQ-101**: The initial change MUST prove the entire registered
  Service/Agent-to-Operator journey for PSI, SPC, Custom Drift, deterministic
  Eval, and LLM-judge Eval through real SDK, server, Postgres control state,
  canonical client queue/IPC observation ingest, Bifrost query, server-internal
  result writes, scheduling or transactional Eval enqueue,
  result persistence, and status. Cron, manual activation, worker lease,
  retry, the Eval run-request outbox, single-copy result writes, Operator fanout,
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
  run-request outbox, media binding through the existing judge path,
  result-table writers, and public status/manual surfaces are added. No new
  downstream projector, observation envelope, Eval engine, generic
  broker, or Alert persistence path is permitted.
- **REQ-115**: `wyrd-server` MUST supervise one generic
  `VerificationRuntime` containing Scheduler, Verifier runner, and Operator
  worker capabilities. Operator concurrency, queue reads, engine execution time,
  external delivery, and shutdown drain MUST remain bounded. Verifier runs and
  baseline fitting MUST NOT use global or per-tenant execution-count permits.
  A process restart
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
- **REQ-146**: The initial VerificationRuntime MUST use one scheduler task.
  Verifier runs and baseline fitting have no global or per-tenant execution-count
  permits; their durable PostgreSQL claims, leases, deadlines and shutdown
  behavior remain authoritative across replicas. External Operator execution
  retains a ceiling of 16 per process and 4 per tenant within that process;
  the Operator worker acquires both permits before claiming a dispatch.
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
- **REQ-181**: Every claim round MUST consider every tenant with a claimable
  run, with no limit on the number of tenants, and claim at most one run per
  tenant. A round claims each tenant in order of its oldest claimable run.
  Execution has no count limit and nothing is reserved in advance (revision
  50). The pod's real limits come from the shared resources that already
  admit work: the Postgres pool, Oracle query admission and memory, and the
  Bifrost memory budget. When one of them refuses a run for lack of capacity,
  the run returns to the queue without consuming an attempt, and the process
  claims nothing new until one of its running runs finishes.
- **REQ-182**: Each process MUST cache parsed Verifier Cards by tenant and
  Verifier Card UID. A Card's spec never changes under its UID, so a cached
  entry needs no content invalidation. The claim transaction returns the
  claimed run's Verifier Card status from `wyrd.cards`; a deleted Verifier
  terminates the run `errored` exactly as an unloadable Card does today. On a
  miss the claim transaction also returns the Card spec, so loading a Verifier
  never opens its own connection. The cache is bounded by total bytes with
  least-recently-used eviction, fixed at 64 MiB per process, because every
  Wyrd-owned buffer is bounded (`architecture/bifrost-design.md`, Resource and
  failure invariants). It has no configuration and is never shared across
  tenants.
- **REQ-183**: A run's result MUST be decided once. When execution finishes,
  the runner stores the complete result in one lease-fenced Postgres
  transaction before any of it is written to Bifrost: the `result_id`, the
  result event time, one batch ID per result table, and each table's Arrow IPC
  bytes. A transaction whose lease token no longer matches stores nothing, and
  its work is discarded. A run with a stored result is never executed again:
  any later claimant, whether a retry, a reclaim after lease expiry, or a
  restart, writes that stored result instead. The settle transaction that
  completes the run deletes the stored result. The staging table is owned by
  `wyrd-sql`, carries `data_tenant_id`, enables and forces RLS, and is
  reachable only through `TenantConn`. A failed write of a stored result
  retries the write, not the execution.
- **REQ-184**: While a run executes or writes its result, the runner MUST
  renew its lease on the PostgreSQL clock once a third of the lease duration
  has passed. It renews all of one tenant's in-flight leases in one statement.
  A renewal that finds a run's token gone cancels that run's work at once. A
  run that has not stored its result stores nothing; one that has stored it
  leaves it for the new claimant. Renewal evaluates no permission and writes
  no audit.
- **REQ-185**: A run MUST hold a Postgres connection only inside these short
  transactions, never across engine execution, Oracle reads, LLM or media
  calls, or Bifrost writes: the claim (which also returns the Verifier), the
  result store, the settle, and the shared per-tenant lease renewal. No phase
  mints a token. A connection that cannot be acquired never consumes an
  attempt. The result store and the settle retry with backoff for as long as
  the run's lease remains, and a settle that still cannot run leaves the
  stored result for the next claimant.
- **REQ-186**: A scheduled occurrence whose window ends at `T` MUST NOT be
  claimed before PostgreSQL's `statement_timestamp()` reaches `T + 30
  seconds`. The window stays `[start, T)`. The 30-second wait lets
  observations stamped before `T` finish ingest before the window is read.
  It is a fixed value, not configuration.
- **REQ-187**: Every observation row a first-class SDK emits MUST carry a
  `wyrd_event_time` set once in `wyrd-client`, from the client clock, at the
  moment the emit is called. A caller-supplied `wyrd_event_time` is kept, never
  overwritten. Batching, linger, retry, and flush delay MUST NOT change it.
  Scribe still stamps receipt time only for rows from other producers that omit
  the column, and still refuses values outside its acceptance window. An
  observation that reaches the server more than 30 seconds after its scheduled
  window ended is stored, but that window has already been read and does not
  count it.
- **REQ-188**: A Run view MUST expose real-time verification as
  `observe.verify(verifier, input)` in all three SDKs (Rust
  `run.observe().verify(...)`, Python and TypeScript `run.observe.verify(...)`).
  - `verifier` is the `metadata.name` of a Verifier Card bound in
    `verified_by` to the view's subject Card in the hydrated graph. The client
    resolves it to the exact Verifier locally. A name that is not bound to the
    view's subject fails locally with `WYRD_SDK_404_UNKNOWN_VERIFIER` before
    any network call. The hydrated graph MUST carry what this resolution needs.
  - The subject is always the view's subject Card. The caller never passes a
    Card UID.
  - An Eval Verifier takes one context in the forms `observe.eval` accepts,
    with the same optional media. A Drift Verifier takes a non-empty sequence
    of feature rows in the forms `observe.drift` accepts; the client turns the
    rows into REQ-167 columns. Input of the wrong shape for the Verifier's kind
    fails locally with `WYRD_SDK_400_INVALID_OBSERVATION`. A Drift Verifier
    whose baseline is not yet ready is refused with
    `WYRD_VERIFICATION_409_BASELINE_NOT_READY`; baselines are fitted
    asynchronously, so a caller that needs one waits on the Card's status.
  - It judges only. It records no observation, creates no run, dispatches no
    Operator, and writes nothing to Bifrost. It calls REQ-167 and keeps
    REQ-168's authorization, audit, bounds, errors, and no-retry rule. It does
    not require Bifrost to be started.
  - It returns a typed `Judgment`: `verdict` (`passed | failed |
    inconclusive`), `passed` (true only for `passed`), `summary`, `counts`, the
    exact `verifier` and `subject` references, `execution_id`, and a typed
    Drift or Eval report. A `failed` verdict is a normal return, never an
    error. Python is synchronous; Rust and TypeScript await it.
  - `architecture/wyrd-design.md` documents `observe.verify` beside `drift`,
    `eval`, and `record`.
  - Opening a run on one Card is a single step: Python `state.run("agent")`
    takes the Card alias as its first positional argument (today it is the
    keyword-only `card=`), matching TypeScript `state.run("agent")` and Rust
    `state.run_for_card("agent")`. The canonical example is:

    ```python
    with state.run("agent") as agent:
        judgment = agent.observe.verify("answer-is-yes", {"answer": "yes"})
    assert judgment.passed
    ```
- **REQ-189**: The `Verification` client handle (`get_binding`, `start_run`,
  `get_run`, `execute`) and the Python `wyrd.verification` module MUST be
  removed from the Rust, Python, and TypeScript SDKs, their exports, and their
  generated stubs and declarations. No alias or compatibility shim remains.
  `wyrd-client` keeps only the transport `observe.verify` needs. Rust's
  `WyrdError`, today reachable only as `wyrd_sdk::verification::WyrdError`,
  is exported at the SDK root as `wyrd_sdk::WyrdError`, matching
  `from wyrd import WyrdError` and the TypeScript root export. The Python
  error shape and catalog completeness are owned by the approved
  `py-error-refactor` change, not by this revision.
- **REQ-190**: An `http` Operator action whose `auth` names an `http`
  connection MAY give `url` as a path template beginning with `/`. The
  request then goes to the connection's stored origin, read at each attempt.
  A path-only `url` without a named connection MUST be refused at
  registration with the existing invalid-Operator error. Absolute URLs keep
  today's rules, including that their origin equal the named connection's
  origin. This lets one Card run unchanged against different environments.
- **REQ-191**: An authored artifact entry MAY omit `sha256` and `size_bytes`.
  The client computes both from the local file before any registration call.
  Values that are present are still checked and refused on mismatch with
  `RegistryManifestHashMismatch`. Served and downloaded Cards always carry
  both.
- **REQ-192**: Every client-facing test in the Rust, Python, and TypeScript
  SDKs MUST follow one standard, recorded in `TESTING.md` and used as the
  review checklist. Tests are the product's public examples: they are written
  for the user who copies them and the maintainer who will own them, so how a
  test is written is held to the same bar as what it proves.
  - **One story per file, one outcome per test.** A file covers one user
    story; each test name states the outcome the user gets (for example
    `test_agent_answer_passes_its_verifier`). A story has the same file name,
    test names, and fixtures in all three SDKs, so the languages can be read
    side by side.
  - **Checked-in YAML only.** Cards come from fixture directories under a
    repository-root `fixtures/cards/<story>/`, shared by the three SDKs. Test
    code never builds or edits YAML, JSON, digests, or URLs.
  - **Deployment-shaped server.** (revised in revision 66) A session
    `WyrdTestServer` exports its address and key the way a deployment's
    environment does, and SDK and CLI calls made as that deployment's default
    principal resolve them without arguments. A test that acts as any other
    principal passes a `WyrdClient` built for that principal's key through
    the call's optional `client` argument, the way a multi-tenant program
    would, and never changes the process environment to switch principal.
    (revised in revision 67) A `WyrdState` acting as a Service is created
    with that Service's client (REQ-209). A second test server a story
    starts does not export its address; its calls pass an explicit client.
    Ambient resolution itself is proved once per SDK by one test about
    configuration; in Rust, where changing the environment of a running test
    process is unsound, only that test may run in a child process.
  - **Public surfaces only.** A test uses the public SDK modules, the
    test-only CLI functions (REQ-196), the three test controls, and the
    credential fixtures (REQ-195). It uses no
    private or extension import, subprocess, raw HTTP, SQL against server
    tables, digest computation, YAML or JSON parsing of results, sleep, or
    polling loop.
  - **Setup is fixtures that return domain objects** (a `WyrdState`, a
    registered Card), never helper functions in the test file. A test body
    acts on the SDK and asserts on typed results, short enough to copy as an
    example.
  - **Errors assert one exact catalog code** on the raised `WyrdError`;
    message matching and accepting any of several codes are prohibited.
  - **Fixed, meaningful names**, never uuid or time suffixes; registering a
    fixture again is idempotent.
  - **Value tables use `parametrize` / `it.each` / a table loop with one
    assertion shape**, never branching inside a loop.
  - **Engine mathematics and internals stay in Rust tests**: PSI bins, SPC
    limits, judge scoring, cache and fence counters, and audit staging.
  - **Written to be owned.** Fixtures, fixture YAML, `conftest` and
    `tests/support` modules are production-quality code: minimal, realistic,
    typed, named for the domain, documented by intent, and free of dead
    options, magic values, and clever indirection. Fixture YAML reads as the
    Card a user would author. A fixture that hides an ugly flow behind a name
    does not satisfy this requirement.
- **REQ-193**: `cards.get(ref)` MUST return the same typed Card in all
  three SDKs (parity), for the implemented and tested kinds only: `Data`,
  `Model`, `Prompt`, `Agent`, `Verifier`, `Service`, `Trigger`, and
  `Operator`. It returns the envelope, a `spec` typed by `kind` (for example
  `VerifierSpec`, `ServiceSpec`), and the typed server-managed `status`,
  including a Drift Verifier's baseline state. No SDK exposes a typed spec
  for `Experiment`, `Workflow`, `Policy`, `Mcp`, `Audit`, `Artifact`, or
  `Source` until that kind is implemented and tested.
  Rust already returns `wyrd_spec::envelope::Card`. Python gains the method,
  and TypeScript's `spec`, today `Record<string, unknown>`, becomes a union
  discriminated by `kind`. The Python and TypeScript types are generated
  from the `wyrd-spec` JSON schemas, so that `codegen:check` fails when a
  language drifts. The kind-specific `cards.data`, `cards.model`, and
  `cards.prompt` loaders are unchanged. (revised in revision 67) The Python
  type is read by attribute, not by key: `cards.get(ref)` returns generated
  frozen dataclasses (`card.spec.…`, `card.status.verification.binding_ids`)
  discriminated by `kind`, built from the same schemas by the existing Card
  type generator. No runtime dependency is added.
- **REQ-194**: A Run view identifies itself by the alias it was opened with.
  The view property is `alias` (Rust `alias()`, Python and TypeScript
  `alias`), replacing the string `card_ref` / `cardRef`, whose value and
  documentation disagreed. The exact Card reference for an alias stays
  available from the state as a typed `CardRef` (`state.card_ref(alias)`).
- **REQ-195**: `WyrdTestServer` in all three SDKs documents exactly three
  test controls, each test-only and backed by the production code path:
  - `flush_bifrost()`: publish every accepted row now, so the next query
    sees it;
  - `wait_for_baseline(verifier, timeout)`: return once the named Drift
    Verifier's baseline is ready, or fail at the deadline with the last
    observed baseline state; and
  - `make_binding_due(...)`: make a schedule due.

  (revised in revision 65) It also provides credential fixtures that return an
  API key holding named Roles: a machine principal in the session tenant or a
  named other tenant, and the principal projected by a registered Service
  Card. They stand in for the operator who grants Roles in a deployment;
  no public surface grants a Role. A Card-scoped key without added Roles is
  still issued through the CLI (REQ-199). (revised in revision 67) Roles are
  now granted with the public `grant-role` command (REQ-212), and a Service's
  own key holds `wyrd_default` (REQ-213), so story journeys obtain a
  Service's key with the `issue_key` CLI function and grant any further
  Role with `grant_role`. The credential fixtures remain only for machine
  principals that a story needs but no public command creates: a principal
  holding named Roles in the session tenant, and another tenant's
  administrator.

  (revised in revision 66) The saved-user-login story additionally uses the
  test server's saved-login fixtures: enable human single sign-on on the
  test server, save a human login for a tenant into a configuration home,
  expire it, revoke its refresh chain, and report whether it is stale. They
  stand in for the identity provider and the passage of time, exist only on
  `WyrdTestServer`, and are used by no other story.
- **REQ-196**: (revised in revision 66) The `wyrd` executable ships with the
  Python and TypeScript packages and is the product surface for CLI
  commands. The in-process command functions are a test surface only and are
  absent from production builds: Python `wyrd.testing.cli.<command>(...)`,
  present only when the extension is built with its `testing` feature;
  TypeScript `cli.<command>(...)` from `@wyrd/testing`; and Rust
  `wyrd_sdk::cli::<command>` behind the `testing` feature, which replaces the
  `cli` feature. Production `wyrd.cli` keeps only the executable entry point,
  `@wyrd/sdk` exports no in-process command, and no compatibility alias is
  kept. Each function runs the same Rust command implementation as the
  `wyrd` executable, takes the command's options as typed arguments, returns
  the typed result the command prints with `--format json`, and raises
  `WyrdError` instead of returning an exit code. A networked command takes
  an optional `client` (`WyrdClient`); omitted, it resolves the server and
  credential from the ambient chain exactly as the executable does. The
  `client` argument replaces the `server` argument. At minimum `plan`,
  `apply`, `get`, and `load` are exposed; any further command a journey
  needs is exposed the same way.
- **REQ-197**: An observation emit never blocks the caller. A full queue
  refuses with `WYRD_CLIENT_429_QUEUE_FULL`; this is the documented contract,
  and no SDK adds a blocking or retrying emit.
- **REQ-198**: Every SDK client exposes `access_token()`, returning a current
  bearer token for its credential, for handing to third-party clients such as
  an OpenAI SDK pointed at the Gateway. No principal-id accessor is added.
- **REQ-199**: Card-scoped key issuance and Gateway provider credential
  writes are performed through the CLI functions (REQ-196). No separate SDK
  method is added. (revised in revision 67) Role grants (REQ-212) follow the
  same rule.
- **REQ-200**: The Bifrost query API in all three SDKs accepts bind
  parameters (`sql(query, params)`), and Oracle binds them server-side.
  Examples and tests read their own rows through the Bifrost client with
  parameters, never by interpolating values into SQL text.
- **REQ-201**: A tenant's built-in Bifrost tables are created when the tenant
  is created, so a query against a built-in table that has never been written
  returns zero rows instead of `TABLE_NOT_FOUND`. Existing tenants receive any
  built-in table they lack when the server starts, so a built-in added in a
  later release needs no per-tenant step. Lazy creation on first write or
  describe is removed.
- **REQ-202**: The SDKs expose typed values where the tests found raw wire
  objects: typed constructors for Operator-connection requests and
  `CardRef`; an options object for TypeScript `TableConfig.fromJsonSchema`;
  a catalog code on every local refusal; and no public TypeScript
  constructor that takes a native binding type. A trusted artifact hash is
  read from the registered Card's `artifact_hash`, not recomputed.
- **REQ-203**: Python Card and runtime authoring is typed end to end, and
  each capability exists in Rust and TypeScript wherever that SDK exposes the
  same Card kind:
  - `DataCard.from_path` and `ModelCard.from_path` load a saved Card
    directory or a Card YAML file in one call, as `PromptCard.from_path`
    does;
  - `DataCard` accepts typed `splits` and `target_columns`, and interface
    options read back as typed values;
  - `agent.to_card()` returns a typed `AgentCard`; callback context, prompt
    response schema (`prompt.response_schema`), `WyrdConfig` values, model
    signature dimensions, and query terminals are typed objects, not dicts
    or `repr` text;
  - a documented offline mock provider, with caller-set canned responses,
    is available from public modules for tests and examples.
- **REQ-204**: The client-facing test suites are brought to REQ-192 using the
  per-file verdicts in `review/sdk-test-audit/` as the inventory: tests marked
  DELETE are removed, MOVE tests are re-homed in the named Rust tier, and
  REWRITE and TIGHTEN tests are fixed. Type-only assertions move to
  compile-time type tests (`expectTypeOf`, `ty` fixtures outside pytest
  collection). Every test file runs in a `mise` lane, and the Python
  `WyrdTestServer` stub matches its runtime.
- **REQ-205**: Wyrd's OTLP endpoints accept the caller's API key directly in
  an `x-wyrd-api-key` header, validated on each request, so a stock
  OpenTelemetry exporter in any language is configured with standard OTel
  settings (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_HEADERS`) and
  keeps working past any access-token lifetime. No SDK exporter helper is
  added.
- **REQ-206**: Verification history is read with SQL through the Bifrost
  client, like any warehouse table; no SDK history API is added. How
  `vala.verification.results` stores its Drift and Eval summaries is owned by
  `SPEC-bifrost-variant` in `wyrd-forge`.
- **REQ-207**: (added in revision 66) Loading an authored Workflow accepts an
  optional `client` in all three SDKs: Python
  `Workflow.from_path(path, client=None)`, TypeScript
  `Workflow.fromPath(path, { client })`, and Rust
  `Workflow::from_path_with_client(path, client)` beside the unchanged
  `Workflow::from_path(path)`. Registry Card refs are then read, and the
  loaded Workflow's Gateway calls are made, as that client's principal;
  omitted, the ambient chain is used as today. A wholly local Workflow still
  needs neither a client nor a server.
- **REQ-208**: (added in revision 67) **One identity parameter.** Every
  public SDK entry point that calls the server takes exactly one optional
  identity parameter, a `WyrdClient`: Python `client=None`, TypeScript an
  options object `{ client }`, and Rust a `with_client(client)` constructor
  or a `&WyrdClient` argument beside a `from_env()` form. Omitted, the call
  resolves the client from the ambient chain (environment, then the active
  profile, then the newest saved login). The server URL, credential, and gRPC
  URL are arguments of the `WyrdClient` constructor only; no other public
  surface accepts them. This covers `Cards`, `Bifrost` (connect and table
  describe), `Gateway`, `OperatorConnections`, `Workflow` loading, and
  `WyrdState` (REQ-209), and Rust `Platform`. Rust `Cards::new(server_url,
  credential)` is removed and `Gateway` gains `with_client` and `from_env`
  like the other handles. The removed arguments get no compatibility alias.
  The `WyrdClient` constructor documents `credential` once, in every SDK: an
  API key (`wyrd_sk_…`), which is exchanged for an access token, or an
  access token, which is presented as-is.
- **REQ-209**: (added in revision 67) **A state's identity is fixed when it
  is created.** `WyrdState` takes its client at creation: Python
  `WyrdState.from_path(path, client=None, …)`, TypeScript
  `WyrdState.fromPath(path, { client })`, and Rust
  `WyrdState::from_path_with_client(path, client)` beside `from_path(path)`.
  `start_bifrost` / `startBifrost` takes no identity argument, and every
  server call the state makes, including Bifrost startup and
  `observe.verify`, runs as that client. A state created without a client
  resolves the ambient chain once, at its first server call, and keeps that
  client; later environment changes do not change its principal.
- **REQ-210**: (added in revision 67) **One public surface across SDKs.**
  The Rust, Python, and TypeScript SDKs expose the same public operations,
  with the same argument names (in each language's casing), argument kinds,
  defaults, and typed results. In particular:
  - `cards.hydrate(ref, destination, metadata_only=False)` downloads a
    registered Card's reachable graph as a bundle in all three SDKs (Rust
    `Cards::hydrate(selector, destination, mode)`); Python gains it, and a
    bundle no longer requires the CLI.
  - `Cards` exposes root `get`, `list`, `delete`, and `resolve_latest` in
    all three SDKs.
  - Bifrost `stream(query, params, deadline)` has one shape in all three
    SDKs; Rust keeps one table-describe name (`describe_table(namespace,
    name)`); TypeScript exposes `dropped`.
  - `state.run(alias)` names its argument `alias` everywhere, and the typed
    state accessors exist for every kind the state hydrates.
  - `Workflow.run` accepts the same input kinds, and `Workflow.from_yaml`
    exists wherever `Workflow.from_path` does.
  - (revised in revision 68) The same concept has the same name everywhere:
    a state exposes `root_ref` (the root `CardRef`) and `service` (the typed
    root Card), and a Workflow exposes its step ids as `steps`.
  - (revised in revision 68) Workflows are authored as YAML and loaded with
    `from_path` or `from_yaml`; no SDK exposes programmatic Workflow
    builders.

  Allowed differences are named idiom exceptions only:
  - Python's Data and Model holders and per-kind registries;
  - programmatic `register` in Rust and Python (revised in revision 68);
  - a synchronous twin where the language has blocking IO: Rust's blocking
    Bifrost and Python's `Bifrost` beside `AsyncBifrost` (added in revision
    68);
  - row-model typing (`model=` / `sql_as::<T>` / a row type);
  - row serialization on `insert`;
  - Rust's `run()` / `run_for_card(alias)` and `verify` / `verify_with_media`
    pairs, where Rust has no optional arguments;
  - Python's synchronous `record`;
  - TypeScript string paths;
  - Python's agent runtime (`Agent.run`);
  - the operator-only Rust `Principals`, `Platform`, and storage handles;
  - (added in revision 69) language-native ecosystem adapters that wrap a
    shared surface without adding server behavior: Python's
    `QueryResult.to_pandas` / `to_polars` and
    `wyrd.otel.install_run_correlation`, and (added in revision 70)
    Python's and TypeScript's `QueryResult.to_arrow` / `toArrow`, which
    wrap the shared `batches` in the language's Arrow table type that Rust's
    arrow-rs does not have;
  - (added in revision 69) Rust's `ClientConfig`, `Environment`, and
    `GlobalConfig` as the Rust spelling of the `WyrdClient` constructor
    options that Python and TypeScript take as keyword or object arguments;
  - (added in revision 69) test-harness CLI helpers that only load fixtures,
    such as Rust's `cli::load` taking `Option<&Path>`.

  An SDK that exposes a surface no other SDK has, outside this list, either
  gains the surface everywhere or loses it.

  (added in revision 68) Parity is measured on the SDK packages
  (`wyrd_sdk`, Python `wyrd`, `@wyrd/sdk`), not on `wyrd-client`, which is
  also the shared implementation behind the CLI, MCP, the test harness, and
  the language bindings. `wyrd_sdk` re-exports only parity types; remote
  `Workflows` and `PublicWyrdGatewayCaller` are not re-exported. Low-level
  methods on re-exported types (raw requests, `from_parts`, Bifrost query
  and writer internals, bundle introspection, `Run::subject`, and the
  `Cards` response and download plumbing) are compiled only under a
  `wyrd-client` `internal` feature. `wyrd-cli`, `wyrd-mcp`, `wyrd-testing`,
  and the Python and TypeScript binding crates enable it; `wyrd-sdk-rust`
  never does, and `check:deps` fails if it does.
- **REQ-211**: (added in revision 67) **References and paths.** Every
  selector, loader, and delete accepts a `CardRef` as well as its
  identity or uid pieces, and every path parameter accepts the language's
  path type: Python `str | os.PathLike[str]`, Rust `impl AsRef<Path>`, and
  TypeScript `string`. Docstrings describe what the path names (a Card
  file or a bundle directory) accurately.
- **REQ-212**: (added in revision 67) **Role grants.** A tenant
  administrator grants a built-in or tenant Role to a Card-bound Service or
  Agent principal with `wyrd auth grant-role --kind --name --version --space
  --role`, backed by `POST /v1/auth/grant-role` taking `{ card_ref, role }`
  and returning the principal's roles. The route requires the tenant
  administrator permission (`*`), not `service_accounts:write`. It is
  idempotent, tenant-isolated, answers an unknown or foreign Card with the
  existing non-enumerating not-found code and an unknown role with
  `WYRD_SPEC_400_VALIDATION`, and stages the `auth.principal.role.grant`
  authorization decision on the audit outbox. A grant takes effect at the
  principal's next key exchange. Human users are not grant targets, because
  federated login replaces their roles. The command is also a test-only CLI
  function (REQ-196) in every SDK. No revoke command is added.
- **REQ-213**: (added in revision 67) **A usable default Role.** A Service
  or Agent principal's first projection grants the built-in `wyrd_default`
  Role (`bifrost_table:read`, `bifrost_record:write`, `evals:run`) instead of
  `workload`, so the key `wyrd auth issue-key` issues for a Service can
  start Bifrost, emit its evidence, and verify without any grant. Its Card
  scope still bounds which Cards it may emit for and verify. Tenant-wide
  Bifrost query reads are not in the default; they come from an explicit
  grant of the built-in `workload` Role, which keeps its permissions.
  Bifrost reads scoped to a principal's own Card are not added; they need an
  Oracle row filter that does not exist.
- **REQ-214**: (added in revision 67) **Every public parameter is
  documented.** Each public callable documents every parameter's name, type,
  and meaning, its return value, and its errors, in the SDK's native form:
  - Python: Google-style `Args:`, `Returns:`, and `Raises:` in the stub
    sources. The stub assembler's existing docstring gate requires an
    `Args:` entry for every non-`self` parameter, and Ruff's `D417` runs on
    the stubs.
  - TypeScript: `@param`, `@returns`, and `@throws` in `index.ts`.
  - Rust: an `# Arguments` section on public SDK functions that take
    parameters.

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
  It does not promise atomic visibility across the separate Bifrost result
  tables.

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

### TASK-006 server startup and peer remediation

- **REQ-153**: The published Wyrd application image MUST serve the Rust API,
  live Node BFF, and nginx HTTP routing as one application container. The
  public router MUST pass the actual Rust API, auth, platform, MCP, OpenAPI,
  and health paths without prefix rewriting and serve UI requests through the
  BFF. Nginx MUST expose a working public gRPC listener on port `50051` and
  forward it to the server's internal gRPC listener. `WyrdClient` MUST derive
  the public gRPC host and scheme from its effective `server_url` (explicit
  constructor argument, then `WYRD_SERVER_URL`, then the documented default)
  and use port `50051` by default. Rust, Python, and TypeScript public SDKs
  MUST expose the same resolution behavior. The existing optional `grpc_url`
  argument or `WYRD_GRPC_URL` MAY override that address for deployments with
  separate routing; neither is required for the standard journey. Nginx and
  the Rust gRPC listener MUST use distinct internal bind addresses or ports.
  Public TLS MUST terminate at the deployment edge; the server's internal
  gRPC listener is plaintext and has no certificate inputs. Peer mTLS remains
  separate.
  The published server binary MUST be built with the workspace `dist` Cargo
  profile (`release` plus fat LTO, one codegen unit, stripped symbols); local
  image builds and test lanes MAY use `release`.
- **REQ-154**: Wyrd MUST require an external PostgreSQL service for local and
  production use. It MUST remove embedded PostgreSQL boot and download. The
  local supported journey MUST provide durable Postgres separately and start
  the same one-process Wyrd boot path used in production. Missing database or
  durable-storage inputs MUST produce actionable startup failures.
- **REQ-155**: A published image MUST provide one idempotent first-use setup
  command that creates the initial platform administrator, tenant, and usable
  client credential through existing Wyrd authority. A retry MUST NOT silently
  mint a replacement credential. Credential disclosure MUST be confined to the
  invoking terminal and remain valid across application restart.
- **REQ-156**: Serving Wyrd processes MUST have exactly two PostgreSQL login
  identities: an RLS-bound `wyrd_app` for tenant work via `WYRD_DATABASE_URL`
  and a narrowly granted `wyrd_platform_admin` via
  `WYRD_PLATFORM_DATABASE_URL` for explicit cross-tenant/platform and Iceberg
  catalog work. The app role MUST NOT bypass RLS or read catalog metadata;
  the platform role MUST NOT reach ordinary tenant request handlers or gain
  cluster-role creation or unrestricted DDL. Tenant API principals MUST NOT
  require individual PostgreSQL logins. The database-owner credential MUST
  never be mounted in a serving process.
  *Superseded in part by `SPEC-local-server-bootstrap` D-001:*
  `WYRD_PLATFORM_DATABASE_URL` is optional and defaults to
  `WYRD_DATABASE_URL`. Operators supply the logins; Wyrd names and provisions
  no role. The pool decides scope: tenant pools are bound by
  `tenant_isolation`, and the platform pool's operator session is admitted by
  `operator_access` only for the login that owns Wyrd's objects. One ordinary
  login may serve local and small self-hosted deployments; production uses a
  separate tenant login. Superuser and `BYPASSRLS` logins are refused, and
  `wyrd-server migrate` runs as the platform login.
- **REQ-157**: The image MUST expose `wyrd-server migrate` as a one-off mode
  using an existing database-owner URL supplied through `WYRD_DATABASE_URL` to
  that invocation only. Normal server boot MUST NOT run migrations; it MUST
  verify applied Wyrd/Vala migration versions and checksums, RLS, and required
  grants before readiness, failing closed on missing or incompatible state.
  Local deployment MAY automate the one-off mode; production MAY run it through
  its orchestration system or manually before serving the new image.
- **REQ-158**: No Wyrd release has shipped, so the unshipped migration SQL and
  role bootstrap MUST be revised directly for a fresh install with only
  `wyrd_app` and `wyrd_platform_admin` serving logins. The migration owner
  owns schema changes; the platform role owns Iceberg catalog objects; the
  app role has no catalog access. `wyrd_migrator`, `wyrd_catalog_app`, and
  `wyrd_catalog` MUST be removed rather than retained as compatibility roles.
  Repository-managed development and test databases with obsolete migration
  checksums MAY be reset. Once a release ships, future migration history MUST
  remain checksum-stable. A failed production migration MUST stop rollout;
  retry/repair or verified backup recovery MUST preserve accepted tenant data
  and audit evidence. Wyrd MUST NOT offer an unproven generic down-migration
  or automatic restore.
- **REQ-159**: The default `all` target MUST run local Scribe, Oracle, and
  Forge calls in-process without a private peer listener, certificate, peer
  bearer token, ticket, or calibration profile. It MUST retain bounded normal
  admission defaults. A deployment configured for future replicas MUST run
  the peer listener and require its peer inputs even while it has one replica;
  calls to components in that same process still stay local.
- **REQ-160**: Remote peer transport MUST use mutual TLS alone for cluster
  member authentication. Each peer-mode process MUST read one dedicated Wyrd
  cluster CA certificate and one shared peer leaf certificate/private key from
  `WYRD_PEER_TLS_DIR` (`ca.crt`, `tls.crt`, `tls.key`). The CA private key MUST
  remain outside Wyrd processes. TLS MUST validate the CA chain, validity,
  client/server usage, and fixed `wyrd-peer` peer identity; missing or invalid
  inputs MUST prevent ready membership. Peer API keys, separate peer JWTs,
  signed purpose tickets, ticket keyrings, and nonce replay state MUST be
  retired. Public client authentication and authorization remain required.
- **REQ-161**: Possession of the shared peer certificate MUST confer only
  trusted-cluster-process identity, not tenant identity or a specific replica
  identity. For every private operation—query forwarding, reservations,
  stages/fragments, shuffle reads, and Scribe tail list/acquire/read—the
  receiver MUST validate applicable typed tenant, table/object/snapshot,
  query/assignment, target node and fence, deadline, and resource bounds
  against its own trusted state before decoding executable plans or touching
  tenant storage. A mismatched or unresolvable context MUST fail closed with
  no cross-tenant result. This trust model does not promise protection against
  a compromised cluster peer that holds the shared private key.
- **REQ-162**: Peer-mode processes MUST publish their own reachable private
  address through `WYRD_PEER_ADDRESS`, supplied per instance by the deployment
  runtime. Wyrd MUST validate and register that route in the existing
  `vala.cluster_nodes` role/fence/heartbeat registry; it MUST NOT infer a
  reachable address from a wildcard bind, require a manual peer list, or add a
  second discovery service. Existing membership polling MUST discover a newly
  started replica without restarting peers. A remote failure MUST not return
  partial query results. In Kubernetes, the shared pod template MUST inject
  `status.podIP` into `POD_IP` through the Downward API, then set
  `WYRD_PEER_ADDRESS` to `$(POD_IP):<peer-port>`. The pod UID is not a
  routable address; replicas MUST NOT need individually authored addresses.
- **REQ-163**: Peer-mode replicas MUST use one shared durable object store for
  Card, artifact, and Bifrost/Iceberg objects; a process-local `file://`
  backend MUST fail peer-mode startup. Each Scribe MUST retain a private
  persistent `WYRD_BIFROST_DATA_DIR` for WAL, durable staging, and stable node
  identity; Oracle spill may be disposable. Standalone mode MAY use a durable
  local `file://` backend. Local file upload/download URLs MUST be
  root-relative authenticated Wyrd routes resolved by clients against their
  configured server endpoint, with no `WYRD_PUBLIC_BASE_URL` input; cloud
  provider-presigned URLs remain absolute.
- **REQ-164**: The unshipped private ticket protocol MUST be removed without a
  compatibility shim or old/new peer rollout. After the first release, rolling
  upgrades MAY overlap only when both release manifests declare mutual
  compatibility for schema, object, wire, and peer contracts; incompatible
  peers MUST refuse each other before payload decoding. Otherwise deployment
  MUST drain old peers before replacing them.
- **REQ-165**: The documented default configuration MUST name only inputs
  required by the selected topology and feature. It MUST remove obsolete
  embedded-Postgres, migrator/catalog password, peer API-key/ticket, manual
  peer-identity, and public-base-URL inputs from startup parsing and examples.
  Wyrd MUST NOT accept certificate inputs for its public HTTP or gRPC
  listeners; hosting-platform TLS termination serves that traffic.
  Storage-provider and OIDC sealing credentials remain
  conditional on their selected providers/features. The owning security and
  deployment architecture documents MUST be synchronized before completion.
- **REQ-166**: Wyrd MUST remove `POST /v1/authz/check` and `PolicyHook` from
  its server, shared authorization code, public contracts, client surfaces,
  and documentation. The route MUST be absent from the router and OpenAPI;
  requests to it MUST receive the normal unknown-route response, not a
  disabled or allow-by-default decision. Delete the always-allow hook from
  delegated token exchange, its policy-only branches, and the fake `invoke`
  policy attribution in exchange audit. Existing token exchange and Wyrd API
  authorization otherwise remain intact. No replacement policy gate is part
  of this change.

### Direct execution, telemetry, and verification capacity (revision 51)

- **REQ-167**: `POST /v1/verification/execute` MUST synchronously execute
  one exact registered Verifier for one exact subject over supplied input and
  return the judgment in the response. The request is `{ verifier_uid,
  subject_card_uid, input }`. `input` is tagged by `kind`:
  `eval_record { context: object, media?: [MediaRef] }` for assertion-only
  and LLM-judge Evals, with judges receiving the supplied context; or
  `drift_samples { columns: { <feature>: [number | string | null] } }`, where
  PSI and SPC score against the Verifier's `ready` fitted baseline and Custom
  scores the mean of `profile.metric_name`. Success is `200 { execution_id,
  verifier, subject, kind, verdict, summary, counts, detail }`. `verdict` is
  `passed | failed | inconclusive`, and `detail` is `{ drift: DriftReport }`
  or `{ eval: EvalReport }`. `execution_id` is a UUIDv7 that appears only in
  the response, audit, and trace. It is never persisted or queryable. A
  `failed` verdict is a successful response. The operation MUST NOT create a
  durable run, publish a result, dispatch an Operator, read or write Bifrost,
  or apply the Eval sampling policy. Registry, baseline, and judge
  Agent/Prompt resolution and canonical audit still use PostgreSQL.
- **REQ-168**: Direct execution MUST require `evals:run` with exact Verifier
  and subject scope, exactly as the direct-target run start does. The
  permission check blocks; the audit does not. It MUST stage exactly one
  allowed or denied decision per request on the process audit outbox shared
  with Oracle, without waiting for its commit; a decision that fails to
  commit is logged and counted and never refuses the request. Unknown or
  cross-tenant targets return `404 verification_target_not_found`. Bounds:
  - a request body of at most 1 MiB;
  - `drift_samples` of at most 64 columns × 100,000 values;
  - `eval_record.context` of at most 256 KiB;
  - one 60-second execution deadline;
  - no concurrency cap or admission layer.

  Stable errors:

  | Status | Code | Condition |
  |---|---|---|
  | 400 | `verification_input_invalid` | Malformed input |
  | 413 | `verification_input_too_large` | A bound is exceeded |
  | 403 | existing RBAC code | Permission denied |
  | 409 | `verification_baseline_not_ready` | No `ready` fitted baseline |
  | 409 | `verification_baseline_legacy` | Baseline fitted under an earlier format |
  | 422 | `verification_input_incompatible` | Missing feature or type mismatch |
  | 422 | `verification_input_unsupported` | An Eval with trace or agent assertions, refused before any task runs |
  | 502 | `verification_dependency_failed` | Judge provider failure after the task's own `max_retries` |
  | 504 | `verification_execution_timed_out` | The deadline elapsed |

  The operation is not idempotent, takes no `Idempotency-Key`, and is never
  retried automatically by an SDK. A client disconnect cancels in-flight work.
  Provider calls already issued may have incurred cost.
- **REQ-169** (revised in revision 64): The Rust, Python, and TypeScript SDKs
  MUST expose REQ-167 only as `observe.verify(...)` (REQ-188), implemented
  once in `wyrd-client` without duplicating transport or scoring. MCP MUST
  expose the write tool `verification_execute`,
  gated on `evals:run`, and the served OpenAPI MUST describe the operation.
- **REQ-170**: Verifier execution telemetry MUST follow the TASK-008 closeout
  telemetry contract. Required elements:
  - one execution owner shared by queued and direct paths;
  - closed labels: `kind` (`drift_psi`, `drift_spc`, `drift_custom`,
    `eval_assertion`, `eval_llm_judge`, `eval_other`, `unknown`), `mode`
    (`queued`, `direct`), `origin`, `phase` (`load`, `input_read`, `prepare`,
    `engine`, `publication`, `settlement`), and the bounded `outcome` sets;
  - `wyrd_verification_engine_overhead_seconds`, computed as engine elapsed
    time minus the union of measured wait intervals;
  - the shared bucket set
    `0.0005, 0.001, 0.0025, 0.005, 0.0075, 0.009, 0.01, 0.025, 0.05, 0.1,
    0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 300`;
  - INFO-level correlated spans and queued task-start delay;
  - the operator catalog and its PromQL.

  No Grafana deliverable is part of this change.
- **REQ-171**: `mise run bench:capacity` is Wyrd's one capacity benchmark. It
  replaces `bench:verification:capacity` and `bench:bifrost:ingest-capacity`;
  no other server capacity benchmark exists. It follows the Google SRE
  load-test shape: one production-shaped workload, a ramp to find
  saturation, sustained steps at load, and the four golden signals judged
  against stated SLOs. A default run MUST complete within 30 minutes,
  including setup, against release `wyrd-server` replicas in peer mode over
  Postgres and the RustFS emulator, each in an 8-CPU/16-GiB scope.
  - **Workload.** One mix of everything the server does, driven open-loop
    through the public Rust client and spread evenly over four tenants with
    identical traffic. The load level `L` is verification executions per
    second. At level `L` the mix is:

    | Operation | Rate | Shape |
    |---|---|---|
    | Verification, direct | `L/2` per second | PSI, SPC, Custom, assertion Eval, LLM-judge Eval in equal shares (AC-040 reference workloads) |
    | Verification, queued | `L/2` per second | same five kinds and shares |
    | Scribe ingest | `2.5·L` Drift observations per second | 100 features each, through `WyrdState` with default queue configuration |
    | Oracle query | `L/2` per second | equal shares of a selective lookup and a small aggregate over the last 5 minutes of the tenant's ingested data |

    Audit, run settlement, Scribe publication, and Forge maintenance run as
    the server's own side effects of this mix. Nothing else is driven.
  - **Steps.**
    1. warmup: 30 seconds at `L = 50` on one replica, not judged;
    2. ramp: 60-second steps at `L` = 50, 100, 200, 400 on one replica,
       stopping at the first step that misses an SLO; the highest passing
       step is the knee `K`;
    3. sustained: 180 seconds at `K` on one replica;
    4. sustained: 180 seconds at `K` on two replicas;
    5. scale-out: 180 seconds at `2K` on two replicas.
  - **SLOs.** A step passes only when every SLO holds:

    | Golden signal | SLI | SLO |
    |---|---|---|
    | Traffic | achieved ÷ offered, per operation | ≥ 95% |
    | Errors | requests that failed, were refused, were lost, or produced a wrong judgment | 0 (intentionally failing verifier inputs are judgments, not errors) |
    | Latency | direct verification paired engine overhead, non-judge kinds | p95 < 10 ms (AC-040) |
    | Latency | Scribe ingest: client queue drain after load stops | ≤ 1 s |
    | Latency | client p50/p95/p99 per operation | reported |
    | Saturation | every backlog (run queue, Scribe, audit outbox, Forge demand) after load stops | drained within 60 s |
    | Saturation | replica CPU cores and peak memory | reported |

  - **Verdict.** The run passes when the sustained one-replica step and both
    two-replica steps pass. The scale-out step passing is the proof that two
    replicas carry more than one.
  - **Report.** One table, one row per step, one column per SLI, each cell
    marked PASS or FAIL, then one per-operation row per step with the same
    columns for diagnosis. Nothing else is judged.
  - Correctness and isolation are proven by tests, not by this benchmark:
    tenant fairness, exactly-once queued claims across replicas, judgment
    correctness, and the AC-041 queue properties.
  - `--profile` captures symbolized per-step, per-replica `perf` profiles of
    a diagnostic build and fails explicitly on missing evidence.

### Client ingestion throughput and memory (revision 53)

The client queue behind `observe.*` and `Bifrost.insert*` follows the
established producer model of Kafka and librdkafka: per-destination batching
lazily created under one shared memory budget, byte- and linger-triggered
batches, several idempotent sends in flight, and immediate refusal at the
call site when the budget is exhausted.

- **REQ-172**: One `Bifrost` handle MUST bound every client-owned ingestion
  byte with one configurable handle-wide budget, `client_byte_limit_bytes`.
  Every byte counts against it, from admission until the server acknowledges
  it or a definite refusal settles it as a counted loss:
  - admitted rows;
  - sealed batches;
  - in-flight batches;
  - retained ambiguous batches.

  This budget is the only memory bound. In particular:
  - Any number of destination tables may be written, each through one lazily
    created producer per (scope, table).
  - There is no producer-count limit, no per-producer share of the budget, and
    no row-count capacity.
  - Configured values are honoured without library ceilings. The current 32
    MiB, 64-entry, 1,024-row, 4,096-row, and 50,000-row clamps are removed.
  - Sealing keeps one maximum-message headroom that admission cannot consume,
    so admitted rows can always be sealed.
  - The default budget is 256 MiB. A user MUST be able to override it when
    connecting Bifrost or starting a `WyrdState`'s Bifrost in Rust, Python,
    and TypeScript. An override smaller than one maximum message plus its
    sealing headroom is refused at connect time.
- **REQ-173**: The queue MUST NOT preallocate. No buffer, slot array, or
  channel storage is sized from a configured capacity or budget before rows
  arrive. Memory grows with admitted rows and is released on settlement. An
  idle producer reserves no budget bytes beyond its own bounded bookkeeping.
- **REQ-174**: A producer MUST seal a batch when either of these occurs:
  - its staged rows reach the configured `max_message_bytes` frame target; or
  - the configured linger has elapsed since the first staged row and one of
    its `max_in_flight` send slots is free. The linger default is 5 ms. While
    every slot is busy, staged rows keep accumulating until a slot frees or
    the frame target is reached, so batches grow under backpressure.

  An explicit flush or shutdown seals immediately. Row-count triggers and the
  1-second default interval are removed. Intake MUST continue while sends are
  in flight as long as the budget has room.
- **REQ-175**: Each producer MUST allow up to a configurable number of
  concurrent sends, `max_in_flight`.
  - Every batch keeps one UUIDv7 for all its retries, and Scribe's batch
    deduplication absorbs replays.
  - Retained ambiguous batches retry with backoff without blocking other
    sends.
  - Batches of one producer carry no delivery-order guarantee; rows within
    one batch keep admission order. Consumers order by row timestamps, never
    by arrival.
  - The default `max_in_flight` is the smallest value that meets AC-041,
    recorded with the `bench:capacity` evidence.
- **REQ-176**: Admission MUST be immediate and all-or-none per logical record.
  For example, every tall row of one Drift observation is admitted, or none is.
  - A record that cannot fit in the remaining budget returns
    `WYRD_CLIENT_429_QUEUE_FULL` and admits no row, so the caller may flush or
    back off and then resubmit the same record.
  - A record larger than the whole admission budget returns
    `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE`.
  - Admission never waits on the network.
  - No blocking or awaitable admission API is added.
- **REQ-177**: A send that ends without a definite ACK or refusal, including
  an exhausted transport retry budget or deadline, MUST retain its batch and
  identity until it is reconciled. Only a definite refusal settles a batch as
  a counted loss.

### Gateway capture writer (revision 54)

Gateway call capture is server-internal evidence, written the way Bifrost's
own engines reach Scribe. It does not impersonate a client: no token, no
per-tenant embedded client, and no loopback through the public ingest
listener. This revision supersedes the capture-authority mechanism in
`changes/active/wyrd-gateway-port/spec.md` REQ-006 and its capture boundary.

- **REQ-178**: Each `wyrd-server` process MUST own exactly one capture writer.
  - When Scribe is active in the same pod, the writer submits the projected
    `vala.gateway.calls` and `vala.traces.spans` batches to it in-process.
  - Otherwise it submits them over the existing mutually authenticated peer
    plane to a live, ready Scribe, through a capture-only peer ingest RPC
    that carries the tenant and destination table explicitly.
  - The choice follows pod topology (`WYRD_TARGET`), never table ownership.
  - The peer RPC is served only by pods running Scribe and admits only
    `wyrd-peer` client certificates. It refuses any table other than the two
    capture destinations and the three Verifier result tables (REQ-086), and
    any reserved system tenant. Widening it to other server-internal writers
    requires a spec revision.
- **REQ-179**: Capture MUST be a server-internal write. It holds no token,
  evaluates no permission, and writes no audit decision. Captured rows carry
  the reserved `GATEWAY_CAPTURE_PRINCIPAL` identity, which, like
  `PLATFORM_AUDIT_PRINCIPAL`, never appears in a token. The capture token,
  its issuance and verification paths, the capture role, and Gate's
  principal exception are removed. Gate MUST refuse every public write to
  `vala.gateway.calls`.
- **REQ-180**: A capture is delivered when Scribe acknowledges it before the
  call's deadline.
  - Retryable refusals (backpressure or an unavailable peer) are retried with
    bounded backoff until that deadline.
  - Any other failure, or the deadline elapsing, drops the capture with a
    counted, logged reason and never changes the call's result.
  - Capture holds no per-tenant queue, client, or in-memory backlog beyond
    the in-flight attempt.
  - Each capture batch has a deterministic identity derived from the tenant,
    call, and table, so a retried or redirected submission is absorbed by
    Scribe's batch-id dedup.

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
  server machinery and plumbing; REQ-153–REQ-157 are the explicit exception
  correcting PSI/SPC behavior. Removing obsolete implementation refs is the
  only other intentional logical input-record subtraction. The existing Eval
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
- **INV-016**: Local engine calls need no network credential. Remote peer
  identity is cluster scoped; public principal authorization, tenant RLS,
  receiver-side tenant/resource/fence checks, and canonical audit remain
  separate mandatory boundaries.
- **INV-017**: A serving process never receives the PostgreSQL owner
  credential. No separate migrator or catalog role or password is required.
- **INV-018**: A multi-replica Wyrd deployment shares one authoritative object
  namespace while each Scribe owns its own durable WAL/staging identity.
- **INV-019**: Client-owned ingestion bytes never exceed one handle's
  `client_byte_limit_bytes`. A refused record admits no row. A batch is
  released only by a durable ACK or a definite refusal, and never by
  ambiguity.
- **INV-020**: Gateway capture cannot write outside its tenant and the two
  capture destinations. No public principal, including one claiming the
  capture identity, can write `vala.gateway.calls` through Gate.
- **INV-021**: A Verifier run has at most one result. Every write of it, by
  any replica and after any crash, carries the same stored bytes and batch
  IDs, and no public principal can write a Verifier result table.

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
  vala.verification.results and vala.drift.result_features rows joined by `result_id` within the caller's
  tenant-scoped query, and queryable results. A Custom metric journey
  proves ready registration without a fit job and server-side window mean.
  Empty and invalid Custom windows MUST persist `completed/inconclusive` with
  null `details` and zero feature rows; scored Drift MUST persist its existing
  report and produced feature rows.
  Invalid/non-Parquet baselines, failed fitting, insufficient data,
  unauthorized access, and cross-tenant reads fail visibly.
- **AC-034**: Published NIST X-bar/S examples or independently calculated
  fixtures MUST prove both chart limits, subgroup mean/standard-deviation
  signals, equality at the limit, and the missing `sqrt(n)` regression. PSI
  fixtures MUST prove exhaustive numeric/category bins, including unseen
  target categories and zero-count smoothing. Baseline and target fixtures
  MUST prove null, non-finite, omitted feature, insufficient sample, and
  incomplete subgroup outcomes without silent row removal. Direct PSI/SPC
  fixtures MUST pass only preselected relevant rows, prove that a selected
  null-only row makes an otherwise sufficient target wholly unscored, and
  prove that unrelated observations are excluded before direct scoring and by
  the server. Real Rust,
  Python, and TypeScript SDK-to-server Drift journeys MUST prove the new
  report evidence, a failed scheduled run's Operator dispatch, and the
  visible refusal of legacy versions; direct scoring and server scoring must
  agree for the same valid data. Existing tenancy, fixed-window, audit,
  capacity, timeout, and result-publication obligations remain.
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
  the new vala.eval.observations row is acknowledged without waiting for run
  creation, and its runs appear through the run-request outbox.
  The created run MUST retain the committed row's exact `record_id` and
  server-managed `wyrd_event_time`, and its input read MUST demonstrate UTC-day
  partition pruning with those frozen values even when client `created_at`
  falls on a different day.
  Successful enqueue runs the existing Eval executor and persists
  vala.verification.results plus vala.eval.result_items joined by `result_id` within the caller's
  tenant-scoped query. A workflow that skips a task MUST persist
  its `TaskRunOutcome::Skipped` beside every `Ran` task outcome, while the
  common result's `details` serializes `EvalWorkflowSummary`. A forced
  Postgres outage during a flush MUST keep the requests and create exactly one
  run per matching binding once Postgres returns. A repeated request MUST
  create no duplicate. Graceful shutdown MUST flush queued requests before
  exit. A failing
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
  processor registration failure, span enrichment failure, and exit
  context-update failure do not escape or block explicit observations; a user
  exception from the block MUST propagate unchanged. Unknown aliases MUST
  still fail before entry. Repeated entry MUST register the processor once on
  a normal provider by way of the provider marker, and every optional
  registration failure MUST leave explicit observations working; duplicate
  registration under a concurrent first entry is permitted. A nested scope
  MUST NOT overwrite `wyrd.card_ref`
  on an already-active span that carries one, and an explicitly installed
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
  automation, server-internal SYSTEM work, cached-token requests, and ordinary observations
  do not activate or renew an owner. Inactivity, suspension, and deletion
  prevent scheduled work; later reauthentication starts at the next future
  occurrence without backfill. Two A/B versions remain independently
  active, replicas of one exact principal do not duplicate runs, component
  bindings inherit Service activity, and ordinary requests or observations
  do not write activity or require a heartbeat.
- **AC-020**: Supporting integration tests MUST exercise the real Postgres
  registration/auth/binding/run/dispatch seams, Oracle/Scribe result and
  observation seams, scheduler claim and lease expiry, baseline fitting,
  the Eval run-request outbox, and Operator retry/fanout. Unit tests cover
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
  without local Scribe or Oracle and prove that its result batches reach a live
  Scribe through the peer ingest RPC, are acknowledged, and are queryable, and
  that a Drift run completes through a peer-forwarded Oracle with an audited
  SYSTEM read decision. It MUST assert input rows carry the client
  Service/Agent `principal_id` and subject `card_uid`, while result/detail
  rows carry the tenant-scoped SYSTEM `principal_id`, exact Verifier
  `card_uid`, and explicit subject/owner/binding IDs. Provisioning MUST create
  one stable UUIDv7 SYSTEM principal per tenant without a public credential.
  Tests MUST reject public create, list, get, update, suspend/delete,
  credential, refresh, workload, delegation, impersonation, and token-exchange
  operations for SYSTEM; the issuer's refusal to mint a SYSTEM token;
  verification's refusal of any `kind=system` claim set; every public write to
  a result table through Gate, including from a wildcard administrator; a
  result row whose `card_ref` differs from the submitted Verifier; and a SYSTEM
  read of another table or another tenant. Two bindings for one subject MUST
  remain independently filterable through runs/results while sharing the one
  raw subject observation without Verifier/binding columns or per-binding
  copies.
- **AC-024**: Bifrost catalog/schema tests MUST assert the exact column names,
  order, Arrow types, and nullability in `architecture/logic/table_schema.md`,
  daily `wyrd_event_time` partitioning for all five verification tables, and the
  resolved Bloom-column union for `vala.verification.results` and both
  result-detail tables. A real tenant-scoped query MUST retrieve results by
  Verifier `card_uid`, subject
  Card UID, and binding ID, then retrieve matching details joined by `result_id` within the caller's
  tenant-scoped query across at least two time partitions.
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
  management and Gate's refusal of every public result-table write. A multi-tenant runtime journey
  MUST execute more than sixteen held Verifier runs, including more than four
  for one tenant, while another tenant also progresses. Baseline fitting MUST
  proceed while those runs are held. Operator delivery MUST still enforce four
  executions per tenant and sixteen per process. A slow local Operator endpoint
  MUST prove the 30-second attempt timeout,
  three-attempt budget, 30-second/two-minute retry schedule, five-minute
  deadline, and terminal status without rerunning the Verifier. Shutdown tests
  MUST prove claims stop immediately, work drains for at most 30 seconds, and
  unfinished durable work is recoverable with the same identity after restart;
  an unexpected worker exit MUST restart and be visible through health,
  tracing, and the required runtime metrics.

For pre-release TASK-006 acceptance, AC-034, AC-037, and AC-038 use the
official image recipe built from the reviewed commit, pinned and recorded by
its immutable local image ID. No published image is required before the first
release. The first release MUST repeat those image journeys against the
published image pinned by an immutable registry digest before release.

- **AC-034**: Starting from the pinned official image and a fresh external
  Postgres database, the local deployment MUST run migration, start one
  application container, execute the one setup command, and use a real client
  to write and read through nginx. Restart MUST preserve credentials, Card and
  artifact bytes, and acknowledged Bifrost data. The same boot contract MUST
  work with production-supplied values. Rust, Python, and TypeScript journeys
  MUST prove local relative upload/download paths; the Node BFF, actual Rust
  routes, health, and MCP streaming MUST work through nginx. One-process boot
  MUST require no peer credential or calibration profile and MUST not dial
  itself for local Scribe reads. A real public Rust or Python SDK client,
  configured only with `server_url` or `WYRD_SERVER_URL`, MUST complete a
  public gRPC call through the official image's nginx listener, with no
  explicit gRPC URL; an explicit gRPC URL override MUST still work. Rust,
  Python, and TypeScript SDK contract checks MUST establish the same URL
  resolution and override behavior through their public constructors.
- **AC-035**: A fresh database MUST migrate with only the two serving roles
  and the separate migration owner, with no migrator or catalog role. Tenant
  requests MUST be RLS isolated and unable to
  read catalog metadata; platform work MUST succeed without the owner URL in
  serving environment or mounts. An unmigrated, checksum-mismatched, or
  security-grant-deficient database MUST keep serving unready. A forced
  recoverable migration failure and retry MUST preserve accepted data and
  audit evidence.
- **AC-036**: From one peer-enabled replica, start a second with the same
  deployment template and its own runtime-assigned address. The first MUST
  discover the second through existing membership without restart, and an
  analytical query MUST dispatch real remote Oracle work over mTLS. Cross-pod
  Scribe reads MUST also work. Tests MUST reject missing, expired,
  wrong-identity, and unrelated-CA peer credentials; forged or cross-tenant
  private contexts and stale fences MUST yield no tenant data. A dropped peer
  or broken remote call MUST not produce partial results. Peer mode with
  process-local file storage MUST fail before ready membership. No ticket
  replay-capacity refusal or false replay audit may remain.
- **AC-037**: A future upgrade rehearsal MUST reject incompatible peer
  overlap and stop rollout on migration failure; it MUST NOT require an
  unshipped ticket-protocol compatibility path. A documented production
  recovery point MUST exist before upgrade. This remediation MUST also ship
  and run one local kind/mise.local.toml journey using the pinned official
  image: keep the fixed `all` anchor, start one ready Oracle-only replica,
  send bounded analytical read traffic, and use a Kubernetes
  HorizontalPodAutoscaler driven by successful Oracle-executed reads per second
  on the Oracle workload to grow that workload from one to two replicas when
  the rate exceeds about 10 reads per second. HTTP ingress counts, CPU, and
  memory utilization MUST NOT be the scale trigger, and the test MUST NOT
  change the replica count directly. The autoscaler MUST be capped at two
  Oracle replicas; the traffic generator MUST have bounded rate, concurrency,
  duration, and cleanup so the local test cannot run away. Then prove the new
  Oracle replica registered its runtime address and executed a real analytical
  query dispatched over mTLS using remote Scribe data. Missing metrics,
  failure to scale, failure to join, or execution only on the anchor MUST fail
  the journey. This local proof is required for this remediation but is not a
  default CI gate. The separate two-process peer journey MUST remain runnable
  without kind.
- **AC-038**: The official image MUST boot in production without a policy
  hook. `POST /v1/authz/check` MUST be absent from the served OpenAPI and
  return the normal unknown-route response. A real Wyrd API request MUST
  still reject insufficient permission and record its decision. Existing
  delegated exchange MUST still work without `PolicyHook`, policy-only audit,
  or unevaluated `invoke` attribution.

- **AC-039**: Real Rust, Python, and TypeScript client journeys MUST prove
  direct execution for PSI, SPC, Custom, assertion Eval, and LLM-judge Eval.
  They MUST prove exact version attribution, passed and failed judgments,
  missing or legacy baselines, permission and cross-tenant refusal,
  malformed, oversized, incompatible, and unsupported input, and timeout. They
  MUST also prove that no durable run, result, dispatch, or Bifrost evidence
  operation occurs. An isolated test MUST prove that an audit-append failure
  neither refuses nor delays an authorized execution or changes a permission
  refusal.
- **AC-040**: The reference workloads are:

  | Case | Workload |
  |---|---|
  | Assertion Eval | 4 assertion tasks over a 2 KiB context |
  | Custom | 1 metric, 1,000 samples |
  | PSI | 8 numeric features × 1,000 samples, 10 quantile bins, baseline fitted from 10,000 rows |
  | SPC | 4 features × 1,000 samples, subgroup size 5, baseline from 10,000 rows |
  | LLM judge | 1 judge plus 1 assertion against a local TLS mock with a 200 ms delay |

  In `bench:capacity` (REQ-171), the four non-judge kinds MUST show
  direct-mode paired per-request engine overhead below 10 ms at p95 in every
  judged step, from at least 1,000 samples per kind in the one-replica
  sustained step. Judge cases report overhead and provider waits separately,
  with no threshold.
- **AC-041**: `bench:capacity` (REQ-171) carries 50,000 ingest rows per
  second (500 Drift observations × 100 features) at `L = 200` with default
  queue configuration. Every step at or below the knee MUST show zero
  `QUEUE_FULL` refusals and client queue drain within one second of load
  stopping. Tests MUST prove, against a real server, that every emitted
  observation is durable exactly once with 100 rows per `record_id`, and that
  client-owned bytes stay flat under sustained emission.
- **AC-042**: Queue and journey tests MUST prove:
  - all-or-none admission with resubmission and no duplicates;
  - a record larger than the budget refused as too large;
  - intake continuing during an in-flight send;
  - linger and byte sealing;
  - retention of ambiguous sends, reconciled by the same batch UUID;
  - no producer-count limit, with at least 1,000 tables written through one
    handle;
  - an idle producer reserving no budget bytes.

  Rust, Python, and TypeScript journeys MUST each prove an uninterrupted
  1,000-observation × 9-feature Drift burst with QUEUE_FULL resubmission
  after flush, durably reading back exactly 9,000 rows, 1,000 distinct
  `record_id`s, and 9 rows per id. Each SDK MUST also prove that a
  byte-budget override is honoured and that an override too small to seal a
  message is refused at connect time.
- **AC-043**: Gateway capture journeys MUST prove:
  - in a single-pod topology, capture rows land in-process in both capture
    tables, stamped with the capture principal;
  - in a peer-mode topology, a gateway served by a pod without Scribe lands
    capture through the peer RPC on a live Scribe;
  - a Scribe outage longer than the call's deadline drops that call's
    capture with a counted reason and never fails the call;
  - a resubmitted capture batch is not written twice.

  Negative tests MUST prove that Gate refuses every public write to
  `vala.gateway.calls`, and that the peer RPC refuses tables other than the
  capture and Verifier result tables, reserved tenants, and callers without a
  peer certificate.

- **AC-044**: Verifier runtime journeys under load MUST prove:
  - 200 held queued runs released at once on the 8-connection test pool all
    complete on their first attempt, with no `settlement_failed` outcome and
    no connection-acquire timeout;
  - with more than 64 tenants each holding a claimable run, every tenant
    receives a claim in the first round;
  - a run whose lease is reclaimed after its result was stored is written
    from the stored result without re-executing, and each result table holds
    exactly one copy of that run's rows;
  - a stale claimant whose lease was taken cannot store a result, and its
    work never reaches Bifrost;
  - a run that executes longer than its lease keeps its lease through
    renewal, and a renewal that finds its token gone cancels the run's work;
  - a second run of the same Verifier on one process opens no connection to
    load its Card, and a run whose Verifier was deleted settles `errored`;
  - a run refused by a full shared resource returns to the queue without
    consuming an attempt, and claiming resumes when a running run finishes.
- **AC-045**: In Rust, Python, and TypeScript journeys, `observe.verify`
  returns a passing and a failing Eval `Judgment` and a Drift `Judgment`
  through the fixture graph; an unbound Verifier name fails locally with no
  request sent; a caller without `evals:run` is refused with REQ-168's error;
  and a public Bifrost read shows no observation row for the verified input.
- **AC-046**: No SDK exports `Verification`, `wyrd.verification` does not
  import, and `mise run codegen:check` passes with the handle removed.
- **AC-047**: A static fixture Service whose `http` Operator names a
  connection and a path-only `url` delivers to the local origin the test
  registered on that connection; a path-only `url` without a connection is
  refused at registration. A fixture artifact without `sha256` and
  `size_bytes` registers, and a wrong declared digest is refused.
- **AC-048**: `TESTING.md` records the REQ-192 standard, and every
  client-facing test in the three SDKs conforms to it, checked by review
  against each bullet of REQ-192.
- **AC-049**: `cards.get(ref)` returns each of the eight REQ-193 kinds,
  typed, in all three SDKs, read back from a shared fixture, with `codegen:check` proving the Python and TypeScript
  types match the `wyrd-spec` schemas. A Python journey reads a Drift Verifier
  through `cards.get(ref)` and sees its baseline state; `observe.verify` before the
  baseline is ready is refused with `WYRD_VERIFICATION_409_BASELINE_NOT_READY`,
  and succeeds after `wait_for_baseline`.
- **AC-050**: Run views expose `alias` and no `card_ref` / `cardRef` in all
  three SDKs, and `codegen:check` passes.
- **AC-051**: (revised in revision 66) In each SDK a journey runs `apply`
  and `get` through the test-only CLI functions against the test server, in
  the test's own process, with an explicit `client`; a refused command raises
  `WyrdError` with its catalog code; and the installed `wyrd` executable runs
  from Python and TypeScript packages. The production Python wheel and the
  published `@wyrd/sdk` package expose no in-process command function, and
  no Rust journey other than the one ambient-configuration test runs in a
  child process.
- **AC-052**: Rust callers import `wyrd_sdk::WyrdError`, and no journey reads
  a refusal code out of `UpstreamFailure` details.
- **AC-053**: A journey calls the Gateway through the stock OpenAI SDK with
  a token from `access_token()` (REQ-198), and the CLI functions issue a Card-scoped key and write a
  provider credential (REQ-199).
- **AC-054**: In each SDK a parameterised `sql` returns the caller's rows,
  and a bound value containing SQL text is treated as data (REQ-200).
- **AC-055**: A new tenant's built-in tables are queryable and empty before
  any write; an existing tenant missing a built-in has it after restart
  (REQ-201).
- **AC-056**: Each REQ-202 and REQ-203 capability has a test in every SDK
  that exposes it, and `codegen:check` passes.
- **AC-057**: Every DELETE, MOVE, REWRITE, and TIGHTEN row in
  `review/sdk-test-audit/` is resolved, and every client-facing test file is
  selected by a `mise` lane (REQ-204).
- **AC-058**: In each SDK, a stock OTel exporter configured only with the
  standard `OTEL_EXPORTER_OTLP_*` settings and an `x-wyrd-api-key` header
  exports a span that is then read back through the Bifrost client
  (REQ-205).
- **AC-059**: (added in revision 66) In each SDK a journey loads a Workflow
  whose Agent and Prompt refs are registry Cards with an explicit `client`
  holding read access while the ambient credential has none, and the
  Workflow loads and runs as that client (REQ-207).
- **AC-060**: (added in revision 67) No public SDK surface outside the
  `WyrdClient` constructor accepts a server URL, credential, or gRPC URL, in
  any of the three SDKs; each REQ-208 surface takes an optional client, and a
  journey in each SDK uses every one of them with an explicit client
  (REQ-208).
- **AC-061**: (added in revision 67) In each SDK a journey creates a
  `WyrdState` with a Service's client, starts Bifrost without identity
  arguments, and verifies as that Service; a state created with a client
  lacking `evals:run` is refused with `WYRD_PERMISSION_403_DENIED_RBAC`
  (REQ-209).
- **AC-062**: (added in revision 67) A parity table under the change packet
  lists every public operation of the three SDKs with its arguments and
  result type; every row matches or names its REQ-210 idiom exception, and
  each SDK has a journey that hydrates a bundle with `cards.hydrate`
  (REQ-210).
- **AC-063**: (added in revision 67) Selectors, loaders, and deletes accept
  a `CardRef`, Python path parameters accept `pathlib.Path`, and Python
  `cards.get` results are read by attribute with `ty` type checking passing
  over the journeys (REQ-193, REQ-211).
- **AC-064**: (added in revision 67) A journey in each SDK grants a Role
  with the `grant_role` CLI function and acts with it after a fresh key
  exchange, and is refused with `WYRD_PERMISSION_403_DENIED_RBAC` when the
  caller is not a tenant administrator. A Rust route test proves tenant
  isolation, the non-enumerating not-found answer, an unknown role refused,
  idempotency, and the staged audit decision (REQ-212).
- **AC-065**: (added in revision 67) A Service key issued by `issue_key`,
  with no grant, starts Bifrost, emits, and verifies in each SDK; the same
  key is refused a tenant-wide Bifrost query until `workload` is granted
  (REQ-213).
- **AC-066**: (added in revision 67) The Python stub gate fails on a public
  callable with an undocumented parameter, and every public Python,
  TypeScript, and Rust SDK callable documents its parameters, result, and
  errors (REQ-214).
- **AC-067**: (added in revision 68) `wyrd_sdk` built without the
  `internal` feature exposes no raw-request, writer-internal, or
  introspection method; `check:deps` fails when `wyrd-sdk-rust` enables
  `internal`; every SDK names the root accessors `root_ref` and `service`
  and the Workflow step ids `steps`; no SDK exposes Workflow builders; and
  the parity table has no unexplained row (REQ-210).

## Open material decisions

Revision 70:

- None. The user approved on 2026-10-07 naming Python and TypeScript
  `QueryResult.to_arrow` as an ecosystem adapter instead of adding a Rust
  `to_arrow`.

Revision 69:

- None. The user approved on 2026-10-07 three named REQ-210 exceptions:
  language-native ecosystem adapters, Rust config types as the spelling of
  constructor options, and fixture-only test-harness CLI helpers.

Revision 68:

- None. The user approved every decision on 2026-10-07: unified names,
  the synchronous-twin and Rust programmatic-`register` exceptions, Rust-only
  types left out of `wyrd_sdk`, the `internal` feature for low-level
  methods, and removing the Python Workflow builders in favour of YAML.

Revision 67:

- None. The user approved every decision on 2026-10-07, including the
  `wyrd_default` Role name (role names allow only `[a-z0-9_]`), its narrow
  permission set, and documenting parameters in every SDK.
- Noted outside this change: `POST /v1/principals` gates on
  `service_accounts:write`, which `runtime_admin` holds, so that role can
  create a principal holding `admin`. REQ-212 does not reuse that gate. The
  existing route needs its own follow-up.

Revision 66:

- None. The user approved the revision, including the saved-login fixtures
  in REQ-195, on 2026-10-07.

Revision 65:

- None. The Bifrost storage and query decisions moved to
  `SPEC-bifrost-variant`.

Revision 39 records the user's narrow deletion: remove the always-allow
hook and its fake `invoke` policy attribution without redesigning delegation.

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
- `architecture/operations/deployment-and-release.md`
- `changes/active/verified-change-contract/review/TASK-006-r2/TASK-006-R2-continuous-eval-closure.md`
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

- **Revision 70 Arrow table adapter (2026-10-07, approved by the user):**
  - **Trigger.** The final TASK-017-R2 parity pass left one row: Python and
    TypeScript `QueryResult.to_arrow` return an Arrow table, and Rust has
    only `batches()`.
  - **Decision (REQ-210, AC-067).** arrow-rs has no table type, and a batch
    slice is its equivalent, so `to_arrow` is an ecosystem adapter under
    exception (a). Rust gains no copying `to_arrow`.
- **Revision 69 Named idiom exceptions for adapters, config, and test
  helpers (2026-10-07, approved by the user):**
  - **Trigger.** The TASK-017-R2 parity table left three surfaces matching
    no REQ-210 exception: Python pandas/polars and OpenTelemetry adapters,
    Rust's config types, and Rust's test-only `cli::load` signature.
  - **Decision (REQ-210, AC-067).** Each is an idiom, not a divergent
    contract: adapters wrap a shared result or runtime hook in the
    language's ecosystem, Rust config types carry the same constructor
    options, and fixture-loading test helpers are not user surface. All
    three are named exceptions; no SDK is changed to add or remove them.
- **Revision 68 Parity measured on the SDK packages (2026-10-07, approved
  by the user):**
  - **Trigger.** The TASK-017-R2 parity table found surfaces in one or two
    SDKs that matched no REQ-210 exception: Rust low-level client, writer,
    and bundle-inspection methods; Rust remote `Workflows` and the gateway
    caller; blocking Bifrost in Rust and Python only; Python Workflow
    builders; and different names for the root accessor and step ids.
  - **Decision (REQ-210, AC-067).** Nothing has shipped, so parity is
    enforced on the SDK packages for the long term: names are unified;
    synchronous twins and Rust/Python programmatic `register` are named
    exceptions; Rust-only types are not re-exported by `wyrd_sdk`;
    low-level methods sit behind a `wyrd-client` `internal` feature that
    `wyrd-sdk-rust` never enables; Workflows are authored as YAML only.
- **Revision 67 One client, one surface, usable roles (2026-10-07,
  approved by the user):**
  - **Trigger.** A user-persona review of the TASK-017-R1 journeys, written
    from the view of a Python data scientist, found that the story tests
    cannot be copied into a notebook. A bundle needed the test-only CLI. A
    verify key needed a harness-only Role grant. Identity arrived through
    `credential=`, `client=`, or the environment, and `verify` silently ran
    as whichever client Bifrost had started with. A cross-SDK inventory
    found three SDKs with different shapes for the same operations, and
    most public parameters undocumented.
  - **Identity (REQ-208, REQ-209).** Every server-facing surface now takes
    one optional `WyrdClient`, and a state's identity is fixed when it is
    created.
  - **Parity (REQ-210, REQ-211).** The three SDKs expose one surface, with
    named idiom exceptions only. Python gains `cards.hydrate`, typed
    attribute-access Cards (REQ-193), `CardRef` selectors, and path
    arguments.
  - **Roles (REQ-212, REQ-213).** A tenant administrator grants Roles with
    `wyrd auth grant-role`. Service principals default to `wyrd_default`,
    which can emit and verify but not read the tenant's Bifrost.
  - **Documentation (REQ-214).** Every public parameter is documented in
    every SDK.
  - **Breaking.** The removed identity arguments get no alias.
- **Revision 66 Test-only CLI functions and explicit clients (2026-10-07,
  approved):** An independent readability review of the TASK-017 rewrites found
  that every Rust journey needing a credential re-ran its own test binary as
  a child process, because the CLI functions and Workflow loading read their
  credential only from the environment and a Rust test cannot change its own
  environment soundly. The user decided that the in-process CLI functions
  are a test surface, not a product surface: they move behind each SDK's
  existing testing gate, with no new feature, and gain an optional
  `WyrdClient` that replaces `server` (REQ-196). Tests acting as a non-default
  principal pass that client instead of changing the environment, and
  ambient resolution is proved once per SDK (REQ-192). Workflow loading gains
  the same optional client as product API, because Workflows also run
  locally (REQ-207). The saved-user-login story's fixtures are sanctioned
  with the credential fixtures (REQ-195). Removing the in-process CLI from
  the production Python wheel and `@wyrd/sdk` is a breaking change with no
  alias.
- **Revision 65 Credential fixtures (2026-10-07, approved by the user):**
  Journeys need keys holding Roles (`reader`, `writer`, `agent`, another
  tenant's `admin`), and no public surface grants a Role. REQ-195 now
  sanctions the test server's credential fixtures alongside the three
  controls instead of adding a Role-granting CLI option; Card-scoped keys
  without added Roles stay on the CLI (REQ-199).
- **Revision 64 SDK verification ergonomics (2026-10-05, approved by the user
  directing planning into two tasks independent of the Bifrost work):** The
  user found the SDK verification surface and its journeys unusable: a
  separate `Verification` handle driven by raw dicts and UIDs, polling, raw
  SQL, and Card YAML built from Python strings. Real-time verification moves
  to `observe.verify(...)` on the Run view, judge-only, returning a typed
  `Judgment` (REQ-188). The SDK `Verification` handle is removed; binding,
  run start, and run status stay on HTTP and MCP (REQ-137, REQ-169, REQ-189).
  HTTP Operators may take their origin from a named connection (REQ-190) and
  artifact digests may be computed by the client (REQ-191), so fixture Cards
  are static files. REQ-192 sets the journey test standard, modelled on the
  opsml client, PromptCard, and agent-service tests. The SDK test audit
  (`review/sdk-test-audit/`) added what the journeys could not do without
  workarounds: a refusal for an unready baseline (REQ-188), a root Rust
  `WyrdError` (REQ-189), a typed `cards.get` for every kind in every SDK (REQ-193), `alias` on the Run
  view (REQ-194), three documented test-server controls (REQ-195), and the
  CLI as in-process functions in every SDK (REQ-196). The user approved the
  test standard (REQ-192), widened to every client-facing test and to how
  tests and fixtures are written, not only what they assert. Audit follow-ups:
  non-blocking emit stays (REQ-197), `access_token()` (REQ-198), key and
  credential setup through the CLI (REQ-199), bind parameters (REQ-200),
  built-in tables created with the tenant (REQ-201), typed SDK values
  (REQ-202), typed Python authoring with parity (REQ-203), and the test
  clean-up (REQ-204). Refusing unimplemented kinds at registration is
  deferred. OTLP endpoints accept API keys so stock exporters need no helper
  (REQ-205), and verification history stays SQL (REQ-206). Bifrost storage
  and query work (Variant, Iceberg v3, shredding, pruning, typed result
  summaries) moved to its own change, `SPEC-bifrost-variant`.
- **Revision 63 Observations carry their own event time (2026-10-03, approved
  on user direction):** SDK observations omitted `wyrd_event_time`, so Scribe
  stamped the batch's receipt time and a buffered observation landed in the
  window it arrived in rather than the one it happened in. REQ-187 makes
  `wyrd-client` stamp the event time when the observation is emitted, for all
  three SDKs. Scribe's acceptance window (30 days past, 24 hours future) keeps
  rows held through an outage admissible.
- **Revision 62 Late observations reach their window (2026-10-03, approved on
  user direction):** A scheduled Drift occurrence was claimed the instant
  PostgreSQL reached its window end. An observation stamped just before the end
  but still being ingested was left out of that window, and the next window
  starts at the end, so it was never counted. REQ-186 adds a fixed 30-second
  wait after the window end before the occurrence is claimed. Event time stays
  producer-owned.
- **Revision 61 Eval runs follow the writer (2026-10-03, approved on user
  direction):** The TASK-015 review found that the flusher created runs for
  every binding on the observation's subject and checked each owner's
  activity when the request was written, which could backfill another owner
  that authenticated after the observation. The user directed that an Eval
  observation written by principal A runs only bindings owned by A, and that
  the activity gate applies to scheduled and Trigger runs. REQ-108 now says so.
- **Revision 60 Eval run-request outbox (2026-10-03, approved):** Revision
  59 had Scribe insert `verifier_runs` in its batch-fence transaction. That
  breaks the repository rule that no crate's transaction writes another
  crate's tables (`architecture/v1/00-foundations/sql-foundation.md:75`,
  enforced by two `vala-sql` checks). The alternative of writing runs before
  acknowledging would make clients wait. The user chose a batched run-request
  outbox like audit's instead. The acknowledgement covers receipt only, a
  flusher writes runs in batches and retries rather than dropping, and
  graceful shutdown flushes. The user explicitly accepted losing unflushed
  requests on a hard process kill. REQ-077 and AC-014 change.

- **Revision 59 Verifier runtime under load (2026-10-03, approved):** An
  independent architecture review (Codex gpt-5.6-sol, medium) found that one
  run could write more than one result, because each retry built a new result
  and new batch IDs. It also found that Eval runs were lost when the
  post-acknowledgement handoff failed, that leases were never renewed, and
  that claim rounds stopped at 64 tenants. The user also objected to three
  more things. Internal jobs minted tokens, results looped back through the
  server's own Gate as if from an outside client, and every run reloaded its
  Verifier from Postgres. Each run took five to seven connections.
  - **Changes:**
    - Results are written by the server-internal capture writer with no
      token and no Gate (REQ-086). They are stored once before writing, and
      every write reuses those stored bytes and batch IDs (REQ-183).
    - Leases are renewed (REQ-184).
    - The claim round has no tenant limit (REQ-181).
    - Verifiers are cached per process (REQ-182).
    - A run holds a connection only for its claim, result store, settle, and
      shared lease renewal (REQ-185).
    - Eval run creation is moved off the acknowledgement path (REQ-077,
      superseded by revision 60).
    - The SYSTEM tokens are removed.
    - Revision 58's half-pool bound (the first REQ-178, which also duplicated
      the capture writer's ID) is withdrawn.
  - **Decided by the user:**
    - Verifier execution has no count cap. Shared resources that are full
      push back instead: the run returns to the queue without using an
      attempt, and claiming pauses until a run finishes.
    - Drift only runs its SQL on Bifrost and scores what returns. It makes
      no completeness or freshness check.
  - **Proof:** AC-044 and the revised AC-014 and AC-023.

- **Revision 58 runner connection bound (2026-10-03, approved):** A fairness
  test that released 200 held queued runs at once exhausted the server's
  Postgres pool. Revision 50 removed the Verifier execution permits, which
  were the only thing bounding concurrent runner Postgres work. Each run makes
  three separate connection acquisitions (Verifier load at `runner.rs:314`,
  token mint at `publisher.rs:197`, and settlement at `runner.rs:456`). The
  pool defaults to 32 connections with a 5 s acquire timeout. A timed-out mint
  consumes an attempt; a timed-out settlement strands the run until its
  10-minute lease expires; and HTTP, MCP, and audit work share the same pool.
  - **Changes:** REQ-178 bounds runner Postgres phases to half the pool and
    makes settlement retry connection timeouts. Execution remains
    permit-free.
  - **Proof:** the fairness test returns to 200 runs on the 8-connection test
    pool and fails on any `settlement_failed` outcome or any attempt beyond the
    first.

- **Revision 57 one capacity benchmark (2026-10-03, approved):** Wyrd had three
  capacity benchmarks (verification, Bifrost ingest, Bifrost query) and the
  verification one judged correctness, fairness, and tenant roles alongside
  capacity. Its tenant shape changed twice with no production basis, and
  two-replica results were hard to read.
  - **Direction:** the user chose one benchmark for the whole server that is
    easy to follow under Google SRE practice.
  - **Changes:** REQ-171 becomes `bench:capacity`: one mix of verification,
    Scribe ingest, and Oracle query over four identical tenants; ramp,
    sustained, and two-replica scale-out steps; golden-signal SLIs with
    stated SLOs and PASS/FAIL per cell. It replaces
    `bench:verification:capacity` and `bench:bifrost:ingest-capacity`.
    AC-040 and AC-041 become SLOs of that run; fairness, exactly-once claims,
    judgment correctness, and queue durability move to tests.
    `bench:bifrost:query-capacity` is already deleted by
    `opitimization-and-benchmarks` REQ-010; its ClickBench and observability
    benchmarks compare the storage engine externally and are not capacity
    benchmarks.

- **Revision 56 production-shaped capacity benchmark (2026-10-02, approved):**
  The REQ-171 defaults expanded to 134 steps of 30 seconds: 2 paths × 6
  cases × 5 rates on each of two replica counts, plus warmup, combined, and
  fairness steps. A run took over an hour, and no person could diagnose its
  report. The 10 and 25 per second steps could never yield AC-040's 1,000
  samples per case.
  - **Direction:** the user asked for a simple, human-readable benchmark
    that tests production behavior the way standard capacity tests do, in
    30 minutes or less.
  - **Changes:** REQ-171 now runs one production mix through a warmup, a
    four-rate ramp, and sustained steps on one and two replicas, with a
    30-minute ceiling. Per-kind and per-path figures become breakdowns of
    each step. AC-040 measures overhead in the one-replica sustained step
    instead of solo and mixed ladders.
  - **Proof:** AC-040 keeps the reference workloads, the 10 ms p95 bound,
    and the 1,000-sample floor.

- **Revision 55 linger sealing under backpressure (2026-10-02, approved):**
  The AC-041 benchmark showed the producer sealing a ~350-row batch on every
  5 ms linger even with every send slot busy. Sealed batches never merge, so
  the batch rate was fixed by the linger, the sink's
  `max_in_flight / ack latency` capacity capped throughput near 33k rows/s,
  client bytes grew, and drain took seconds.
  - **Direction:** the user approved sealing an elapsed linger only when a
    send slot is free.
  - **Changes:** REQ-174's linger trigger now requires a free send slot; the
    frame-target trigger, flush, and shutdown seal as before.
  - **Proof:** AC-041 is unchanged.

- **Revision 54 gateway capture writer (2026-10-02, approved):** Gateway
  capture minted a tenant token per tenant and wrote through one embedded
  client per tenant over the server's own public gRPC listener, with a
  64-tenant ceiling and a preallocated per-tenant budget.
  - **Direction:** the user directed that capture behave like Bifrost's own
    engines: in-process when Scribe is in the pod, otherwise over the gRPC
    peer plane, through one writer.
  - **Changes:** REQ-178 to REQ-180 and INV-020 define the topology-routed
    writer, the capture-only peer RPC, server-internal identity, and
    deadline-bounded delivery. AC-043 makes both paths and the refusals
    acceptance evidence.
  - **Decided:** the peer RPC is a new capture-only service, not a general
    internal-write RPC. Delivery is acknowledged before the call's deadline or
    a counted drop; no backlog survives the deadline. The user approved these
    decisions and the requirement text in conversation.

- **Revision 53 client ingestion throughput and memory (2026-10-02,
  approved):** The capacity benchmark's 1,000-observation Drift seed failed with
  `WYRD_CLIENT_429_QUEUE_FULL`.
  - **Causes:**
    - Intake awaited each network send.
    - Admission was row by row, so a refusal could split one observation.
    - The size trigger was unreachable.
    - A 32 MiB budget was pre-divided across 64 potential producers, leaving
      about 465 staging rows per table.
    - Ambiguous gRPC exhaustion was settled as a loss.
  - **New requirement:** the user requires 500 requests/s × 100 features
    (50,000 rows/s) and directs that no cap exists without justification and
    that nothing is preallocated.
  - **Changes:** REQ-172 to REQ-177 adopt the shared-budget producer model
    from Kafka and librdkafka.
  - **Proof:** AC-041 and AC-042 make sustained throughput and memory
    behaviour acceptance evidence.
  - **Decided:** the default budget is 256 MiB, overridable in every
    first-class SDK. Batches carry no delivery-order guarantee. No blocking
    admission API is added.

- **Revision 52 non-blocking direct-execution audit (2026-10-02):** The user
  directed that permissions are blocking and audits are non-blocking.
  - REQ-168 stages the direct-execution decision on the shared audit outbox
    instead of committing it transactionally; an audit failure no longer
    refuses the request.
  - AC-039's isolated audit test proves that non-blocking behavior.
  - Converting the remaining transactional audit surfaces is a separate
    change.
- **Revision 51 direct execution, telemetry, and capacity (2026-10-02):** The
  user approved the TASK-008 closeout amendments.
  - REQ-135 gains a fourth operation.
  - REQ-167 to REQ-169 define synchronous supplied-input execution, its
    authorization, bounds, errors, and SDK/MCP projections.
  - REQ-170 carries the caller-aligned telemetry contract.
  - REQ-171 defines the capacity benchmark.
  - AC-039 and AC-040 fix direct-execution proof, the reference workloads,
    and the strict latency proof.

  This revision supersedes the recovery task's fixed traffic profile and its
  prohibition on synchronous execution.

- **Revision 50 remove Verifier execution caps (2026-10-02):** Explicit caller
  instruction removes the 16-process/4-tenant permits from Verifier runs and
  baseline fitting directly, without an experimental candidate or replacement
  admission layer. Operator delivery permits, durable claim/lease fencing,
  execution/publication deadlines and shutdown behavior remain unchanged.

- **Revision 49 default workload role (2026-10-02):** A Card-bound Service or
  Agent principal now receives the new built-in `workload` role at first
  projection, in the registration transaction. The role holds
  `bifrost_table:read`, `bifrost_record:write`, and `bifrost_query:read`; table
  read is the describe every writer performs before admission. Card scope still
  bounds emission, re-registration never re-grants a revoked role, and a
  migration seeds the role row for existing tenants. This replaces the
  direct-SQL role grant benchmarks and journeys used because no public route
  grants a role to a Card-bound principal. The user explicitly approved
  revision 49 on 2026-10-02.

- **Revision 48 verification table schema erratum (2026-10-02):** Removed the
  `wyrd_row_ordinal` and `data_tenant_id` rows from the shared managed columns
  in `architecture/logic/table_schema.md`; Bifrost writes neither column.
  Tenant scope for the five verification tables comes from the tenant-bound
  query and the physical tenant table, not a stored row column. AC-012, AC-014,
  and AC-024 now join results to details by `result_id` within the caller's
  tenant-scoped query. No behavior changes. The user explicitly approved
  revision 48 on 2026-10-02.

- **Revision 47 marker-based Run correlation registration (2026-10-02):**
  Simplified Python Run OpenTelemetry registration in REQ-151, AC-032, and
  `architecture/logic/run_api.md` to three guarantees: no processor pile-up
  (a private marker attribute on the provider object makes later entries skip
  it; a duplicate from a concurrent first entry is harmless because the
  processor is stateless and enrichment idempotent), never break the app
  (every optional-telemetry failure is swallowed), and correct stamping from
  the context-held scope stack. Removed the at-most-once-per-provider attempt,
  cached outcomes, the no-enrichment rule for accept-then-raise providers, the
  strict single-processor guarantee, identity-versus-equality caching, and the
  registration lock with inert processors. This supersedes
  `TASK-009-R6-reentrant-provider-registration`. The user explicitly approved
  revision 47 on 2026-10-02 and waived further review.

- **Revision 46 token-free Python Run correlation (2026-10-01):** Resolved
  `FIND-TASK-009-8` by keeping the Python Wyrd scope stack inside the
  OpenTelemetry context value (one private key minted at import, a tuple of
  `(card_ref, run_id)` pairs, innermost last). Entry pushes and exit pops by a
  single context attach each; detach tokens and detach calls are removed, and
  a mismatched or failed exit changes nothing. Replaced "attach/detach
  failure" with "context-update failure" in REQ-151 and "detach failure" with
  "exit context-update failure" in AC-032; exiting restores the prior Wyrd
  correlation in the current context. Registration is attempted at most once
  per provider with a cached outcome, so a provider that raises after
  accepting the processor never receives a second one. New behavior: entry
  stamps the already-active recording span only when it does not already
  carry `wyrd.card_ref`; spans created inside the scope are still stamped by
  the processor with the innermost scope's pair. Rationale is recorded in
  `architecture/logic/run_api.md`. The user explicitly
  approved revision 46 on 2026-10-01.

- **Revision 45 Operator key source correction (2026-10-01):** Replaced the
  nonexistent shared external-secret resolver requirement with server-owned
  Operator key selection: Vault KV v2 for multi-tenant production, environment
  keys for development, and restrictive files for explicitly single-tenant
  deployments. Tenant administrators continue to submit write-only delivery
  credentials to Wyrd's encrypted connection store. The user directed this
  correction on 2026-10-01 and explicitly approved revision 45 on 2026-10-01.

- **Revision 38 direct Drift input boundary (2026-09-24):** Approved by the
  user after the TASK-011 review. Direct PSI/SPC batches contain only selected
  observations; every row participates in completeness checks. Server
  selection continues to use series identity, including present-null values.
  The revision removes the impossible requirement to infer selected-row
  membership from a wide batch's null values. It retains revision 37's
  statistical formulas, fixed subgrouping, and explicit null-to-inconclusive
  policy; no missing PSI bin, new observation format, or statistical method is
  introduced. The remaining TASK-011 review findings still require remediation.

- **Revision 37 conventional PSI/SPC (2026-09-24):** Approved complete
  categorical PSI bins, strict missing-data handling, fixed rational
  subgroups, NIST three-sigma X-bar/S limits, chart-specific result evidence,
  and an explicit new-version/refit boundary. This reverses revision 32's
  SPC preservation decision and was explicitly approved by the user on
  2026-09-24. Statistical
  grounding: [NIST X-bar/S chart](https://itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm),
  [NIST rational subgroups](https://www.itl.nist.gov/div898/handbook/glossary.htm),
  and [ASQ control-chart setup](https://asq.org/quality-resources/control-chart).

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
- **Revision 36 SYSTEM Drift read (2026-09-24):** Resolved TASK-005 r1
  `FIND-TASK-005-6`. Added a second, mutually exclusive SYSTEM token purpose:
  a short-lived `bifrost_query:read` token scoped to the tenant's registered
  `vala.drift.observations` table UID, resolved without creating the table.
  Drift verifies it through the ordinary verifier and dispatches server-built
  fixed SQL through the ordinary query service, Gate, and local or
  peer-forwarded Oracle, replacing the local typed `query_plan` seam and the
  hand-built SYSTEM principal. Stated that the token does not enforce
  subject, series, or window limits. This revision was directed by the user
  on 2026-09-24.
- **Revision 36 SYSTEM Eval input reads (2026-09-23):** Resolved
  FIND-TASK-006-7. Continuous Eval's observation and trace reads use the
  existing persisted per-tenant SYSTEM principal with a narrow server-minted
  `bifrost_query:read` authority scoped to exactly `vala.eval.observations` and
  `vala.traces.spans`, kept separate from its exact-Verifier result-write
  token, instead of a fabricated user. Missing identity, wrong tenant, and
  insufficient scope fail closed before rows are returned. This revision was
  explicitly approved by the user on 2026-09-23: "That is an anti-pattern why
  are you creating a new user. If this is a server runtime/machinary that is
  concstantly running and by the nature of its design, doesnt have a direct
  principal, then it needs to use a system principal (per tenant)".
- **Revision 37 TASK-006 server startup expansion (2026-09-24):** Added
  the image-to-client first-use journey, external Postgres and one-off
  migration with two serving logins and direct cleanup of unshipped role
  migrations, in-process local engine calls, shared-cert peer mTLS with
  receiver-side tenant and fence checks, existing Postgres discovery,
  multi-replica shared object storage, public nginx/BFF routing, relative local
  storage URLs, and removal of the unshipped peer-ticket protocol.
  This revision was explicitly approved by the user on 2026-09-24: "I approve
  the spec. revise the packet".
- **Revision 38 external authz removal and public gRPC draft (2026-09-24):**
  At the user's direction, removed the unearned `/v1/authz/check` and
  `PolicyHook` surfaces from the target design while retaining delegated
  token-exchange identity, permission intersection, tenancy, and truthful
  audit boundaries. Made
  one effective `server_url` sufficient for `WyrdClient` to reach HTTP and
  the image's public gRPC port through nginx; the existing gRPC URL remains
  an optional override. The user approved the revised contract on 2026-09-24
  through "ok go ahead" and the explicit delegation-rule follow-up "agree".
- **Revision 39 delegation correction (2026-09-24):** Removed revision 38's
  invented delegation rule and extra journey. The change deletes the
  always-allow `PolicyHook`, its policy-only branches, and false `invoke`
  attribution while otherwise preserving token exchange. The user directed
  this correction after clarifying that delegation was already settled.
- **Revision 40 Kubernetes autoscaling proof (2026-09-24):** Made the
  previously optional kind journey a required local proof for this remediation.
  Fixed the per-pod Downward API address injection and required a
  read-requests-per-second-driven HorizontalPodAutoscaler scale event, peer
  registration, and remote Oracle work. The user explicitly required
  autoscaling simulation with bounded local resource use.
- **Revision 41 pre-release image proof (2026-09-25):** For TASK-006 acceptance,
  the official-recipe image built from the reviewed commit is pinned by its
  immutable local image ID for startup, kind, and production-profile journeys.
  The first release repeats those journeys against the published image pinned
  by an immutable registry digest. The user explicitly approved this decision
  on 2026-09-25 after confirming no image has yet been published.
- **Revision 42 Oracle autoscaling proof (2026-09-25):** Corrected the original
  kind journey to scale its Oracle read tier from one to two replicas on
  successful Oracle-executed reads per second. The prior Scribe HTTP-ingress
  scale result does not satisfy AC-037. The production Kubernetes example
  follows the same Oracle scaling model. The user explicitly directed this
  correction and rejected adding a second kind test.
- **Revision 43 edge-only public TLS (2026-09-25):** Removed the server's
  public gRPC certificate inputs (`WYRD_GRPC_CERTIFICATE_CHAIN_FILE`,
  `WYRD_GRPC_PRIVATE_KEY_FILE`). The official image's nginx forwards public
  gRPC to the server in plaintext, so those inputs could only break it, and the
  production rule requiring them whenever peer mode was on prevented any
  production peer deployment from starting. Public TLS terminates at the edge;
  peer mTLS is unchanged. The user explicitly approved this deletion and the
  spec update on 2026-09-25.
- **Revision 44 published release profile (2026-09-25):** Added a workspace
  `dist` Cargo profile and required it for the published server binary that
  the release workflow packages into the image. The workspace previously had
  no release tuning, so published images shipped a default `release` build.
  Local `docker:build` and test lanes keep `release` so iteration stays fast.
  The user explicitly directed this addition and the spec update on
  2026-09-25.
- **Integration of parallel revision lines (2026-09-29):** Revisions 36–38
  were approved on two parallel lines. The Drift line recorded the SYSTEM
  Drift read token (36), conventional PSI/SPC (37), and the direct Drift input
  boundary (38); the continuous Eval line recorded SYSTEM Eval input reads (36)
  through the published release profile (44). This packet carries both lines'
  approved content unchanged at revision 44; no requirement was redefined.
