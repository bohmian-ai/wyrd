---
id: SPEC-bifrost-canonical-otel-signals
revision: 11
status: approved
---

# Canonical OpenTelemetry signal storage

## Human intent and user value

Bifrost shall accept complete OpenTelemetry traces, logs, and metrics from day
one through both its public OTLP routes and its public canonical Arrow write
path. OTLP input shall be projected into the canonical table schema; canonical
Arrow input is already in that schema and shall be validated without semantic
re-projection. Equivalent inputs through either public path shall write the
same logical records. Accepted fields must not disappear between transport
decoding or Arrow validation, Scribe acknowledgement, Forge publication, and
Oracle readback.

The current trace and GenAI table model is incomplete: trace children and
context are decoded but not fully persisted, dormant derived tables have no
production writer, and table-specific mapping is duplicated under Scribe.
Replace that shape with one canonical physical table per OpenTelemetry signal,
one table-owned semantic mapping, and no duplicated GenAI payload store.

## Current repository facts

- `wyrd-server` decodes OTLP protobuf and JSON into generated OTLP request
  types. Gate authenticates and routes those requests.
- Scribe's `direct_traces`, `direct_logs`, and `direct_metrics` modules
  currently map generated OTLP values into Arrow using positional knowledge of
  schemas declared separately under `tables/`.
- Native Arrow and OTLP writes currently converge only after their respective
  OTLP-projected and Arrow-validated batches reach Scribe's prepared-slice path.
- The trace projector validates events and links but persists only their
  dropped counts. It also drops status descriptions, most resource and scope
  data, schema URLs, and nanosecond precision.
- `vala.traces.events` and `vala.traces.links` are registered physical schemas
  without a production writer or typed query implementation.
- `vala.genai.messages`, `embeddings`, `tool_calls`, and `memory` are registered
  physical schemas without a production writer. Their former post-commit
  derivation runtime was deleted. The public GenAI query still assumes one of
  those unwritten tables.
- Existing public OTLP journey files do not prove complete public
  write-through-read fidelity.

These facts describe implementation drift; they do not define desired
behavior.

## Scope

- Redefine the built-in trace, log, and metric logical schemas for complete
  supported OTLP fidelity.
- Make the Bifrost table layer the sole owner of deterministic OTLP semantic
  projection and canonical Arrow schema validation.
- Converge public OTLP and canonical Arrow writes onto the same Scribe
  persistence path before WAL preparation.
- Store span events and links within their owning span row.
- Store GenAI spans, logs/events, and metrics in their canonical OTel signal
  tables.
- Promote the minimum GenAI span scalars required for efficient typed search
  without copying large GenAI payloads.
- Retire the typed observation read surface so that canonical SQL over the
  signal tables is the only Bifrost read path.
- Remove superseded table registrations, derived-table assumptions, mapping
  owners, tests, contracts, generated projections, and documentation.
- Prove exact public ingest, durability, publication, and readback using the
  existing Bifrost test harness and topology owners.

## Non-goals

- A new test harness, topology framework, ingest framework, mapper framework,
  extraction service, projection worker, queue, or background subsystem.
- A physical `vala.genai.*` table or a second durable copy of a GenAI span,
  prompt, response, instruction, tool definition, event, or metric.
- A physical `vala.traces.events` or `vala.traces.links` table.
- A typed per-signal or per-table read route, rpc, SDK method, or MCP tool.
  Canonical SQL is the only read contract.
- Synthesizing GenAI metrics from spans or converting span events into log
  records, or log records into span events.
- Promoting every current or future semantic-convention attribute into a
  dedicated column.
- Preserving obsolete internal module names, positional mapping, empty journey
  placeholders, or compatibility aliases for removed physical tables.
- Changing Scribe's durability boundary, Forge's publication authority,
  Oracle's execution architecture, or tenant identity rules except where the
  canonical schemas require consumer adaptation.
- Adding a dependency merely to express the canonical schemas or mappings.

## Definitions

- **Canonical signal table:** the sole durable Bifrost table that owns one OTel
  signal: `vala.traces.spans`, `vala.logs.records`, or
  `vala.metrics.points`.
- **Table-owned projection:** deterministic, IO-free conversion from a decoded
  OTLP signal into the canonical signal fields and its candidate
  `wyrd_event_time`. It does not authenticate, stamp any other server-managed
  identity, write WAL, or perform IO.
- **Canonical Arrow write:** a public Arrow IPC write whose user columns match
  the same canonical logical schema produced from OTLP.
- **Supported OTLP field:** every field exposed by the repository's pinned
  generated trace, log, and metric request types on the accepted public routes.
  Unknown future wire fields that those decoded types cannot expose are outside
  this contract; unknown semantic-convention attributes carried by supported
  attribute maps remain supported values.
- **Lossless OTLP value:** a representation that preserves the supported
  protocol value's identity, type, nesting, bytes, numeric meaning, and any
  presence distinction exposed by the pinned decoded type sufficiently to
  reconstruct the accepted semantic value.
- **Promoted scalar:** one nullable, query-oriented physical column derived
  from a canonical semantic-convention attribute while the complete attribute
  remains authoritative in the signal's lossless attribute payload.
- **GenAI span:** an ordinary OTel span carrying `gen_ai.*` semantic-convention
  attributes. It is not a separate telemetry signal.
- **OTLP partial success:** the standard response for one multi-record export
  request in which at least one complete record was accepted and at least one
  complete record was rejected. It never means that part of a record was
  stored.
- **Optional Card correlation:** a telemetry row may omit `card_ref`; the
  authenticated `principal_id` still identifies its publisher and `card_uid`
  is null. When present, `card_ref` selects one Card from the principal's
  signed scope and resolves through the trusted Card-identity-to-UID mapping in
  that signed claim.
- **OTLP correlation attributes:** the final record-level `wyrd.card_ref` and
  `wyrd.run_id` attributes, when present, project into the canonical
  correlation columns. Earlier duplicate keys remain in the lossless attribute
  payload but do not override the final value.

## Required behavior

### Ownership and ingest flow

#### REQ-001 — One table schema authority

The Bifrost table layer shall be the sole authority for each canonical table's
logical fields, stable field identities, types, nullability, sensitivity,
physical layout, schema fingerprint, OTLP semantic projection, and canonical
Arrow validation.

No Gate, Scribe, Forge, Oracle, server route, SDK, or test-only mapper may
independently encode table column order or field semantics.

#### REQ-002 — Transport and Gate ownership

`wyrd-server` shall own bounded OTLP protobuf/JSON transport decoding into the
repository's pinned generated OTLP types. Gate shall authenticate, authorize
the tenant and requested operation, validate request identity, and route the
decoded signal or transfer the canonical Arrow write to Scribe. Gate shall not
decode Arrow, project OTLP fields, own table validation, or assign row
ordinals. `wyrd_event_time` is the sole managed column whose value may
originate from signal data. For OTLP input, the table-owned projection shall
derive its candidate value from the record's canonical signal timestamp.
Canonical Arrow input may supply the same candidate column. Scribe shall
validate the candidate against the configured event-time window and stamp
server receipt time only when the candidate is absent. Table-owned validation
shall reject client-supplied values for every other server-managed column.
Neither Gate nor the server transport shall own canonical table-field mapping.

#### REQ-003 — Scribe ownership

Scribe shall own admission, server-managed and correlation-column stamping,
partition validation, WAL encoding and fsync, durable batch fencing, memtable
authority, staging, replay, recovery, and acknowledgement. Scribe may invoke
the table-owned canonical validator while consuming transferred Arrow IPC, but
shall not own or duplicate its schema rules. It shall materialize the
server-managed and correlation envelope and assign contiguous zero-based row
ordinals across the complete logical request before WAL encoding. It shall
neither accept decoded OTLP signal types nor invoke OTLP semantic projection,
and it shall contain no signal-specific schema copy.

#### REQ-004 — One canonical persistence path

An OTLP write and its semantically equivalent canonical Arrow write shall
produce equivalent user-column `RecordBatch` values for the same canonical
table. Both shall pass through the same managed-column stamping, prepared
slice, WAL, shard, memtable, staging, publication, and query authorities.

Only OTLP input shall undergo OTLP semantic projection. Canonical Arrow input
shall already carry the canonical user schema and shall undergo table-owned
schema and value validation without semantic re-projection. The two paths
shall converge before WAL preparation. Transport choice shall not change
stored semantics, row identity, durability, tenancy, or query results.

#### REQ-022 — Optional Card correlation and trusted UID resolution

Every accepted public ingest row shall carry the non-null `principal_id`
derived from its required verified principal token. `card_ref` is optional. An
absent or null `card_ref` shall be accepted and shall produce a null
`card_uid`; the server shall not infer a Card from the principal's root Card.

When `card_ref` is present, the server shall parse its exact
`(kind, space, name, version)` identity, require that identity in the
principal's signed Card scope, and stamp only the `card_uid` paired with that
identity by trusted signed claims. Token mint and refresh shall resolve each
scoped Card against the tenant-local Card registry and include the resulting
bounded identity-to-UID mapping in the signed principal claims. Ingest shall
perform no Postgres lookup or new cache lookup for Card resolution.

For OTLP input, the table owner shall read optional correlation only from the
record-level attributes named exactly `wyrd.card_ref` and `wyrd.run_id`. The
final occurrence of a duplicate key is authoritative, matching the existing
OTLP attribute lookup rule. `wyrd.card_ref` shall use the existing compact
`CardRef` grammar; a supplied `#uid` suffix is accepted as syntax but is
untrusted and ignored during authorization and UID resolution. `wyrd.run_id`
shall use the existing `RunId` text grammar. Projection shall retain every
original attribute entry losslessly. A final occurrence with the wrong OTLP
value type or malformed text shall reject only that record under REQ-005.

A malformed, out-of-scope, or signed-scope entry without a trusted UID shall
fail closed. OTLP applies that refusal atomically to each affected record under
REQ-005; canonical Arrow retains whole-write schema/value refusal. The table
owner extracts transport-specific Card correlation, Gate remains responsible
only for principal authentication, operation authorization, and routing, and
Scribe stamps the already-authorized correlation without acquiring Card
registry authority.

#### REQ-005 — Atomic records and standard OTLP batch results

An OTLP export request may contain multiple spans, log records, or metric data
points. Each record is atomic: Bifrost shall either accept and store every
supported field of that record losslessly or reject the complete record and
store none of it. A shared resource or scope defect rejects each descendant
record it affects.

When one request contains both accepted and rejected records, Bifrost shall
store the accepted records completely and return the standard signal-specific
OTLP `partial_success` response with the exact rejected-record count and a
stable actionable reason. Accepted records shall retain their relative request
order and receive contiguous `wyrd_row_ordinal` values from zero; rejected
records receive no row identity or durable state. The response shall not be
returned until the accepted records are WAL-fsynced and batch-fenced.

An empty request shall follow OTLP full-success behavior. A nonempty request
with no valid records shall return the existing signal-specific OTLP response
with the exact rejected-record count, create no canonical rows, and perform no
empty WAL append or batch fence. Transport decode, authentication,
authorization, tenant, request-wide schema or size, admission, and durability
failures shall fail the whole request through the existing stable error
contract, shall not return `partial_success`, and shall not commit an
authoritative partial batch.

### Canonical traces

#### REQ-006 — Complete span row

`vala.traces.spans` shall store one row per accepted OTLP span containing:

- trace ID, span ID, nullable parent span ID, trace state, and flags;
- name and the original supported span-kind value without collapsing distinct
  wire values;
- nanosecond start and end timestamps plus a consistently derived duration;
- status code and status message;
- lossless span attributes and dropped-attribute count;
- the complete ordered span-event collection and dropped-event count;
- the complete ordered span-link collection and dropped-link count;
- complete supported resource attributes, resource dropped-attribute count,
  resource schema URL, and supported resource entity references;
- instrumentation-scope name, version, attributes, dropped-attribute count,
  and schema URL; and
- the table's server-managed and correlation envelope.

The schema shall retain nullable `service_name` as a promoted scalar sourced
only from the string value of the canonical resource attribute `service.name`.
A missing or non-string source value shall produce null while the original
resource attribute remains losslessly retained. Existing trace filters and
summaries shall use this promoted column. Derived duration and promoted scalars
shall never replace their canonical source values.

#### REQ-007 — Nested span events

Each span event shall remain ordered within its owning span and preserve its
timestamp, name, lossless attributes, and dropped-attribute count. Repeated
event and link collections shall preserve request order; an empty decoded
collection remains empty without inventing an unavailable presence state.

#### REQ-008 — Nested span links

Each span link shall remain ordered within its owning span and preserve its
linked trace ID, linked span ID, trace state, flags, lossless attributes, and
dropped-attribute count.

#### REQ-009 — Trace readback

Canonical SQL shall read one trace cut from `vala.traces.spans` with complete
events, links, status, resource, scope, and attribute content. It shall not
return literal empty child collections as a stub or reinterpret incomplete
storage as "none recorded."

### Canonical logs and OTel events

#### REQ-010 — Complete log record

`vala.logs.records` shall store one row per accepted OTLP log record containing
the supported time and observed time, severity number and text, event name
when present in the pinned protocol, lossless body, trace ID, span ID, trace
flags, lossless attributes, dropped-attribute count, complete supported
resource and instrumentation-scope data and schema URLs, and the server-managed
and correlation envelope.

#### REQ-011 — Preserve received signal identity

A child supplied in an OTLP span's event collection shall remain a nested span
event. A GenAI or other OTel event supplied through the OTLP Logs signal shall
remain a log record with its trace/span correlation. Bifrost shall not
synthesize, copy, or translate records between those signal forms.

### Canonical metrics

#### REQ-012 — Complete metric point

`vala.metrics.points` shall retain the metric identity, description, unit,
lossless metric metadata, complete supported resource and
instrumentation-scope data and schema URLs, and one canonical row per accepted
point for every supported OTLP metric kind.

The point representation shall preserve as applicable:

- point and start timestamps at supported wire precision;
- flags and lossless attributes;
- integer values without conversion through floating point and double values
  with their defined IEEE meaning;
- aggregation temporality and monotonicity;
- histogram count, sum, minimum, maximum, bucket counts, explicit bounds, and
  exemplars;
- exponential-histogram scale, zero count, zero threshold, positive and
  negative buckets, and exemplars;
- summary count, sum, and ordered quantile values; and
- exemplar time, value type, filtered attributes, trace ID, and span ID.

Unsupported or internally inconsistent metric shapes shall fail explicitly
instead of being normalized into another kind or partially stored.

### GenAI semantics

#### REQ-013 — Canonical GenAI signal placement

GenAI inference, embedding, retrieval, agent, workflow, memory, and tool spans
shall remain canonical `vala.traces.spans` rows. GenAI log/event records shall
remain canonical `vala.logs.records` rows. Emitted GenAI metrics shall remain
canonical `vala.metrics.points` rows.

The OpenTelemetry GenAI semantic conventions shall not create a fourth signal
or require a duplicate physical span table.

#### REQ-014 — Complete GenAI payload retained once

Every accepted `gen_ai.*` attribute, including unrecognized attributes from a
newer compatible convention, shall remain in the canonical signal's lossless
attribute payload. Large or sensitive GenAI content shall have one logical
canonical payload record in its owning signal table unless the client emitted
it as multiple OTel records. No trace-child or `vala.genai.*` table shall hold
a derived payload copy. WAL, memtable, staged Parquet, published objects,
Iceberg snapshots, Forge rewrites, backups, and recovery artifacts representing
that same logical row are permitted lifecycle representations, not additional
logical records.

#### REQ-015 — Minimal GenAI promoted span columns

`vala.traces.spans` shall expose these nullable promoted columns:

- `gen_ai_operation_name`, sourced only from `gen_ai.operation.name`;
- `gen_ai_provider_name`, sourced only from `gen_ai.provider.name`;
- `gen_ai_request_model`, sourced only from `gen_ai.request.model`;
- `gen_ai_conversation_id`, sourced only from `gen_ai.conversation.id`;
- `gen_ai_usage_input_tokens`, sourced only from
  `gen_ai.usage.input_tokens`; and
- `gen_ai_usage_output_tokens`, sourced only from
  `gen_ai.usage.output_tokens`.

Promotion shall use the exact typed value retained in the canonical attributes
payload. Missing source attributes produce null. A present source attribute of
the wrong type rejects the containing span. No legacy key alias or fallback
source is permitted.

#### REQ-016 — GenAI convention version awareness

GenAI interpretation and promoted-field mapping shall follow OpenTelemetry
GenAI semantic conventions commit
`94f432d7126f5884d30a2cdde6f4e89908ebb6fd`. Bifrost shall preserve applicable
resource and scope schema URLs. Unknown or newer attributes remain losslessly
stored but do not acquire promoted columns or change query classification until
a later approved specification updates the pinned convention.

#### REQ-017 — One canonical read path

Canonical SQL over the signal tables shall be the only Bifrost read contract.
There shall be exactly one obvious way to read an observation, and it shall be
the same way a caller reads any other Bifrost table.

Generation-only search shall remain expressible as a predicate on the promoted
columns REQ-015 defines: `gen_ai_operation_name` in exactly `chat`,
`generate_content`, or `text_completion` selects generation spans without
scanning the canonical attributes payload. The promoted columns exist to make
that predicate cheap; they do not justify a dedicated read surface.

Structured GenAI content shall be read from the canonical attributes payload
sourced from `gen_ai.input.messages` and `gen_ai.output.messages`, preserving
array order, object structure, scalar types, and nulls. The speculative
always-absent `cost_usd` field and its `genai.messages` documentation shall be
removed; no cost field shall exist until a canonical emitted source and query
requirement are separately approved.

#### REQ-018 — GenAI metrics are emitted facts

Bifrost shall store GenAI metrics only when received through the canonical
metrics ingest contract. It shall not derive token, duration, agent, workflow,
or tool metrics from spans and present them as emitted OTel metrics.

### Removal

#### REQ-019 — Remove superseded physical surfaces

The current tree shall remove the physical `vala.traces.events`,
`vala.traces.links`, and `vala.genai.*` table definitions, registrations,
writer/reader assumptions, generated schema projections, documentation, tests,
and exclusive configuration. It shall not add aliases, compatibility routes,
or empty replacement tables for those removed physical names.

#### REQ-020 — Remove duplicate mapping authority

The current Scribe-owned `direct_traces`, `direct_logs`, and `direct_metrics`
mapping modules and the obsolete test-only projection oracle shall be removed
after their required behavior is represented by the table-owned mappings.
Generic Scribe admission, managed-column, Arrow/IPC, and durability machinery
may remain when it contains no table-specific semantic or positional schema
copy.

Removal of `direct_traces`, `direct_logs`, `direct_metrics`, and the test
projection oracle shall not remove OTLP record-level rejection or
partial-success behavior. Their deterministic validation and accepted-subset
projection shall move to the owning table modules. The existing signal-specific
OTLP outcome contract and HTTP/gRPC partial-success response encoding shall be
reused unchanged; this change shall not introduce a replacement outcome type,
response encoder, or second projection path. Scribe-owned semantic validators
and parity copies shall be deleted only after the table-owned projector and
existing public-route tests cover the same behavior.

#### REQ-023 — Remove the typed observation read surface

The current tree shall remove the typed observation read surface in full: the
`GetTrace`, `QueryTraces`, `QueryRecentTraces`, `QueryGenAi`, `QueryEval`,
`QueryDrift`, `QueryMetrics`, `QueryLogs`, and `QueryAgentTraces` HTTP routes
and rpcs, their request and response types, their plan builders and filter
helpers, their SDK and binding methods, their generated projections and stubs,
their per-route configuration and body limits, and their documentation and
route tests.

Each of these is a hardcoded SELECT over one canonical table with a time
window and a small fixed set of equality predicates, which the canonical query
contract already serves. Trace-waterfall nesting is a presentation concern for
whichever client wants it, not a stored shape or a server contract.

Canonical Arrow write, `POST /v1/bifrost/query` and its `Query` rpc, table
describe, query lifecycle controls, and the OTLP ingest routes are retained
unchanged. No alias, compatibility route, replacement typed reader, or
client-side reimplementation of a removed route shall be added.

#### REQ-021 — Pre-release replacement

No production data migration is required because these schemas and tables have
not shipped. The change shall replace the three canonical schemas directly and
delete the stale trace-child and `vala.genai.*` tables, registrations, control
state, maintenance work, configuration, and generated artifacts. Development
and test environments shall recreate these pre-release tables from the current
definitions. The change shall add no migration, backfill, compatibility reader,
dual-schema serving, or legacy alias.

## Invariants and prohibited outcomes

#### INV-001 — One canonical durable copy

One accepted span, log record, or metric point has one logical canonical row in
one canonical signal table. Promoted columns are fields on that row. Normal
Scribe and Forge lifecycle representations do not create another logical
record.

#### INV-002 — No silent field loss

No accepted trace, log, metric, event, link, resource, scope, exemplar, or
GenAI field covered by this specification may be silently dropped, narrowed,
defaulted, or synthesized.

#### INV-003 — No positional cross-schema mapping

Any mapping or validation across independently represented schemas shall bind
fields by stable identity or name, never an independently maintained positional
index.

#### INV-004 — Signal boundaries remain exact

Trace children, log/event records, and metric points remain the signal forms
the caller supplied. Storage optimization shall not change their OTel meaning.

#### INV-005 — Canonical source wins

Promoted `service.*` or `gen_ai.*` values shall equal their canonical
attribute source. Conflicting duplicated values are prohibited.

#### INV-006 — Sensitive payload remains protected

Span attributes and events, log bodies and attributes, and large GenAI content
remain sensitive payload. Authorization shall be enforced before projection or
return, and sensitive content or identifiers shall not become metric labels.

#### INV-007 — Tenant isolation is unchanged

Every ingest and query path shall continue to derive tenant identity from the
authenticated principal and enforce the existing physical table, row,
Postgres, object-prefix, Scribe, Oracle, and plan-root tripwires.

#### INV-008 — Acknowledgement retains its meaning

Acknowledgement means the complete canonical rows for that accepted logical
batch are WAL-fsynced, batch-fenced, and authoritative in Scribe. Successful
decode or partial projection is insufficient.

#### INV-009 — Bounded processing

Nested events, links, attributes, bodies, histogram structures, exemplars, and
GenAI payloads remain subject to checked configured request, row, nesting,
count, and material limits before unbounded allocation or durable mutation.

#### INV-010 — No new harness or shadow path

The change shall extend the existing Bifrost harness, table registry, public
ingest routes, Scribe path, Forge publication, and Oracle query owners. A new
harness or parallel ingest/query implementation is prohibited.

#### INV-011 — Publisher identity without mandatory Card identity

An accepted public ingest row always identifies its authenticated publisher by
`principal_id`. Missing Card correlation never causes the server to substitute
the principal's root Card, and resolving a present `card_ref` never adds
Postgres or a new cache to the ingest hot path.

## Externally observable behavior and failures

- An authorized OTLP client can export complete traces, logs, and every
  supported metric kind, receive an acknowledgement only after Scribe's
  durable boundary, and read equivalent values after flush and publication.
- An authorized canonical Arrow client can write the equivalent logical rows
  and observe the same query results and durability behavior.
- Canonical SQL returns actual nested events and links with their parent spans.
- GenAI search uses promoted-column predicates through canonical SQL, reads
  canonical spans, and does not require duplicated payload storage.
- A caller without sensitive-payload permission can query permitted metadata
  but cannot receive protected bodies, attributes, messages, instructions, or
  tool content.
- Invalid IDs, timestamps, value encodings, nested structures, metric shapes,
  schema fingerprints, field types, tenant identities, and configured size
  limits fail with stable structured errors before acknowledgement.
- A supplied field that the pinned canonical schema cannot represent fails
  explicitly. It is never acknowledged and omitted.

## Material constraints

- The canonical schema and mappings are server-owned Rust behavior.
- `wyrd-spec` remains the owner of public typed request/response contracts and
  stable errors; it remains IO-free and PyO3-free.
- The canonical analytical dependency cone and existing Arrow, Parquet,
  Iceberg, and DataFusion versions remain authoritative unless a separately
  justified dependency change is required and approved.
- Schema fingerprints, Iceberg field identities, Arrow and Parquet types,
  nullability, timestamp units, nested structures, and sensitive-column
  metadata must agree across every generated and runtime projection.
- Parquet projection shall permit GenAI list/search queries to avoid reading
  large sensitive payload columns when the requested result does not need
  them.
- Generated schemas, OpenAPI, stubs, descriptors, and golden files are updated
  from their owning sources, never hand-edited.
- No new Cargo feature or dependency is introduced without demonstrated need.
- Existing tests, fixtures, journeys, integration targets, and topology owners
  that already prove required behavior shall be reused. A parallel test,
  fixture, integration suite, or topology test shall not be created for
  behavior already covered by an existing owner. Newly uncovered behavior
  shall extend the nearest existing test owner whenever that owner can express
  it.

## Required system boundaries and flow

```text
OTLP protobuf/JSON
  -> wyrd-server bounded transport decode
  -> Gate authentication, authorization, tenant and request routing
  -> table-owned signal projection
  -> canonical TableRef + user-column RecordBatch + trusted request context
  -> Scribe request-wide row-ordinal and managed-envelope materialization
  -> partitioning, admission, WAL and live authority
  -> Forge publication and maintenance
  -> Oracle canonical query result

Canonical Arrow IPC
  -> Gate authentication, authorization, tenant and request routing
  -> ownership transfer to Scribe
  -> table-owned canonical schema/value validation invoked by Scribe
  -> the same Scribe ordinal, managed-envelope, Forge and Oracle path
```

No client or route may supply server-managed columns other than the validated
`wyrd_event_time` exception above. No table projection may perform IO,
authentication, durable mutation, or query execution.

## Acceptance obligations

Using only the existing Bifrost harness and fixtures, acceptance shall
exercise:

- OTLP HTTP protobuf and JSON for each exposed trace, log, and metric route;
- canonical Arrow ingestion through the public Bifrost gRPC contract and the
  Rust, Python, and TypeScript SDK projections;
- canonical SQL reads of the signal tables through their HTTP and gRPC
  contracts and the Rust, Python, and TypeScript SDK projections; and
- affected agent-facing MCP reads and writes.

One canonical expected dataset may be reused across these journeys. Each
journey shall cross the real server, Scribe acknowledgement, flush/publication,
Oracle read, and client decode boundary. No new harness, in-process engine
substitute, or duplicate topology framework may provide the evidence.

#### AC-001 — Complete trace fidelity

The canonical expected dataset shall contain a representative maximal supported
OTLP trace with resource and scope metadata, schema URLs, nanosecond timing,
status description, nested and byte-bearing attributes, multiple ordered
events, multiple ordered links, dropped counts, and GenAI attributes. Readback
shall assert every value and order exactly.

#### AC-002 — OTLP and Arrow equivalence

The same logical trace, log, and representative metric data written through
OTLP and canonical Arrow shall produce equivalent canonical user columns and
public query results, excluding intentionally different server-managed
identity and ingestion timestamps.

#### AC-003 — Complete log fidelity

The canonical expected dataset shall prove complete supported log/resource/
scope/body/attribute correlation through authorized and redacted readback.

#### AC-004 — Complete metric-kind fidelity

The canonical expected dataset shall prove integer and double gauge/sum,
histogram, exponential histogram, summary, attributes, temporality,
monotonicity, lossless metric metadata, resource/scope metadata, and exemplars
through write and readback.

#### AC-005 — GenAI signal fidelity without duplication

The shared journeys shall prove that representative GenAI inference, embedding,
agent, workflow, tool, event/log, and metric signals remain complete in their
canonical signal tables; generation-only search uses promoted columns through
canonical SQL; and no `vala.genai.*` payload copy is written.

#### AC-006 — Failure and OTLP partial-success behavior

Focused tests and existing public HTTP protobuf, HTTP JSON, and gRPC negative
journeys shall send mixed-validity requests for traces, logs, and metrics. They
shall prove that valid siblings are acknowledged and readable without field
loss, invalid records are absent, accepted relative order is preserved,
`wyrd_row_ordinal` is contiguous over only the accepted subset, and the
signal-specific OTLP `partial_success` reports the exact rejected count and a
stable reason. Recovery/replay evidence shall prove that the one batch fence
and payload digest cover exactly that accepted canonical subset.

The same evidence shall prove that an all-invalid nonempty request returns the
existing signal-specific OTLP response with the exact rejected count, no
canonical rows, no empty WAL append, and no batch fence. Malformed transport,
schema drift, request-wide oversize, access without the existing ingest
permission, tenant mismatch, admission failure, and durability failure shall
reject the complete request without `partial_success` or an authoritative
partial batch. Canonical Arrow shall retain its existing whole-batch
schema/value refusal; AC-002 remains the equivalence proof for valid logical
data. Sensitive reads without the applicable existing payload-read permission
shall remain denied or redacted by the owning public contract.

#### AC-007 — Durability and recovery

Existing Scribe recovery and replay evidence shall prove that nested canonical
rows retain exact batch identity and values across timeout, retry, rotation,
restart, staging, publication, and duplicate-source suppression.

#### AC-008 — Schema and storage round trips

Focused table tests shall prove stable named field mapping and exact logical
schema, Arrow, Parquet, Iceberg, and Arrow round trips, including nested
events/links, lossless values, nanosecond timestamps, integers, IEEE doubles,
and every presence distinction exposed by the pinned decoded types.

#### AC-009 — Existing topology owners

Existing standalone/distributed and single-tenant/multi-tenant Bifrost topology
journeys shall remain passing. Each existing topology shall exercise at least
one representative complete-signal public write-through-read path. Exhaustive
trace, log, metric-kind, and OTLP-versus-Arrow fidelity may be proven once in
focused public journeys and shall not be duplicated across every topology. No
new harness may supply this evidence.

#### AC-010 — Complete verification

The implemented change shall pass focused named tests plus the repository's
canonical `mise run codegen:check`, `mise run fmt`, `mise run lints`,
`mise run verify:bifrost`, and, because this change crosses public contracts,
server composition, all Bifrost roles, generated artifacts, and test
infrastructure, `mise run gate`.

#### AC-011 — Existing test ownership

The implementation evidence shall identify the existing test, fixture,
journey, integration target, and topology owner reused for each acceptance
obligation. Existing passing evidence shall not be duplicated. When an
obligation is not already covered, its scenario shall be added to the nearest
existing owner rather than a new parallel suite. A new test owner is permitted
only when no existing owner can execute the required public boundary, and that
gap must be demonstrated before the new owner is created.

#### AC-012 — Optional Card correlation without ingest IO

Existing auth and Bifrost tests shall prove that an authenticated row without
`card_ref` is accepted with non-null `principal_id` and null `card_uid`; root
and secondary scoped references receive the UID paired with their identity in
verified signed claims; malformed, unmapped, and out-of-scope references fail
closed; OTLP uses the exact approved record keys, final-entry behavior, and
existing text grammars while retaining the original attributes; and no tenant
Card-registry query or new cache participates in ingest.

## Open material decisions

None.

## Planning-decision inventory

`$wyrd-plan` shall decide only implementation-level mechanics consistent with
this specification:

- the smallest cohesive task boundaries for contracts, canonical table
  schemas/projections, Scribe convergence, query adaptation, stale deletion,
  generated artifacts, and integrated journeys;
- the smallest signed-claim representation that preserves each bounded scoped
  Card identity with its tenant-registry-resolved UID through mint, refresh,
  verification, and in-memory ingest lookup;
- the exact lossless Arrow/Parquet representation for OTLP `AnyValue` and
  nested collections, using the pinned dependency capabilities;
- the deletion and replacement order for the pre-release schemas and stale
  table surfaces fixed above;
- the reuse or deletion of generic exact-capacity and IPC machinery after
  table-specific schema knowledge is removed;
- the focused Red-Green-Refactor scenario order, mapping to existing test
  owners, and exact named-test commands; new test owners require the evidence
  gap fixed by AC-011;
  and
- reconciliation with the integrated Forge/Oracle owners so the journeys do
  not reintroduce deleted table or mapping authority.

These decisions may not add another durable signal copy, table-specific Scribe
mapping, positional schema authority, new harness, or unresolved product
choice.

## Revision history

- Revision 11 (2026-09-10): reconcile the remaining trace and GenAI readback
  prose with revision 10's canonical-SQL-only contract, align the Bifrost
  authority, and treat the integrated Forge/Oracle change as the implementation
  baseline. This revision also permits the focused OTLP journey target to be
  earned back with its first real tests; empty scaffolding remains prohibited.
- Revision 10 (2026-09-09): retire the typed observation read surface in favor
  of one canonical SQL read path, replacing REQ-017's typed `QueryGenAi`
  requirement with the single-read-path rule and adding REQ-023 to remove the
  nine typed routes, rpcs, types, SDK methods, projections, and docs. The
  promoted GenAI columns are retained as SQL predicates.
- Revision 9 (2026-09-07): bind optional OTLP Card and Run correlation to the
  exact record-level `wyrd.card_ref` and `wyrd.run_id` attributes, reuse the
  existing `CardRef` and `RunId` text grammars, retain last-entry lookup
  semantics and lossless source attributes, and ignore any client-supplied
  Card UID during trusted scope resolution.
- Revision 8 (2026-09-07): make Card correlation optional while retaining
  required authenticated publisher identity; resolve present Card references
  from a bounded trusted identity-to-UID mapping populated during token mint
  and refresh with no Postgres or new cache on the ingest hot path; restore
  Gate to authentication and routing; and assign request-wide row ordinals in
  Scribe.
- Revision 7 (2026-09-06): preserve the existing all-rejected OTLP response,
  add lossless metric metadata, and require reuse or extension of existing
  tests, fixtures, journeys, integration targets, and topology owners instead
  of parallel test infrastructure.
- Revision 6 (2026-09-06): require direct reuse of the existing OTLP outcome
  contract and HTTP/gRPC partial-success encoding rather than redesigning or
  relocating that behavior.
- Revision 5 (2026-09-06): clarify that each OTLP record is accepted completely
  or rejected completely while the standard `partial_success` response applies
  only to a mixed multi-record request, and remove all migration and historical
  data handling because the affected schemas have not shipped.
- Revision 4 (2026-09-06): incorporate the second independent audit and
  reviewer consensus for event-time ownership, exact pinned GenAI mappings and
  generation-only query behavior, structured GenAI payload responses, logical
  copy semantics, standards-conformant OTLP partial success, forward schema
  activation and safe table retirement, required `service_name` projection,
  and explicit existing-harness public-surface coverage.
- Revision 3 (2026-09-06): apply the independent Ponytail audit by fixing the
  Gate/table/Scribe boundary, defining supported OTLP fields, removing an
  impossible repeated-field presence promise, reducing GenAI promotion to the
  current typed query surface, clarifying canonical row cardinality, focusing
  topology evidence, and removing non-authoritative storage comparisons.
- Revision 2 (2026-09-06): clarify that Bifrost accepts complete signals
  through both public OTLP and canonical Arrow write paths, and that only OTLP
  input is semantically projected while canonical Arrow input is validated.
- Revision 1 (2026-09-06): initial draft defining canonical OTLP signal
  storage, table-owned mapping, complete trace/log/metric fidelity, nested span
  children, GenAI promoted scalars without derived tables, stale-surface
  deletion, and existing-harness production proof.

## Material authority and grounding

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/languages/testing-workflows.md`
- [OTLP trace protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
- [Pinned OpenTelemetry GenAI semantic conventions](https://github.com/open-telemetry/semantic-conventions-genai/blob/94f432d7126f5884d30a2cdde6f4e89908ebb6fd/docs/gen-ai/gen-ai-spans.md)
