---
id: SPEC-bifrost-snowflake-federation
revision: 1
status: draft
---

# Bifrost Snowflake read federation

## Human intent and user value

A tenant that records observations and custom analytical data in Bifrost may
also keep business data in Snowflake. An authorized user or agent should be
able to issue one bounded read-only query that joins tenant-qualified Bifrost
relations with explicitly bound Snowflake relations without copying Bifrost
data into Snowflake, exposing credentials, weakening tenant isolation, or
silently changing values during cross-vendor type conversion.

This change extends Oracle query planning and execution to one external
Snowflake relation through an exact `Source` Card reference. It is not a
general distributed database, arbitrary external SQL gateway, or external
write facility.

This specification is intentionally stored in the backlog. It remains a draft
and does not authorize planning or implementation.

## Repository facts informing the draft

- Wyrd doctrine already defines `Source` as the read-only external-data
  contract and includes Snowflake as a `sql_warehouse` connection vendor.
- `wyrd-server` is the only public serving surface. Vala owns Source adapters,
  Bifrost, and Oracle execution.
- The current `BifrostQueryRequest` has no external-source binding.
- Current Oracle planning resolves every SQL relation as a tenant-qualified
  Bifrost table and registers only Bifrost providers.
- Bifrost ingestion uses exact logical schemas and schema fingerprints. There
  is no current cross-vendor coercion policy.
- The pinned analytical dependency cone is DataFusion 55.0.0 and
  Arrow/Parquet 59.2.0. Current architecture rejects bridging duplicate native
  analytical universes through FFI, IPC, JSON, or sidecars merely to reconcile
  dependencies.
- Wyrd security requires a tenant-supplied Source endpoint to undergo one
  bounded DNS resolution, address screening, and connection pinning before IO.
- Snowflake's SQL API is first-party and HTTPS-based but returns JSON row
  values that require typed decoding. Apache ADBC offers an Arrow stream but
  introduces a separately distributed driver, Arrow C boundary, and
  driver-owned network behavior that are not yet qualified against Wyrd's
  dependency and SSRF invariants.

These facts describe the current boundary; they do not select the connector
technology or override the authorities linked below.

## Scope

- One read-only Snowflake relation explicitly bound into one Bifrost query.
- Exact tenant, Source Card, object, column, and query-class authorization.
- Snowflake schema discovery and conversion to Wyrd's canonical analytical
  type system before DataFusion planning.
- Exact, fail-closed join-key type compatibility and coercion.
- Safe projection, filter, and limit pushdown with authoritative residual
  evaluation when Snowflake and DataFusion semantics are not proven equal.
- Bounded remote execution, streaming conversion, cancellation, result
  framing, and resource accounting under Oracle's existing query lifecycle.
- Independent Bifrost and Snowflake read-cut evidence in the query audit and
  terminal result.
- Consistent HTTP, Rust, Python, TypeScript, CLI, and MCP projections wherever
  the Bifrost query operation is already exposed.
- A Snowflake connectivity and schema preflight through the existing
  `wyrd source check <ref>` contract.

## Non-goals

- Writing Bifrost rows, temporary tables, query results, or metadata to
  Snowflake.
- More than one external relation or more than one Source per query in the
  initial capability.
- BigQuery, Postgres, or arbitrary SQL-warehouse federation.
- Arbitrary caller-supplied Snowflake SQL, stored procedures, multi-statement
  execution, DDL, DML, CTAS, or Snowflake-side execution of the cross-vendor
  join.
- A global transaction or globally atomic snapshot spanning Bifrost and
  Snowflake.
- Transparent support for every Snowflake type, collation, function, or
  comparison semantic.
- Automatic ingestion or materialization of Snowflake data into Bifrost.
- A second query-job API, result polling API, planner, execution attempt,
  fallback engine, scheduler, or materialized shuffle service.
- Sending Snowflake credentials to Oracle peers or allowing peers to open an
  independently authenticated Snowflake session.
- Treating SQL text, a Snowflake object name, or a Source Card reference as
  sufficient authorization by itself.

## Definitions

- **Source binding:** A public query input that assigns one SQL relation alias
  to an exact Snowflake `Source` `CardRef` and one bounded Snowflake table or
  view identity.
- **External relation:** The read-only row set produced for the bound
  Snowflake object.
- **Canonical source schema:** The Wyrd-owned logical schema derived from
  Snowflake metadata, including names, logical types, nullability, decimal
  precision and scale, timestamp unit and timezone semantics, and a stable
  fingerprint.
- **Common join type:** The one lossless canonical type to which both operands
  of a cross-vendor equality join can be converted before rows are compared.
- **Bifrost cut:** Oracle's existing immutable tenant-qualified cut across
  Iceberg, committed Scribe hot objects, and leased live-tail authority.
- **Snowflake statement cut:** The committed Snowflake state visible to the
  single remote read statement according to Snowflake's statement isolation
  semantics.
- **Remote scan:** The bounded Snowflake read that produces only the external
  columns and rows required by the admitted plan.
- **Connector:** The server-owned mechanism that authenticates to Snowflake,
  obtains metadata, submits and cancels a statement, and converts bounded
  results into the pinned Wyrd Arrow universe.

## Required behavior

### Public binding and authorization

#### REQ-001 — Explicit source binding

The public query contract shall bind the external SQL relation through one
exact `Source` `CardRef`, one relation alias, and one Snowflake table or view
identity. The SQL text shall refer to the declared alias and shall not carry
connection coordinates, credentials, a tenant identifier, or an unbound
external object name.

Aliases and object components shall be validated as identifiers. Duplicate,
ambiguous, shadowed, missing, or unused bindings shall fail before source IO.

#### REQ-002 — Tenant and permission authority

`wyrd-server` shall derive the effective tenant from the verified principal
and authorize the Bifrost query, exact Source Card version, external object,
projected columns, sensitive fields, and applicable query class before remote
row IO. A caller shall not widen tenant, Source, role, database, schema,
warehouse, or object authority through the request or SQL text.

#### REQ-003 — Read-only external behavior

The connector shall execute exactly one bounded read statement. It shall not
issue DDL, DML, session-persistent writes, temporary-object creation, stored
procedure calls, multi-statement requests, or uploads of Bifrost data.

### Schema discovery and type behavior

#### REQ-004 — Schema before planning

Oracle shall obtain the external relation's column metadata before DataFusion
planning and derive one canonical source schema. Fields shall be mapped by
stable name or identity, never by position. The schema fingerprint used for
planning shall bind the Source Card version, Snowflake object identity, field
names, logical types, nullability, decimal precision and scale, and timestamp
unit and timezone semantics.

#### REQ-005 — Initial canonical Snowflake mapping

The initial capability shall expose only these Snowflake scalar families:

| Snowflake family | Canonical Wyrd analytical type |
|---|---|
| `BOOLEAN` | `Bool` |
| character text | `Utf8` or `LargeUtf8`, chosen from validated bounds |
| binary | `Binary`, `LargeBinary`, or `FixedSizeBinary`, when exact |
| `FLOAT` and `DOUBLE` | `Float64` |
| `NUMBER(p,s)` | `Decimal128(p,s)` when `p <= 38` |
| `DATE` | `Date32` |
| `TIME` | `Time64(Nanosecond)` |
| `TIMESTAMP_NTZ` | timezone-free `Timestamp` preserving declared precision |
| instant-bearing `TIMESTAMP_LTZ` | UTC `Timestamp` preserving declared precision |
| `TIMESTAMP_TZ` | UTC `Timestamp` only under an explicitly approved offset-loss contract |

Unsupported, ambiguous, out-of-range, or incompletely described columns shall
fail schema discovery. `VARIANT`, semi-structured `OBJECT`, `ARRAY`, spatial,
vector, and vendor-extension types shall not be exposed in the initial
capability. A tenant may expose such data through a Snowflake view that
projects it into supported scalar columns.

#### REQ-006 — Lossless common join types

Oracle shall determine a common join type for every cross-vendor equality key
before scanning either relation. Implicit conversion is permitted only when it
is lossless for the complete declared domains:

- identical canonical scalar types are compatible;
- integer and fixed-point operands may widen to an exact `Decimal128`;
- two decimals use a common scale equal to the larger scale and enough
  precision for the larger integral domain, and fail if precision exceeds 38;
- timestamp units may widen without losing resolution;
- timezone-free timestamps are compatible only with timezone-free timestamps;
- instant-bearing timestamps are normalized to UTC before comparison;
- float and decimal, string and numeric, binary and string, or wall-clock and
  instant-bearing timestamps are not implicitly compatible.

An explicit SQL cast may request a lossy conversion only when the query floor
allows that cast and the resulting behavior is deterministic and documented.
Overflow, underflow, truncation, invalid UTF-8, malformed values, and ambiguous
timezone interpretation shall fail the query rather than substitute, clamp,
round, or drop a row.

#### REQ-007 — Runtime schema verification

Before accepting the first remote row batch, Oracle shall verify that the
actual returned schema equals the planned canonical source schema. Every later
batch shall carry that same schema. Metadata drift, reordered positional
interpretation, incompatible nullability, or a changed logical type shall end
the query with a typed failure and no successful terminal.

### Planning and execution

#### REQ-008 — One Oracle plan and local join

Oracle shall compose the authenticated external relation with the pinned
Bifrost providers and run the existing pinned DataFusion planner once. The
cross-vendor join shall execute under Oracle's admitted DataFusion plan. Wyrd
shall not upload the Bifrost side to Snowflake or silently substitute a second
engine, plan, snapshot, or execution attempt.

The initial remote scan shall remain coordinator-owned. Distributed Oracle
workers may process only Wyrd-authorized plan fragments and batches that do not
contain Snowflake credentials or grant independent Source access.

#### REQ-009 — Truthful pushdown

The external provider shall push only validated projection, filter, and limit
operations that preserve the admitted query's semantics. A pushed predicate
shall be reported as exact only when equivalence between Snowflake and the
pinned DataFusion version is proven for every value in its declared domain.
Otherwise DataFusion shall retain and evaluate the residual predicate.

Identifier construction shall follow Snowflake identifier semantics without
string concatenation of untrusted input. Scalar values shall use connector
parameter binding. A connector that cannot bind or safely delimit a required
value shall refuse the query.

#### REQ-010 — Bounded source and query resources

The query shall acquire one Oracle admission decision covering local
operators, remote connection and response buffers, conversion batches,
network bytes, memory, spill, result bytes, absolute deadline, and cancellation
tree. The remote scan shall stream bounded batches and shall never collect the
complete Snowflake result in memory or on an unowned scratch path.

The server shall reject a query before remote row IO when its authorized
projection, filter shape, or conservative remote-read bound cannot fit the
configured source and Oracle ceilings. Runtime exhaustion shall produce one
typed failure terminal and cancel the complete query.

#### REQ-011 — Cancellation and terminal behavior

Client cancellation, deadline expiry, authorization loss detected before IO,
connector failure, schema conflict, conversion failure, Snowflake refusal,
network loss, local operator failure, peer failure, or resource exhaustion
shall cancel the remote statement when possible, stop producers, join
descendants, and release all owners exactly once.

The query retains Oracle's single-attempt terminal contract. Rows preceding a
failure terminal do not constitute a successful result, and no connector or
planner fallback may turn that attempt into success.

### Consistency, security, and audit

#### REQ-012 — Independent read cuts

One query shall bind one immutable Bifrost cut and one Snowflake statement cut.
The public contract and documentation shall state that these cuts are
independently consistent and do not form a global transaction. A success
terminal shall identify both cuts or provide stable digests sufficient to
distinguish the exact evidence used.

#### REQ-013 — Credentials and network trust

The server shall resolve Snowflake credentials at execution time for the
verified tenant, exact Source Card, connector, and read operation. Secret
values shall never enter Cards, query payloads, plans, peer messages, logs,
traces, errors, result metadata, or audit records.

Before connecting to a tenant-derived Snowflake endpoint, the connector path
shall satisfy the repository's parse, normalize, bounded DNS resolve,
disallowed-address rejection, address-pinning, TLS-name preservation,
certificate-verification, redirect, and IO-bound requirements. Inability to
enforce any control shall make the connector unavailable rather than weaken
the control.

#### REQ-014 — Read audit acceptance

Before any row is permitted to leave the server, Oracle shall fsync one
versioned CRC-framed read-audit acceptance for the logical query. Its bounded
relay shall append the canonical tenant outbox event at least once.

The audit evidence shall bind the verified principal, permission decision,
request digest, exact Source Card reference, external object and projected
column identities, canonical source-schema fingerprint, Bifrost cut digest,
available Snowflake statement-cut identity, connector class, resource bounds,
and terminal outcome without recording credentials, raw Source payloads, or
unnecessary personal data. Distributed stages shall not create additional
logical read events.

### Surface alignment and preflight

#### REQ-015 — One public contract

HTTP, Rust, Python, TypeScript, CLI, generated schemas, documentation, and MCP
shall project the same source-binding, success, and failure semantics wherever
they expose Bifrost querying. SDKs may add typed construction ergonomics but
shall not perform durable authorization, schema authority, connector
selection, or query execution.

#### REQ-016 — Source preflight

`wyrd source check <ref>` and its typed server operation shall exercise the
same credential resolution, network controls, connector initialization, and
metadata conversion used by runtime reads without returning secrets or
performing external writes. It shall report whether the Source is usable and
return typed, redacted failure information.

## Invariants and prohibited outcomes

- **INV-001 — Tenant isolation:** No Source lookup, credential, remote row,
  batch, plan, spill object, result, audit event, or diagnostic may cross the
  verified tenant boundary.
- **INV-002 — External read-only boundary:** Wyrd never writes to Snowflake
  through Source or Bifrost federation.
- **INV-003 — Exact schema:** Cross-vendor fields are never aligned by
  position or silently reinterpreted after schema drift.
- **INV-004 — No silent loss:** Implicit cross-vendor coercion never loses
  precision, scale, timestamp resolution, timezone meaning, bytes, or rows.
- **INV-005 — One attempt:** A selected query never changes connector, source
  object, schema, cut, plan, execution path, or attempt after execution begins.
- **INV-006 — Bounded ownership:** Remote and local query work is admitted,
  bounded, cancellable, and terminally settled under one Oracle lifecycle.
- **INV-007 — Credential confinement:** Snowflake secret material remains in
  the server-owned connector boundary and never reaches DataFusion plans,
  Oracle peers, clients, or durable evidence.
- **INV-008 — Audit before rows:** No result row is released before durable
  local read-audit acceptance.
- **INV-009 — SQL is not authority:** Parsed SQL, pushdown, connector metadata,
  and engine plans never replace Wyrd authentication or authorization.
- **INV-010 — No false snapshot claim:** Wyrd never describes independent
  Bifrost and Snowflake cuts as globally atomic.

## Externally observable behavior and failure modes

| Situation | Required result |
|---|---|
| Authorized compatible join | Bounded row batches followed by one success terminal identifying both read cuts and the source-schema fingerprint |
| Missing, ambiguous, or unauthorized Source binding | Typed refusal before source IO |
| Unauthorized object or sensitive column | Typed authorization refusal before source IO |
| Unsupported Snowflake type | Typed schema-unsupported refusal naming the field and non-secret type |
| No lossless common join type | Typed coercion refusal naming both canonical operand types |
| Source schema changes after planning | Typed schema-conflict failure terminal; preceding rows are not success |
| Remote read exceeds admitted bound | Typed resource failure and remote cancellation |
| Snowflake, network, connector, or peer failure | One failure terminal with no fallback or successor attempt |
| Client cancellation or deadline | Complete cancellation and one cancellation/deadline terminal |
| Audit acceptance failure | No result rows |
| Source preflight success | Redacted confirmation of credential, network, connector, and metadata readiness |
| Source preflight failure | Stable redacted error with remediation and no write side effect |

## Material constraints

- `wyrd-spec` remains IO-free, async-free, PyO3-free, Arrow-free,
  DataFusion-free, Iceberg-free, driver-free, and cloud-SDK-free. It owns only
  typed public contracts and stable errors.
- `wyrd-server` owns the public route, authentication, authorization, request
  bounds, Source Card resolution, and public audit behavior.
- Vala owns the Snowflake read adapter, canonical schema conversion, external
  provider behavior, and Oracle execution integration.
- DataFusion executes the admitted plan but owns no authentication,
  authorization, tenancy, connector selection, audit, retry, or successful
  terminal decision.
- The chosen connector shall resolve into Wyrd's pinned Arrow/DataFusion
  dependency universe without a duplicate native analytical runtime or an
  unqualified FFI/IPC compatibility bridge.
- Existing Bifrost tenant tripwires, permissions, sensitive-column controls,
  one-planner behavior, query-owned memory/spill, deadlines, cancellation,
  audit WAL, and terminal framing remain authoritative.
- This capability must operate in self-hosted, multi-tenant SaaS, and
  single-tenant enterprise deployments without topology-specific public
  contracts.

## Required system boundaries and public interfaces

The public query request shall add a typed collection equivalent in meaning to:

```text
SourceRelationBinding {
  alias,
  source_ref: CardRef,
  object: SqlObjectRef,
}
```

The exact wire spelling remains a draft decision, but the contract shall keep
connection details and credentials out of the query request. `SqlObjectRef`
shall identify one Snowflake table or view through bounded typed identifier
components rather than raw SQL.

The public success terminal shall extend its evidence equivalent in meaning
to:

```text
ExternalReadEvidence {
  source_ref,
  object,
  schema_fingerprint,
  statement_cut,
}
```

The exact connector representation remains internal. No public contract shall
name a Rust driver, ADBC, Arrow FFI, or Snowflake SDK.

Required cross-boundary flow:

```text
client / agent
  -> wyrd-server
     -> authenticate tenant and authorize query + Source + object + columns
     -> resolve Source Card and tenant-scoped credential
     -> Vala / Oracle
        -> preflight connector and discover canonical schema
        -> pin Bifrost cut and bind Snowflake statement cut
        -> plan once with Bifrost and external providers
        -> admit aggregate local + remote resources
        -> execute one bounded remote read
        -> validate and stream canonical batches into the Oracle plan
        -> fsync read-audit acceptance before permitting result rows
        -> emit one explicit success or failure terminal
```

## Acceptance obligations

- **AC-001 — Contract evidence:** Generated schemas and public contract tests
  prove exact Source binding, identifier validation, terminal evidence, and
  stable failure shapes without exposing connector internals.
- **AC-002 — Type evidence:** Focused tests cover every supported mapping,
  lossless decimal widening, timestamp compatibility, null behavior, field
  reordering, overflow, malformed values, unsupported types, and schema drift.
- **AC-003 — Connector evidence:** Qualification against the selected
  connector version proves authentication, metadata fidelity, parameter and
  identifier safety, bounded streaming, cancellation, TLS, DNS screening and
  address pinning, error redaction, and compatibility with the pinned
  Arrow/DataFusion universe.
- **AC-004 — Authorization evidence:** Negative integration evidence proves an
  under-privileged principal cannot read another tenant, Source, object, or
  sensitive column and causes no remote row IO.
- **AC-005 — Resource evidence:** Integration evidence proves projection and
  safe filter pushdown, residual evaluation, backpressure, remote-byte and
  memory refusal, spill ownership, deadline, and complete cancellation.
- **AC-006 — Consistency and audit evidence:** Integration evidence proves the
  Bifrost and Snowflake cuts are independently identified, audit WAL failure
  prevents rows, relay replay remains safe, and no audit evidence contains a
  credential or raw Source payload.
- **AC-007 — Read-only evidence:** Connector and end-to-end evidence proves no
  external DDL, DML, temporary object, upload, or multi-statement operation is
  reachable.
- **AC-008 — User journeys:** Real Rust, Python, and TypeScript clients each
  query a real Wyrd server, join Bifrost data to a controlled Snowflake test
  relation, consume the terminal-safe result, and cover authorization,
  incompatible types, schema drift, backpressure, cancellation, and audit
  refusal. The HTTP and MCP journeys cover the capability when exposed there.
- **AC-009 — Source preflight evidence:** The public preflight uses the runtime
  credential, network, connector, and schema path and returns only redacted
  readiness or stable failure information.
- **AC-010 — Existing Bifrost closure:** Focused regression evidence proves
  Bifrost-only Interactive and Analytical queries retain their existing
  planning, tenant, resource, cancellation, audit, and terminal behavior.

## Open material decisions

An approved revision may not retain any item in this section.

### DEC-001 — Snowflake connector boundary

Choose and qualify the actual connector before approval:

- a Wyrd-owned Snowflake SQL API client plus exact typed JSON-to-Arrow
  conversion within the pinned Arrow universe; or
- an Arrow-native driver only if architecture is explicitly reconciled with
  its separate binary/runtime, foreign-memory boundary, cancellation behavior,
  and ability to enforce Wyrd's DNS screening and address pinning.

The decision must be based on a pinned-version capability spike. Convenience
or nominal Arrow support is not sufficient evidence.

### DEC-002 — External object and column authorization source

Choose the durable owner and public administration surface for the exact
Snowflake object and column grants used by REQ-002. The Source Card alone and
the Snowflake role alone are insufficient because Wyrd must make and audit its
own authorization decision before remote IO.

### DEC-003 — `TIMESTAMP_TZ` contract

Choose whether the initial capability rejects `TIMESTAMP_TZ` entirely or
normalizes it to a UTC instant while explicitly discarding original per-row
offset presentation. Silent offset loss is prohibited.

### DEC-004 — Snowflake cut evidence

Choose the minimum stable evidence exposed in the success terminal and audit:
statement/query identity when available, explicit time-travel coordinate, or
a Wyrd digest over available statement metadata. The chosen evidence must not
claim cross-system atomicity.

## Planning-decision inventory

After approval, `$wyrd-plan` must resolve the following implementation-level
decisions without changing this specification:

- exact wire field names and stable error catalog additions;
- connector crate ownership, pinned dependency capability, authentication
  modes, pooling, session setup, query tagging, and shutdown behavior;
- canonical identifier normalization and schema-fingerprint encoding;
- metadata cache lifetime and invalidation without stale-success behavior;
- provider statistics, conservative remote-read estimation, join planning,
  safe pushdown translation, and residual predicate placement;
- bounded batch geometry, conversion ownership, foreign-memory validation if
  approved, query memory/spill charging, and cancellation ordering;
- Source permission evaluation and sensitive-column integration after
  DEC-002 fixes their durable owner;
- statement-cut capture and terminal/audit projection after DEC-004 fixes the
  evidence contract;
- topology for controlled Snowflake integration fixtures and real
  client-to-server journeys without requiring developer credentials in fast
  lanes;
- generated contract, docs, boundary, connector qualification, and focused
  verification lanes required by `AGENTS.md`.

## Revision history

- **Revision 1 — draft:** Initial backlog specification for one bounded,
  read-only Snowflake relation joined with tenant-qualified Bifrost data.
  Records connector, authorization, timestamp, and cut-evidence decisions that
  must be resolved before human approval.

## Materially relevant authority and grounding

- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md) —
  Source doctrine, Card identity, public surface alignment, and Vala-owned
  vendor read adapters.
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx) —
  Wyrd nouns, service boundaries, and external-read posture.
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md) —
  Oracle planning, query admission, resources, audit WAL, cuts, cancellation,
  and terminal behavior.
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md) —
  Source credential confinement, tenant authority, SSRF defense, and audit
  privacy.
- [`architecture/references/domain/vala-architecture.md`](../../../architecture/references/domain/vala-architecture.md) —
  Vala Source and Bifrost ownership.
- [`architecture/references/domain/olap-serving.md`](../../../architecture/references/domain/olap-serving.md) —
  tenant-safe analytical serving and fail-closed query behavior.
- [`architecture/references/domain/datafusion.md`](../../../architecture/references/domain/datafusion.md) —
  provider authority, truthful pushdown, pinned dependency cone, and
  query-owned resources.
- [`architecture/references/domain/arrow-analytical-interop.md`](../../../architecture/references/domain/arrow-analytical-interop.md) —
  schema fidelity, name-based field mapping, bounded batches, and foreign
  memory validation.
- [Snowflake SQL API](https://docs.snowflake.com/en/developer-guide/sql-api/) —
  first-party statement submission, metadata, cancellation, and result
  transport.
- [Snowflake identifier literals](https://docs.snowflake.com/en/sql-reference/identifier-literal) —
  identifier binding behavior.
- [Apache ADBC Snowflake driver](https://arrow.apache.org/adbc/20/driver/snowflake.html) —
  Arrow-native alternative requiring Wyrd qualification.
