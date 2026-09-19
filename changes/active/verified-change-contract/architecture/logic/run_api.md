# Run API for continuous verification

**Status:** Approved client interface through specification revision 32.

## User contract

`WyrdState.from_path` loads a hydrated Service graph locally. The caller then
starts its Bifrost client with `state.start_bifrost(...)`; the optional arguments
pass through the corresponding language SDK's existing Bifrost constructor,
including its environment/default resolution. `start_bifrost` connects and
describes the fixed Drift and Eval system tables. The state owns that one
`wyrd_client::Bifrost` facade and shuts down all of its table producers; it
does not construct a second queue or transport. A caller opens one run for an
application invocation without passing a Bifrost client or table to `run()`.
The run initially targets the root Service Card.
`for_card` / `forCard` selects a registered alias in that same hydrated graph
and returns an immutable Card-scoped view; it does not mutate the parent view.
The views share the invocation's `run_id`. Every observation carries its
view's exact subject `card_ref` and that `run_id` as Bifrost row correlation.
The caller never supplies a Verifier reference, Card UID, record ID, or
timestamp for the ordinary `observe.drift` / `observe.eval` calls. The server
authorizes the asserted subject against the authenticated principal's signed
Card scope and resolves its managed `card_uid`.

Opening a run and selecting a Card do not perform network IO, register a
server-side Run resource, or execute a Verifier. Drift and Eval observations
only project and enqueue. The first `observe.record(table, value)` for a
dynamic table may describe that table before enqueue. An Eval observation can later activate
each matching `observations_ready` binding; Drift observations become input
for scheduled or manual, windowed analysis. The two observation calls do not
return a score or alert.

### Python

```python
from dataclasses import dataclass
from wyrd import WyrdState
from wyrd.eval import MediaRef as EvalMediaRef

@dataclass
class ChurnFeatures:
    age: int
    plan: str
    score: float

@dataclass
class SupportExchange:
    question: str
    answer: str

state = WyrdState.from_path("./service-bundle")
state.start_bifrost()
# Or, with the same arguments as Bifrost(...):
# state.start_bifrost(table=None, server_url=server_url,
#                     credential=credential, grpc_url=grpc_url)
run = state.run()

model = run.for_card("churn_model")
model.observe.drift(ChurnFeatures(age=42, plan="premium", score=0.82))
model.observe.drift({"age": 43, "plan": "basic", "score": 0.37})

agent = run.for_card("support_agent")
agent.observe.eval(
    SupportExchange(question=question, answer=answer),
    session_id=session_id,
    media=[EvalMediaRef(id="screenshot", kind="image",
                        uri=media_uri, media_type="image/png")],
)

run.observe.record("app.events", {"event": "request_completed"})
agent.observe.record("app.agent_events", {"step": "answered"})

# At graceful application shutdown, not after each observation:
state.shutdown()
```

Python accepts a mapping, dataclass instance, or Pydantic model instance.
Dataclass support is new; the current generic Bifrost row helper does not
provide it. For Pydantic, call `model_dump_json()` and pass its JSON text
directly to the Rust boundary; do not first call `model_dump()` or serialize
it again with `json.dumps()`. For a mapping, validate that its keys are
strings and serialize it with `json.dumps(..., allow_nan=False)`. For a
dataclass instance, apply `dataclasses.asdict()`, validate its top-level keys,
then use the same strict `json.dumps()` path. These checks require no Pydantic
runtime dependency in the SDK. `session_id` is optional and belongs to an
emission, not the run. The Python `drift(...)` / `eval(...)` boundary owns
conversion from that JSON payload to the existing typed record and fixed
table rows before inserting them through the Bifrost facade owned by
`WyrdState`.

### TypeScript

```ts
import { WyrdState, type EvalMediaRef } from "@wyrd/sdk";

type ChurnFeatures = { age: number; plan: string; score: number };
type SupportExchange = { question: string; answer: string };

const state = WyrdState.fromPath("./service-bundle");
await state.startBifrost();
// Or: await state.startBifrost({ table, serverUrl, credential, grpcUrl });
const run = state.run();

const model = run.forCard("churn_model");
const features: ChurnFeatures = { age: 42, plan: "premium", score: 0.82 };
model.observe.drift(features);

const agent = run.forCard("support_agent");
const context: SupportExchange = { question, answer };
const media: EvalMediaRef[] = [
  { id: "screenshot", kind: "image", uri: mediaUri, mediaType: "image/png" },
];
agent.observe.eval(context, { sessionId, media });

await run.observe.record("app.events", { event: "request_completed" });
await agent.observe.record("app.agent_events", { step: "answered" });

// At graceful application shutdown, not after each observation:
await state.shutdown();
```

TypeScript accepts a plain serializable object. A TypeScript type is erased at
runtime; the boundary still validates its values. Numeric values must be
finite and safely representable. Unsupported values must fail rather than be
silently omitted or converted to `null` by `JSON.stringify`.
The TypeScript `drift(...)` / `eval(...)` call validates and serializes its
plain object at the SDK boundary, then completes typed-record and fixed-row
projection within that call before inserting through the state-owned Bifrost
facade.

### Rust

```rust
use serde::Serialize;
use wyrd_sdk::{eval::{EvalObservationOptions, MediaKind, MediaRef}, state::WyrdState};

#[derive(Serialize)]
struct ChurnFeatures<'a> {
    age: i64,
    plan: &'a str,
    score: f64,
}

#[derive(Serialize)]
struct SupportExchange<'a> {
    question: &'a str,
    answer: &'a str,
}

#[derive(Serialize)]
struct Event<'a> {
    event: &'a str,
}

let mut state = WyrdState::from_path("./service-bundle".as_ref())?;
state.start_bifrost().await?;
// Configured equivalent: state.start_bifrost_with_config(
//     &client, Some(table), queue_config,
// ).await?; // mirrors Bifrost::connect_with_config
let run = state.run();

let model = run.for_card("churn_model")?;
model.observe().drift(
    &ChurnFeatures { age: 42, plan: "premium", score: 0.82 },
    None,
)?;

let agent = run.for_card("support_agent")?;
agent.observe().eval(
    &SupportExchange { question, answer: &answer },
    EvalObservationOptions {
        session_id: Some(session_id),
        media: vec![MediaRef {
            id: "screenshot".into(),
            kind: MediaKind::Image,
            uri: media_uri,
            media_type: Some("image/png".into()),
        }],
        ..Default::default()
    },
)?;

run.observe()
    .record("app.events", &Event { event: "request_completed" })
    .await?;

// At graceful application shutdown, not after each observation:
state.shutdown().await?;
```

Rust accepts any `Serialize` value satisfying the same Drift or Eval input
contract. The SDK-only `EvalObservationOptions` groups optional per-emission
session, media, and explicit trace/span IDs; Python keyword arguments and
TypeScript options expose the same fields idiomatically. This is not a
second durable Eval observation type.
Python and TypeScript expose `observe` as a member of the scoped run; Rust
uses the idiomatic `observe()` accessor. The Rust `drift(...)` / `eval(...)`
call converts the `Serialize` input to the
existing typed observation and fixed rows before inserting through the
state-owned Bifrost facade. Rust's async startup and shutdown use the existing
async client; a synchronous Rust entry point may use the existing blocking
Bifrost boundary outside an async runtime.

## Input and queue boundary

The shared insertion flow is:

```text
WyrdState owns one Bifrost client
  + caches {table name → described user schema}
  + describes and caches Eval and Drift tables at start_bifrost
  + uses the existing explicit-table WriterPool::insert path

observe.eval(wrapper)
  → build EvalRecordObservation from the public wrapper
  → project its fixed table row to JSON
  → insert(eval table, cached schema, JSON, Correlation{card_ref, run_id})

observe.drift(object)
  → language boundary converts the object to JSON (Rust may project directly)
  → Rust validates the existing feature map and builds DriftRecordObservation
  → project one fixed table row per feature; serialize each row to JSON
  → insert(drift table, cached schema, each row, same Correlation)

observe.record(table, object)
  → lazily describe and cache the registered table
  → serialize its row to JSON
  → insert(table, cached schema, JSON, Correlation{card_ref, run_id})
```

These are three authoring projections into the same existing Bifrost insert,
queue, Arrow IPC, Gate, and Scribe path. Neither `use_table_by_name` nor
`use_table` runs per observation, and `card_ref`/`run_id` are correlation,
not fields in the user-row JSON.

The public arguments are authoring conveniences, not new durable observation
types or table schemas. The `observe.drift(...)` and `observe.eval(...)` calls
own their respective projections **before** queue insertion. `WyrdState`
owns a Bifrost facade; Bifrost owns schema resolution, the existing pooled
queue, and transport. Shared `wyrd-client` and `wyrd-queue` remain
Verifier-kind-agnostic plumbing. In each first-class SDK:

1. Each foreign-language boundary supplies one JSON value; Rust `Serialize`
   reaches the same logical value. Python Pydantic uses its own
   `model_dump_json()` output as that value; Python mappings and dataclasses
   use the strict standard-library JSON path above. No user-defined class,
   interface, or struct defines a Bifrost table schema.
2. Inside `drift(...)`, the JSON root must be a flat object. The Drift
   observation boundary deserializes it directly into the existing
   `BTreeMap<FeatureName, FeatureValue>`—not a new `FeatureMap` type or a
   handwritten per-key coercion loop. `FeatureName` validates every key;
   accepted `FeatureValue` scalars are boolean, signed 64-bit integer, finite
   float, or string. Null, arrays, nested objects, invalid feature names,
   and numbers that cannot satisfy the agreed Float64 projection fail before
   queue admission. Eval instead requires a JSON-serializable `context`
   value for its authored task paths.
3. Inside the respective `drift(...)` or `eval(...)` call, the observation
   boundary constructs the existing `DriftRecordObservation` or
   `EvalRecordObservation` with a client-generated record ID, optional
   emission session, and emission time. Remove the old `run_id` field from
   both canonical records: the scoped run supplies `run_id` and `card_ref`
   together in Bifrost `Correlation`, not as user-writable feature, context,
   or table-row fields. Existing optional Eval trace/span/media data
   remains part of the canonical Eval record; normal trace context is captured
   by the SDK when available, and advanced emission must preserve those fields.
4. `drift(...)` projects one canonical Drift observation into one
   fixed-schema `vala.drift.observations` row per `features` entry. Every row
   repeats its logical `record_id`, optional `session_id`, and `created_at`;
   `series` is the validated feature name. `Cat` and `Bool` populate
   `str_value` only (`Bool` uses `"true"` or `"false"`). `Int` and `Float`
   populate `num_value: Float64` and a canonical `str_value` so a fitted
   categorical PSI baseline can use an originally numeric feature. Integer
   conversion must be exact and float values finite. String conversion must
   match the registered Data Card baseline fitter. Neither the logical
   feature map nor a four-value-column projection enters the queue.
5. `drift(...)` or `eval(...)` serializes each projected flat table row to
   JSON bytes and submits those bytes with its explicit destination, cached
   user schema, and the same subject CardRef/run ID correlation through the
   state-owned Bifrost facade's existing `WriterPool::insert` path. The caller
   supplies no schema. The queue adds correlation columns rather than
   receiving them in the user-row JSON or user table schema.
   `wyrd-queue` parses those scalar rows against that schema, builds an Arrow
   `RecordBatch`, serializes Arrow IPC, and publishes
   through the existing Gate/Scribe path. Gate admits the authorized table
   write; Scribe checks the asserted subject against the signed Card scope,
   resolves its Card UID, and stamps tenant and publisher principal;
   no Verifier/binding identity or per-binding copy is written to raw input.

The Drift physical projection is fixed by
[drift.md](drift.md#bifrost-observation-schema-and-projection): one tall table
with `record_id`, `series`, nullable `num_value`, nullable `str_value`,
optional `session_id`, `created_at`, and Bifrost's managed columns. The Eval
observation projection is fixed by [table_schema.md](table_schema.md), including
nullable `FixedSizeBinary(16)` trace IDs and `FixedSizeBinary(8)` span IDs,
which must match `vala.traces.spans`. The existing schema-driven
JSON-row builder must decode canonical trace/span hex into those bytes and
reject malformed or wrong-width values; no Eval-specific queue or publisher
is added.
The current queue admits projected rows one at a time and its existing drop
telemetry applies; do not claim that one multi-feature observation is an
atomic queue admission or that queue admission is a durable Scribe ACK.

## Bifrost lifecycle and table schemas

- Python `start_bifrost(table=None, server_url=None, credential=None,
  grpc_url=None)` is synchronous and passes the four positional/keyword
  arguments directly to `Bifrost(...)`. TypeScript
  `await startBifrost(options = {})` passes `{ table?, serverUrl?, credential?,
  grpcUrl? }` directly to `Bifrost.connect(options)`. Rust's async
  `start_bifrost()` uses `Bifrost::from_env()`; its configured form takes the
  same client, optional table, and queue configuration as
  `Bifrost::connect_with_config`. No new configuration object, credential
  rule, or constructor vocabulary is introduced. Omitted options retain
  existing environment/default behavior. Python `shutdown()` is synchronous;
  TypeScript and the async Rust surface await shutdown.
- A state starts Bifrost once. Repeating `start_bifrost` / `startBifrost`
  returns an already-started error, even if the caller supplies the same
  options. `shutdown()` drains that writer. After an ambiguous shutdown
  failure, retry `shutdown()` on the same state and handle; never replace the
  writer while it may retain pending batches. A successfully shut-down state
  stays closed to Bifrost writes. Create a new `WyrdState` to start again.
- Startup connects one Bifrost facade per state and describes both fixed
  `vala.drift.observations` and `vala.eval.observations` system tables. Missing,
  unauthorized, or incompatible fixed tables fail startup, before a run can
  emit. Bifrost stores each resolved table name and its **user schema** for
  this connected writer's lifetime; neither the SDK nor `WyrdState` describes
  them per observation. Fixed-table preflight makes `drift(...)` and
  `eval(...)` synchronous projection-and-enqueue calls in all three SDKs.
  The cache contains table names and described user schemas only. The
  `WriterPool` already owns per-table producer reuse; observations do not
  create or cache another producer.
  The `table` constructor option retains its existing Bifrost meaning; it
  does not choose a run's destination or change the fixed system tables.
  These explicit describe requests may use the existing card-bound token
  exchange and activate the exact principal; an idle state does not refresh.
  Each describe requires the existing `bifrost_table:read` permission and is
  audited at the server boundary that evaluates it.
- `observe.record(table, value)` is the generic user-table call on either the
  root run or a Card-scoped view. The table name is required on that call,
  never on `state.run()`. The input is a language-native serializable, flat
  row representable by the existing JSON-to-Arrow queue path; tables with
  unsupported binary or nested columns retain Bifrost's explicit Arrow
  `write_batch` path. The caller does not supply an Arrow schema,
  `TableConfig`, Bifrost handle,
  or correlation. The first call for a table describes that already
  registered table through Bifrost, caches its name and user schema, then
  enqueues; subsequent calls use the cached schema and producer without a
  describe request. Concurrent first calls for the same table must converge
  on one schema and producer. An unknown, unauthorized, or unavailable table
  fails before queue admission. Python `record(...)` is synchronous and may
  block for that first describe; TypeScript and Rust `record(...)` are async
  because their first call may await it. Subsequent calls retain the same
  language-level signature but perform no schema network lookup. Describing
  the table proves that it exists and is accessible; generic row values are
  checked against the schema when the queue seals a batch, and the server
  checks the registered schema fingerprint. A returned `record(...)` call
  is not a durability or whole-row-validation acknowledgement.
- Bifrost routes each submission by its explicit table name and cached user
  schema through the existing explicit-table `WriterPool::insert` path. It
  does **not** call `use_table_by_name` or `use_table` from an observation:
  `use_table_by_name` performs a remote describe and changes the Bifrost
  handle's shared active table, so repeated calls are neither a cache nor
  safe routing for concurrent run views. Its existing pool creates one
  producer on first insert per client scope and table; Model, Agent, and run
  identities travel per row and do not create producers or connections. All
  table producers share that facade's sink, transport, and bounded byte
  budget. One schema is authoritative per table name for the writer's
  lifetime. A second incompatible schema cannot silently replace a live
  producer's schema; a server-side schema change requires a new writer after
  shutdown and may otherwise receive a schema-fingerprint refusal.
- `observe.record` uses ordinary user-table permissions and cannot write
  reserved or system-managed tables. The dedicated Drift and Eval calls use
  their fixed system-table path. All three admission paths require the existing
  `bifrost_record:write` permission plus signed Card scope over the exact
  observed subject, and Gate audits the allow or deny decision. A generic
  record does not by itself create a Verifier run or select a Verifier binding.

The same explicit-table insert path handles all three observe methods.
`observe.record(table, value)` serializes one caller row after its lazy
describe; `observe.eval(...)` projects the canonical Eval record into its
fixed row and serializes it; `observe.drift(...)` first parses a foreign
language's JSON input into the existing validated feature map, then projects
and serializes one fixed tall row per feature. The second Drift JSON encoding
is the table-row output, not another serialization of the original object.
Rust may project directly from its typed input. Eval JSON-valued context and
media are encoded into the scalar table fields required by the described
schema before the outer row is serialized. No SDK defines a table schema.

The client-to-server path is: scoped call -> input validation and, for Drift
or Eval, canonical-record/fixed-row projection -> Bifrost table/schema lookup
-> one row at a time into the existing bounded producer -> Arrow batch and IPC
-> authenticated gRPC Gate table admission -> Scribe per-row subject-scope
authorization and server-stamped tenant, publisher, and subject identity -> Scribe schema
fingerprint check, batch dedup, WAL durability, and acknowledgement -> one
tenant-qualified Bifrost table. A committed Eval input can then activate
matching `observations_ready` bindings; Drift analysis later queries the
windowed input. The first dynamic `record` describe is table metadata IO,
not a durable ingest acknowledgement.

`observe.drift` and `observe.eval` do only projection and queue insertion;
they enqueue without a per-call `flush()` or server-analysis wait. After
its possible first-use describe, `observe.record` has the same buffered
publication semantics. The
existing bounded queue publishes in the background.
It is in-memory: queue admission is not a durability acknowledgement, and an
abrupt process exit before Scribe acknowledgement can lose pending rows.
Graceful `WyrdState.shutdown()` drains every producer of its owned Bifrost
facade across all tables; tests or finite jobs may use `flush()` as an
explicit durability barrier. The existing observation
admission/drop policy and drop telemetry apply; no second queue, projector,
or transport is introduced.

## Verification control-plane API

The observation methods above only enqueue inputs. Status and manual Drift
analysis use three typed HTTP operations, projected through the same
`wyrd-client` Verification capability in Rust, Python, and TypeScript:
`get_binding` / `getBinding`, `start_run` / `startRun`, and `get_run` /
`getRun`. They do not require a second client-side analysis engine.

| HTTP operation | Returns |
|---|---|
| `GET /v1/verification/bindings/{binding_id}` | Exact owner/subject/Verifier identities; active gate; readiness; nullable next schedule, last activation, and last run. |
| `POST /v1/verification/runs` | `202 { run_id }` after durable enqueue; no synchronous score. |
| `GET /v1/verification/runs/{run_id}` | Execution status, nullable manual `requested_by_principal_id`, nullable `result_id`, execution error, and independent Operator dispatch/delivery statuses. |

Manual run input is one tagged `binding { binding_id }` or direct
`verifier { verifier_uid, subject_card_uid }` target plus a bounded
`drift_window { start, end }` in UTC (`[start, end)`). A binding run follows
its `on_failure` Operators; a direct run is analysis-only. The existing
`Idempotency-Key` contract makes a retried POST refer to the same request.
Eval remains observation-triggered in this change. Both manual targets require
an authenticated credential and persist that caller as
`requested_by_principal_id`. That principal records who requested the run; it
does not become a binding owner Card.

```json
{
  "target": { "kind": "binding", "binding_id": "..." },
  "input": { "kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": "2026-09-17T01:00:00Z" }
}
```

The existing `GET /v1/cards/by-uid/{kind}/{card_uid}` returns PSI/SPC baseline
status on the Verifier Card and derived binding IDs on the owner Card through
server-derived `card.status.verification`. The result summary and details are
queried by `result_id` through existing Bifrost query APIs, not a second
result endpoint. MCP projects the same operations as `cards.get`,
`verification.get_binding`, `verification.start_run`, and
`verification.get_run`; existing `bifrost.query` reads the result rows.
MCP manual invocation requires explicit write scope.

## Eval observation and media journey

`observe.eval(context, options)` is the public authoring wrapper; it is not a
second durable observation type. Python accepts keyword options, TypeScript
an options object, and Rust the SDK-only `EvalObservationOptions`. All three
project into the existing `EvalRecordObservation` inside `eval(...)` before
queue insertion:

| Public input | Canonical record behavior |
|---|---|
| `context` (required) | JSON-serializable Eval task context |
| `session_id` (optional) | Copied to the one emitted record |
| `media` (optional list) | Existing Eval `MediaRef` items, with binding `id`, supported `kind`, URI, and optional MIME type |
| `trace_id`, `span_id` (optional) | Explicit values win; otherwise use valid active OpenTelemetry span IDs when available, never process environment variables |

The call generates `record_id` and `created_at`, takes `run_id` and subject
CardRef from the immutable scoped run, and passes both as Bifrost row
correlation. `run_id` and `eval_ref` are deleted from
`EvalRecordObservation`, not merely
hidden in the wrapper. An absent active span leaves trace/span IDs absent;
`span_id` without `trace_id` is rejected before enqueue. Each language
exposes the existing Eval media contract under an Eval-specific public name
so Python's existing Prompt `MediaRef` is not mistaken for it. This adds no
new durable media-record type.

The initial media contract keeps the URI as a durable object-storage locator,
not as provider-facing prompt text. The binding `id` names an existing
`${media:id}` variable in the resolved judge Prompt; `kind` selects a
supported Skald media kind rather than inferring it from the filename. The
client queues only the small URI descriptor in the fixed
`vala.eval.observations` projection defined in `table_schema.md`. No media
download, provider call, or Eval scoring happens inside `observe.eval(...)`.

After Scribe acknowledges the input, the server best-effort enqueues the
matching continuous Eval run in Postgres. Each run freezes
`input_record_id` and `input_event_time`; the latter is the exact
server-managed `wyrd_event_time` assigned to the committed row, not the
client-authored `created_at`. The generic runner uses those values to read the
record from its bounded UTC-day partition and feeds it to the existing chain:

```text
vala.eval.observations
  -> ScenarioScoring (preserve named record.media as MediaBindings)
  -> EvalExecutor (one task DAG)
  -> JudgeTaskExecutor (one LLM-judge task and binding validation)
  -> SkaldJudgeInvoker (resolve authorized URI; bind actual media to per-call Prompt)
  -> Skald provider (native multimodal content, not URI text)
  -> EvalResults -> vala.eval.result_items + vala.verification.results
  -> pass_gate failure -> Postgres operator_dispatches
```

At judge execution, the server reads only tenant-authorized, supported
object-storage URIs, enforces bounded bytes and MIME/kind validation, then
passes the actual bytes through Skald's existing native media-binding path.
It does not assume a provider can fetch a private Wyrd URI. An unreadable,
missing, oversized, or unsupported media item is an execution/input error,
not a failed judgment or pass-gate alert. Provider-specific inline encoding
remains Skald's responsibility; provider uploads and file-ID caching are not
part of this change. Offline dataset-backed Eval is deferred, but when added
it must enter the same `ScenarioScoring -> EvalExecutor -> JudgeTaskExecutor`
chain; only its record source differs.

## Identity and routing

- One raw observation is attributed to its exact observed Card and publisher
  principal, never to a Verifier Card. The raw input is not duplicated for
  each matching binding.
- `drift_ref` and `eval_ref` are obsolete. Ordinary observation calls and
  canonical input records do not contain them. The server selects matching
  active `verified_by` bindings using the authorized subject identity.
- Each created `verifier_runs` row freezes the exact binding and Verifier UID;
  results and detail rows carry those identities for dashboards and audits.
- A binding-created run carries its containing Service or standalone Agent as
  `owner_card_uid`. A direct Verifier run has no binding owner, so
  `owner_card_uid` and `binding_id` are null. Its authenticated credential is
  preserved separately as `requested_by_principal_id` on the Postgres run and
  in the authorization audit; neither the caller, subject, nor Verifier is
  copied into the owner column.
- A Card-scoped view may be used concurrently with other views of the same
  invocation without changing their CardRefs. Unknown aliases, out-of-graph
  targets, invalid input shapes, and out-of-scope server correlation fail
  rather than silently falling back to the root Service.

This interface is required in the Rust, Python, and TypeScript SDK journeys.
