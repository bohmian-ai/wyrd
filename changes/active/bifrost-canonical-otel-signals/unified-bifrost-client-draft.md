---
id: DRAFT-unified-bifrost-client
status: draft
---

# Unified Bifrost client — Rust, Python, TypeScript

## 1. What is wrong today

| Surface | Read | Write | Register |
|---|---|---|---|
| Rust | `vala_sdk::QueryClient::new(&WyrdClient)` | `vala_sdk::Bifrost` | none — raw HTTP `POST /v1/bifrost/tables` |
| Python | `BifrostQueryClient(url, token)` | `Bifrost(url, api_key)` | none |
| TypeScript | `new WyrdClient(url, token).bifrost` / `BifrostQueryClient` | `await Bifrost.connect(url, apiKey)` | none |

Four problems:

1. **Two clients per language for one subsystem.** Read and write already run
   off one `WyrdClient` and one credential — `AuthMiddleware` exchanges an API
   key for a bearer at `crates/wyrd/wyrd-client/src/auth.rs:299`. The split is
   packaging, not architecture.
2. **No registration path.** `POST /v1/bifrost/tables` exists
   (`crates/wyrd/wyrd-server/src/bifrost/routes.rs:18`) and no SDK calls it. A
   user hand-writes JSON Schema, hand-writes an HTTP call, then hand-writes the
   same schema again on every `insert`.
3. **Schema is repeated per row.** `Bifrost::insert(table, schema, row, card_ref,
   run_id)` takes the full schema on every call and silently ignores it after
   the first (`crates/vala/vala-sdk/src/handle.rs:159`). The parameter is a lie
   after row one.
4. **`card_ref` is required by the client and optional on the server.**
   `crates/wyrd-spec/src/vala/api.rs:117` says a writer may omit it and the
   server stores `principal_id` with a null `card_uid`. Every SDK forces it.

## 2. The shape

One class per language, named `Bifrost`, constructed with an **optional**
`TableConfig` and an **optional** transport. Without a table it is a query
client. With a table it is a query client that can also write. The bound table
is swappable.

No function anywhere on this surface requires a server URL, token, or API key.
Every one is optional and auto-resolves when omitted, so the smallest working
client takes no arguments at all:

```
Bifrost(table?, transport?)
  ├── register()                 create the active table (idempotent)
  ├── use_table(TableConfig)      swap the active write target
  ├── insert(row)                 enqueue into the active table
  ├── flush() / shutdown()        make enqueued rows durable
  ├── sql(query)   -> QueryResult          any table, collected
  └── stream(query) -> batch iterator      any table, chunked
```

`TableConfig` is built from a language model (`serde` + `schemars`, Pydantic,
Zod) or fetched by name from an already-registered table.

**Writes bind one table; reads bind none.** That asymmetry is the server's, not
an SDK invention: an ingest batch carries exactly one `SealedBatch.table` and
one Arrow schema, while `POST /v1/query` takes arbitrary SQL over any
authorized table.

## 3. Core Rust — `crates/vala/vala-sdk`

### 3.1 `TableConfig`

```rust
/// One Bifrost table a writer is bound to: its identity, the user columns it
/// declares, and the physical layout it asks the server to create.
///
/// A config is inert until [`Bifrost::register`] or [`TableConfig::describe`]
/// resolves it against the server. Only the server mints a fingerprint and a
/// table uid, so this type never computes one: a client-side fingerprint is a
/// second authority for the same identity and would drift the moment the
/// mapping changes on either side.
#[derive(Debug, Clone)]
pub struct TableConfig {
    namespace: String,
    name: String,
    user_schema: SchemaRef,
    physical_layout: Option<PhysicalLayoutWire>,
    /// Server-assigned once registered or described; `None` while inert.
    resolved: Option<ResolvedTable>,
}

/// The server-authoritative identity of a registered table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTable {
    /// Lower-case hex of the 16-byte table uid.
    pub table_uid: String,
    /// Lower-case hex of the 32-byte user-schema fingerprint.
    pub fingerprint: String,
}

impl TableConfig {
    /// Build a config from an explicit Arrow schema.
    ///
    /// The precision path: a caller who needs `Int32`, a non-UTC timezone,
    /// `Decimal128`, or `FixedSizeBinary` supplies Arrow directly, exactly as
    /// [`wyrd_queue::arrow_schema_to_fieldspec`] documents.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_400_SCHEMA_PARSE` when `fqn` is not `<namespace>.<name>`
    /// or a column uses a reserved `wyrd_*`, `card_ref`, or `run_id` name.
    pub fn from_arrow(fqn: &str, schema: SchemaRef) -> Result<Self, ValaSdkError>;

    /// Build a config from a JSON Schema document.
    ///
    /// This is the Pydantic (`model_json_schema()`) and Zod (`z.toJSONSchema()`)
    /// door. Mapping is delegated verbatim to
    /// [`wyrd_queue::json_schema_to_arrow`], which is the single owner of the
    /// JSON-Schema → Arrow table.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_400_SCHEMA_PARSE` for a free-form object, an untyped
    /// array, an unsupported type, or an unresolvable `$ref`.
    pub fn from_json_schema(fqn: &str, schema: &serde_json::Value) -> Result<Self, ValaSdkError>;

    /// Build a config from a Rust type that derives [`schemars::JsonSchema`].
    ///
    /// The Rust twin of the Pydantic path: `TableConfig::from_model::<Prediction>(fqn)`
    /// is `from_json_schema` over `schemars::schema_for!(T)`.
    ///
    /// # Errors
    /// As [`TableConfig::from_json_schema`].
    pub fn from_model<T: schemars::JsonSchema>(fqn: &str) -> Result<Self, ValaSdkError>;

    /// Fetch an already-registered table's config by name.
    ///
    /// Calls `GET /v1/bifrost/tables/{namespace}/{name}` and takes the schema
    /// from the description's `user_fields`, so a writer joining an existing
    /// table never restates a schema it does not own. The returned config is
    /// already resolved.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_404_TABLE_NOT_FOUND`, or stable authentication,
    /// authorization, availability, or protocol errors.
    ///
    /// # Cancellation
    /// Abandoning the future leaves no server state behind; describe is a read.
    pub async fn describe(client: &WyrdClient, fqn: &str) -> Result<Self, ValaSdkError>;

    /// As [`TableConfig::describe`], resolving the transport itself.
    ///
    /// # Errors
    /// As [`TableConfig::describe`], plus `WyrdClientError::NoCredentials` when
    /// the credential chain yields nothing.
    pub async fn describe_from_env(fqn: &str) -> Result<Self, ValaSdkError>;

    /// Declare the physical layout the register call should request.
    ///
    /// Omitted, the server resolves `hour(wyrd_event_time)`, `wyrd_event_time`
    /// descending nulls-last, and the managed Bloom floor.
    #[must_use]
    pub fn with_layout(self, layout: PhysicalLayoutWire) -> Self;

    /// `<namespace>.<name>` — the name SQL and `SealedBatch.table` use.
    #[must_use]
    pub fn fqn(&self) -> String;

    /// The user columns this config declares, without correlation or managed
    /// columns. The producer appends those itself.
    #[must_use]
    pub fn user_schema(&self) -> &SchemaRef;

    /// The server-assigned identity, present only after register or describe.
    #[must_use]
    pub fn resolved(&self) -> Option<&ResolvedTable>;
}
```

### 3.2 `Bifrost` — async core

The existing `vala_sdk::Bifrost` producer pool becomes the private `WriterPool`
(the whole file `handle.rs` keeps its logic, loses its public name). `Bifrost`
becomes the facade over `QueryClient` + `WriterPool` + the active table.

```rust
/// The one Bifrost client: query any authorized table, write to the active one.
///
/// Reads are unbound because `POST /v1/query` accepts SQL over any table the
/// caller may see. Writes are bound because an ingest batch carries exactly one
/// table and one Arrow schema; switching targets is [`Bifrost::use_table`], and
/// a swapped-away table's pooled producer keeps draining rather than being
/// discarded.
pub struct Bifrost {
    query: QueryClient,
    writer: WriterPool,
    active: Mutex<Option<TableConfig>>,
}

impl Bifrost {
    /// Build a client with no arguments, resolving its own transport.
    ///
    /// Endpoints come from `WYRD_GRPC_URL` / `WYRD_SERVER_URL` or the compiled
    /// defaults, and the credential from the
    /// [`ClientConfig::resolve_credential`] chain, whose floor is
    /// `~/.config/wyrd/credentials.toml` `[default].api_key`. This is
    /// [`WyrdClient::from_env`] plus a connect; it adds no resolution of its own.
    ///
    /// # Errors
    /// Returns `WyrdClientError::NoCredentials` when the chain yields nothing,
    /// or a transport error when the ingest channel cannot be dialled.
    pub async fn from_env() -> Result<Self, ValaSdkError>;

    /// Build a client over an explicitly supplied transport.
    ///
    /// `client` is an argument, not an owner: it carries the credential, the
    /// connection pool, and the API-key-to-bearer exchange the read and write
    /// planes already share.
    ///
    /// # Errors
    /// Returns a transport error when the ingest channel cannot be dialled.
    pub async fn connect(client: &WyrdClient) -> Result<Self, ValaSdkError>;

    /// Build a client already bound to `table` for writes.
    ///
    /// # Errors
    /// As [`Bifrost::connect`].
    pub async fn connect_with_table(
        client: &WyrdClient,
        table: TableConfig,
    ) -> Result<Self, ValaSdkError>;

    // ── table lifecycle ────────────────────────────────────────────────────

    /// Create the active table, returning whether it was created or matched.
    ///
    /// Idempotent: `POST /v1/bifrost/tables` answers `AlreadyExists` for a
    /// matching fingerprint. On success the active config is resolved in place,
    /// so `table().resolved()` reports the server's uid and fingerprint.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_409_TABLE_FINGERPRINT_CONFLICT` when a table of the
    /// same name exists with different columns, `WYRD_VALA_400_SCHEMA_PARSE`
    /// for a reserved column name, or `WYRD_VALA_412_NO_ACTIVE_TABLE` when no
    /// table is bound.
    pub async fn register(&self) -> Result<RegisterOutcome, ValaSdkError>;

    /// Bind `table` as the write target, returning the previous binding.
    ///
    /// Cheap and synchronous: the producer for the new table is built lazily on
    /// its first row. The previous table's producer stays in the pool, so its
    /// buffered rows still flush.
    pub fn use_table(&self, table: TableConfig) -> Option<TableConfig>;

    /// Bind an already-registered table by name.
    ///
    /// # Errors
    /// As [`TableConfig::describe`].
    pub async fn use_table_by_name(&self, fqn: &str) -> Result<(), ValaSdkError>;

    /// The active write binding, if any.
    #[must_use]
    pub fn table(&self) -> Option<TableConfig>;

    // ── write ──────────────────────────────────────────────────────────────

    /// Enqueue one JSON row into the active table, propagating queue-full.
    ///
    /// Synchronous in every language surface because enqueueing is a bounded,
    /// non-blocking channel send. The row is durable only after [`Self::flush`]
    /// or [`Self::shutdown`] resolves.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_412_NO_ACTIVE_TABLE` when no table is bound, or
    /// `WYRD_CLIENT_429_QUEUE_FULL` when the producer channel is saturated.
    pub fn insert(&self, row: Vec<u8>, correlation: Correlation) -> Result<(), ValaSdkError>;

    /// Flush every pooled producer and await each durable acknowledgement.
    ///
    /// # Errors
    /// Returns the first producer or sink failure, including the server's
    /// stable refusal of a sealed batch.
    pub async fn flush(&self) -> Result<(), ValaSdkError>;

    /// Drain every producer and stop its background task.
    ///
    /// # Errors
    /// As [`Self::flush`].
    pub async fn shutdown(&self) -> Result<(), ValaSdkError>;

    // ── read ───────────────────────────────────────────────────────────────

    /// Run one SQL SELECT and collect every batch.
    ///
    /// Any authorized table, not just the active one. Bounded by the server's
    /// query floor; use [`Self::stream`] for a result set larger than memory.
    ///
    /// # Errors
    /// Returns `WYRD_VALA_400_QUERY_INVALID_SQL`, the floor's non-SELECT or
    /// oversized refusal, or stable authentication, authorization,
    /// availability, or protocol errors.
    ///
    /// # Cancellation
    /// Abandoning the future abandons the request; the server cancels the query
    /// when the response body is dropped.
    pub async fn sql(&self, query: &str) -> Result<QueryResult, ValaSdkError>;

    /// Run one SQL SELECT and return its batches as they arrive.
    ///
    /// The stream owns the HTTP response body: dropping it propagates
    /// cancellation. The terminal frame is required, so a stream that ends
    /// without one raises `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE`.
    ///
    /// # Errors
    /// As [`Self::sql`], plus a decode error on a malformed Arrow IPC frame.
    pub async fn stream(&self, query: &str) -> Result<QueryResultStream, ValaSdkError>;

    /// Escape hatch to the full query surface: lifecycle controls, typed trace
    /// and GenAI reads, describe.
    #[must_use]
    pub fn query(&self) -> &QueryClient;
}

/// Optional per-row correlation. Both fields are optional on the wire: the
/// server stores an uncorrelated row against the authenticated `principal_id`
/// with a null `card_uid`.
#[derive(Debug, Clone, Default)]
pub struct Correlation {
    /// The Card this row belongs to, resolved server-side to a `card_uid`.
    pub card_ref: Option<CardRef>,
    /// The run that produced this row.
    pub run_id: Option<RunId>,
}
```

### 3.3 `blocking::Bifrost` — sync facade

```rust
/// The synchronous [`Bifrost`], for callers with no runtime of their own.
///
/// Every method blocks on the shared Wyrd runtime; the async client is the
/// implementation, not a parallel one. This mirrors `reqwest::blocking`, which
/// is already in the dependency tree and sets the reader's expectation.
///
/// # Panics
/// Constructing from inside an async context panics, as `reqwest::blocking`
/// does: blocking a runtime thread on its own runtime deadlocks.
pub struct Bifrost { inner: crate::Bifrost }

impl Bifrost {
    pub fn from_env() -> Result<Self, ValaSdkError>;
    pub fn connect(client: &WyrdClient) -> Result<Self, ValaSdkError>;
    pub fn connect_with_table(client: &WyrdClient, table: TableConfig) -> Result<Self, ValaSdkError>;
    pub fn register(&self) -> Result<RegisterOutcome, ValaSdkError>;
    pub fn use_table(&self, table: TableConfig) -> Option<TableConfig>;
    pub fn use_table_by_name(&self, fqn: &str) -> Result<(), ValaSdkError>;
    pub fn table(&self) -> Option<TableConfig>;
    pub fn insert(&self, row: Vec<u8>, correlation: Correlation) -> Result<(), ValaSdkError>;
    pub fn flush(&self) -> Result<(), ValaSdkError>;
    pub fn shutdown(&self) -> Result<(), ValaSdkError>;
    pub fn sql(&self, query: &str) -> Result<QueryResult, ValaSdkError>;
    /// Blocking `Iterator<Item = Result<RecordBatch, ValaSdkError>>`.
    pub fn stream(&self, query: &str) -> Result<BlockingQueryStream, ValaSdkError>;
}
```

### 3.4 `QueryResult`

```rust
/// A collected query result: the Arrow batches plus the terminal frame.
///
/// Held as decoded `RecordBatch`es rather than raw IPC so Rust callers do not
/// re-decode; the Python and TypeScript surfaces re-encode once at their
/// boundary, which is the only place a language-native table can be built.
pub struct QueryResult {
    batches: Vec<RecordBatch>,
    schema: SchemaRef,
    terminal: QueryTerminalFrame,
}

impl QueryResult {
    #[must_use] pub fn batches(&self) -> &[RecordBatch];
    #[must_use] pub fn schema(&self) -> &SchemaRef;
    #[must_use] pub fn terminal(&self) -> &QueryTerminalFrame;
    #[must_use] pub fn num_rows(&self) -> usize;
    /// Encode as one Arrow IPC stream.
    ///
    /// # Errors
    /// Returns a protocol error when IPC encoding fails.
    pub fn to_ipc(&self) -> Result<Vec<u8>, ValaSdkError>;
}
```

### 3.5 Client-Rust usage

```rust
use vala_sdk::blocking::Bifrost;
use vala_sdk::{Correlation, TableConfig};

#[derive(serde::Serialize, schemars::JsonSchema)]
struct Prediction { model: String, score: f64, tokens: i64 }

// no URL, no key: endpoints from WYRD_SERVER_URL / WYRD_GRPC_URL, credential
// from the chain whose floor is ~/.config/wyrd/credentials.toml
let bf = Bifrost::from_env()?;
bf.use_table(TableConfig::from_model::<Prediction>("genai.predictions")?);
bf.register()?;
bf.insert(serde_json::to_vec(&Prediction { .. })?, Correlation::default())?;
bf.flush()?;

// an explicit transport still wins, for a caller holding a scoped token
let client = WyrdClient::with_config(ClientConfig { api_key, ..Default::default() })?;
let bf = Bifrost::connect_with_table(&client, table)?;

// read — any table
let result = bf.sql("SELECT model, avg(score) FROM genai.predictions GROUP BY model")?;
for batch in bf.stream("SELECT * FROM genai.predictions")? { let batch = batch?; }

// join an existing table
bf.use_table_by_name("genai.evals")?;
bf.insert(row, Correlation { card_ref: Some(card_ref), run_id: None })?;
```

Async is the same names on `vala_sdk::Bifrost` with `.await`.

## 4. Python — `python/py-wyrd`

```python
class TableConfig:
    """One Bifrost table: its name, the columns a model declares, and the
    physical layout to request.

    The schema comes from a Pydantic model class (not an instance) via
    ``model_json_schema()``, mapped by the same Rust owner every language uses.
    Unlike Scouter, no fingerprint is computed here: the server mints it, and
    ``resolved`` is ``None`` until ``register()`` or ``TableConfig.describe()``.
    """

    def __init__(
        self,
        model: type[Any],
        table: str,                                   # "namespace.name"
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
    ) -> None: ...

    @staticmethod
    def from_arrow(schema: pyarrow.Schema, table: str) -> TableConfig:
        """Build from an explicit ``pyarrow.Schema`` for types JSON Schema
        cannot express (``int32``, non-UTC timestamps, ``decimal128``)."""

    @staticmethod
    def describe(
        table: str,
        server_url: str | None = None,
        credential: str | None = None,
    ) -> TableConfig:
        """Fetch an already-registered table's config by name.

        Args:
            table (str): the table name, ``"namespace.name"``
            server_url (str | None): the Bifrost server URL, e.g.
            ``"https://wyrd.example.com"``. Auto-resolved from fallbacks if not provided.
            credential (str | None): the API key or bearer token. Auto-resolved from fallbacks if not provided.

        """

    @property
    def fqn(self) -> str: ...
    @property
    def arrow_schema(self) -> pyarrow.Schema:
        """The user columns only; the server appends correlation and managed
        columns."""
    @property
    def resolved(self) -> ResolvedTable | None: ...


class ResolvedTable(TypedDict):
    table_uid: str
    fingerprint: str


class Correlation(TypedDict, total=False):
    card_ref: str
    run_id: str


class QueryResult:
    """Arrow batches from one query, converted on demand."""

    def to_arrow(self) -> pyarrow.Table: ...
    def to_polars(self) -> Any: ...
    def to_pandas(self) -> Any: ...
    def to_bytes(self) -> bytes: ...
    @property
    def terminal(self) -> QueryTerminal: ...
    def __len__(self) -> int: ...


class Bifrost:
    """Query any authorized table; write to the active one.

    Synchronous. Use ``AsyncBifrost`` for the ``await`` surface — the same
    methods on the same core, differing only in how they are driven.
    """

    def __init__(
        self,
        table: TableConfig | None = None,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Every argument is optional.

        Args:
            table (TableConfig | None): the active write target. Omitted, the
            client is query-only until ``use_table``.
            server_url (str | None): the Bifrost server URL, e.g.
            ``"https://wyrd.example.com"``. Auto-resolved from
            ``WYRD_SERVER_URL`` and then the compiled default if not provided.
            credential (str | None): the API key or bearer token. Auto-resolved
            through ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
            ``WYRD_API_KEY`` → ``~/.config/wyrd/credentials.toml``
            ``[default].api_key`` if not provided.
            grpc_url (str | None): the ingest endpoint. Auto-resolved from
            ``WYRD_GRPC_URL`` if not provided.

        Raises:
            NoCredentialsError: the credential chain yielded nothing.

        """

    # table lifecycle
    def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``."""
    def use_table(self, table: TableConfig) -> TableConfig | None: ...
    def use_table_by_name(self, table: str) -> None: ...
    @property
    def table(self) -> TableConfig | None: ...

    # write
    def insert(self, row: Any, correlation: Correlation | None = None) -> None:
        """Enqueue one row. Accepts a Pydantic model instance, a ``dict``, or a
        JSON string; a model is dumped with ``model_dump_json()``. Non-blocking;
        durable after ``flush()``."""
    def flush(self) -> None: ...
    def shutdown(self) -> None: ...

    # read
    def sql(self, query: str) -> QueryResult: ...
    def stream(self, query: str) -> Iterator[pyarrow.RecordBatch]: ...


class AsyncBifrost:
    """The ``await`` surface. Identical names; ``stream`` yields with
    ``async for``."""

    def __init__(self, table: TableConfig | None = None,
                 server_url: str | None = None,
                 credential: str | None = None,
                 grpc_url: str | None = None) -> None: ...
    async def register(self) -> str: ...
    def use_table(self, table: TableConfig) -> TableConfig | None: ...
    async def use_table_by_name(self, table: str) -> None: ...
    def insert(self, row: Any, correlation: Correlation | None = None) -> None: ...
    async def flush(self) -> None: ...
    async def shutdown(self) -> None: ...
    async def sql(self, query: str) -> QueryResult: ...
    def stream(self, query: str) -> AsyncIterator[pyarrow.RecordBatch]: ...
```

Usage:

```python
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig

class Prediction(BaseModel):
    model: str
    score: float
    tokens: int

# no URL, no key: both resolve from the environment and credentials.toml
bf = Bifrost(TableConfig(model=Prediction, table="genai.predictions"))
bf.register()

bf.insert(Prediction(model="claude-opus-5", score=0.94, tokens=128))
bf.flush()

df = bf.sql("SELECT model, avg(score) FROM genai.predictions GROUP BY model").to_polars()
for batch in bf.stream("SELECT * FROM genai.predictions"):
    ...

bf.use_table_by_name("genai.evals")
bf.insert({"suite": "regression", "passed": 41}, {"run_id": run_id})
```

Query-only:

```python
bf = Bifrost()
bf.sql("SELECT * FROM otel.traces LIMIT 100").to_pandas()

# an explicit transport still wins, for a caller holding a scoped token
bf = Bifrost(server_url="https://wyrd.example.com", credential=token)
```

## 5. TypeScript — `typescript/wyrd`

Async only. `insert` stays synchronous because enqueueing is a non-blocking
channel send, not IO — making it a Promise would imply a durability it does not
provide.

```ts
export interface SortKey { column: string; descending?: boolean; nullsLast?: boolean }

export interface TableLayout {
  partitionGranularity?: "hour" | "day" | "month";
  sortKeys?: SortKey[];
  bloomColumns?: string[];
}

export interface Correlation { cardRef?: string; runId?: string }

/**
 * One Bifrost table: its name, the columns a model declares, and the physical
 * layout to request. Build it from a JSON Schema document — `z.toJSONSchema()`
 * for Zod, or a literal — or fetch an existing table by name.
 */
export class TableConfig {
  static fromJsonSchema(
    table: string,
    schema: Readonly<Record<string, unknown>>,
    layout?: TableLayout,
  ): TableConfig;

  /** Transport fields auto-resolve when omitted, as `Bifrost.connect` does. */
  static async describe(
    table: string,
    transport?: { serverUrl?: string; credential?: string },
  ): Promise<TableConfig>;

  readonly fqn: string;
  /** The user columns only; correlation and managed columns are server-side. */
  readonly arrowSchema: Schema;
  readonly resolved?: { tableUid: string; fingerprint: string };
}

/** Arrow batches from one query, converted on demand. */
export class QueryResult {
  toArrow(): Table;
  toBytes(): Uint8Array;
  readonly terminal: QueryTerminal;
  readonly numRows: number;
}

/** Query any authorized table; write to the active one. */
export class Bifrost {
  /** Connecting performs IO, so this is a factory rather than a constructor. */
  static connect(options?: {
    table?: TableConfig;
    serverUrl?: string;
    credential?: string;
    grpcUrl?: string;
  }): Promise<Bifrost>;

  // table lifecycle
  register(): Promise<"created" | "already_exists">;
  useTable(table: TableConfig): TableConfig | undefined;
  useTableByName(table: string): Promise<void>;
  readonly table?: TableConfig;

  // write
  /** Enqueue one row. Synchronous and non-blocking; durable after `flush`. */
  insert(row: Readonly<Record<string, unknown>>, correlation?: Correlation): void;
  flush(): Promise<void>;
  shutdown(): Promise<void>;

  // read
  sql(query: string): Promise<QueryResult>;
  stream(query: string): AsyncIterableIterator<RecordBatch>;
}
```

Usage:

```ts
import { z } from "zod";
import { Bifrost, TableConfig } from "@wyrd/sdk";

const Prediction = z.object({ model: z.string(), score: z.number(), tokens: z.int() });

// serverUrl and credential resolve from the environment and credentials.toml
const bf = await Bifrost.connect({
  table: TableConfig.fromJsonSchema("genai.predictions", z.toJSONSchema(Prediction)),
});

await bf.register();
bf.insert({ model: "claude-opus-5", score: 0.94, tokens: 128 });
await bf.flush();

const result = await bf.sql("SELECT model, avg(score) FROM genai.predictions GROUP BY model");
for await (const batch of bf.stream("SELECT * FROM genai.predictions")) { }

await bf.useTableByName("genai.evals");
bf.insert({ suite: "regression", passed: 41 }, { runId });
```

## 6. Justification

**Why one class.** Read and write already share one credential, one
`WyrdClient`, and one connection pool. `AuthMiddleware` exchanges an API key for
a bearer transparently, so a "write credential" and a "read token" are the same
thing arriving in two shapes. Two classes made users construct the transport
twice to do one job.

**Why no `WyrdClient` root in Python/TS.** Both Scouter and Wyrd's current
Python construct `Bifrost` directly. A root factory is justified only by sharing
one connection across several subsystem clients, and there is one subsystem. The
sharing case that does exist — two writers on one endpoint — is solvable inside
the SDK, because `ClientScope` is already keyed on
`(server_url, credential_fingerprint)` and the producer pool on
`(scope, kind, table)`. That is an internal cache, invisible either way, and not
worth building until someone hits it.

**Why nothing on this surface requires a URL or a key.** Endpoints and
credentials already resolve themselves. `ClientConfig::from_env` reads
`WYRD_SERVER_URL` / `WYRD_GRPC_URL` and falls back to compiled defaults;
`ClientConfig::resolve_credential`
(`crates/shared/wyrd-client/src/config.rs:105`) walks `WYRD_ACCESS_TOKEN` →
`WYRD_WORKLOAD_TOKEN` + tenant → `WYRD_API_KEY` →
`~/.config/wyrd/credentials.toml` `[default].api_key`
(`transport/credential.rs:229`). Every current SDK constructor demands a URL and
a key positionally and so bypasses all of it: a user with a working
`credentials.toml` still has to read the key out of the file and hand it back.
Making them optional costs nothing and deletes the most common line of setup.
Resolution happens once at construction, not per call, and an explicitly
supplied value still wins.

**Why `TableConfig` and not per-row schema.** Today's `insert(table, schema,
row, ...)` takes the schema on every call and ignores it after the first. The
config makes the binding real, gives `register()` something to register, and
gives `describe` somewhere to land.

**Why no client-side fingerprint.** Scouter computes one to compare against the
server's. Wyrd's server already returns `RegisterTableResponse.fingerprint` and
answers `AlreadyExists` for a match, so a client fingerprint is a second
authority for one identity — it drifts the moment either mapping changes, and it
buys nothing the register round-trip does not already prove. `resolved` is
`None` until the server fills it.

**Why the schema mapping is not reimplemented.** `wyrd_queue::json_schema_to_arrow`
and `wyrd_queue::schema::writable_schema` (`crates/shared/wyrd-queue/src/schema.rs:75`)
already own the JSON-Schema → Arrow mapping and the
`user_fields → correlation_fields → managed_candidates` layout rule, and are
already shared by `vala-sdk/src/python.rs:396`, `wyrd-node/src/lib.rs:586`, and
`query.rs:1851`. `TableConfig` calls them. No language port, in either
direction.

**Why sync and async are separate structs, not dual methods.** A struct with
both `fn sql` and `async fn sql_async` is not idiomatic Rust and doubles the
surface for one behavior. `reqwest::blocking` — already in the tree — is the
pattern: the async client is the implementation, the blocking one is a thin
`block_on` on the shared Wyrd runtime. Python mirrors it with
`Bifrost`/`AsyncBifrost`. The *default-named* class is each language's default
idiom: sync in Python, async in Rust and TypeScript. That is deliberate, not
inconsistency.

**Why `sql` and `stream` both exist.** Every mainstream OLAP client ships both —
DuckDB, ClickHouse, Snowflake, BigQuery, Databricks. `sql` is the answer for a
result that fits in memory; `stream` is the answer for one that does not, and it
is the only way to bound peak memory on a large scan. Collapsing them would
force every caller onto the iterator ceremony or onto an unbounded buffer.

**Why writes bind one table.** `SealedBatch` carries exactly one table and one
Arrow schema; the producer pool is keyed on it. Multiplexing writes inside one
`insert` call would mean either a table argument on every row (what we are
deleting) or a hidden schema switch per row. Swapping is explicit and cheap:
`use_table` builds nothing, and the previous table's producer stays pooled so
its buffered rows still flush.

**Why there is no context manager.** A Bifrost client is an application-lifetime
object — built once at startup, held for the process — not a scoped resource, so
`with Bifrost(...) as bf:` would put a shutdown at the end of a block that is
almost never where the client's life ends. Nor is the block buying safety:
dropping a producer handle already drains its buffered rows through
`drain_dropped_handle` (`crates/shared/wyrd-queue/src/producer.rs:945`), and the
background task seals on its own timer regardless. `shutdown()` stays explicit
for the caller who wants to block until every batch is acknowledged. TypeScript
drops `AsyncDisposable` for the same reason, so the three surfaces keep one
shape.

**Why there is one write method, not two.** `insert` and `record` differed in
exactly one thing: what happens on a full queue. The queue already drains on its
own — `flush_interval_ms: 1000`, `flush_max_rows: 50_000`, a 1024-row channel
over 4096 staging slots — so `WYRD_CLIENT_429_QUEUE_FULL` does not mean "you
wrote fast", it means the channel, the staging slots, and the 32 MiB byte budget
are all occupied while the sink is still behind. That is sustained overload and
worth telling the caller about. Note that `Producer::enqueue` is `try_send`
(`crates/shared/wyrd-queue/src/producer.rs:679`): the queue is *bounded*, and
refuses rather than waiting, so the policy decision genuinely belongs to the
caller. `record` existed only to make that decision silently, and the one caller
that wants a dropped row — telemetry — already owns `observe::record`.

**Why `card_ref` and `run_id` are optional.** The server has always allowed it —
`crates/wyrd-spec/src/vala/api.rs:117` states an omitted `card_ref` stores the
row against the authenticated `principal_id` with a null `card_uid`. Only the
client forced it. `Correlation` defaults to both absent.

## 7. Delete list

| Delete | Location | Replaced by |
|---|---|---|
| `QueryClient` as a public root | `vala-sdk/src/query.rs:199` | stays public for lifecycle/typed reads via `Bifrost::query()` |
| `BifrostQueryClient` | `python/py-wyrd/python/wyrd/bifrost/__init__.py:106` | `Bifrost` / `AsyncBifrost` |
| `BifrostQueryClient.writable_schema` | `bifrost/__init__.py:152` | folded into `TableConfig.describe` |
| `BifrostClient`, `BifrostQueryClient`, `WyrdClient.bifrost` | `typescript/wyrd/src/index.ts:470,678,684` | `Bifrost` |
| `BifrostWrite.schema` / `.cardRef` | `index.ts:592` | `TableConfig` + optional `Correlation` |
| `schema` and `card_ref` params on `insert` | `handle.rs:159`, `python.rs:108`, `wyrd-node/src/lib.rs` | `TableConfig` + `Correlation` |
| `SinkKind` from the public signature | `handle.rs:159` | internal; the client only writes `SinkKind::Record` |
| `Bifrost::record` and `Bifrost::dropped` from every SDK | `handle.rs`, `python.rs`, `index.ts:625` | one `insert` that returns the refusal; `observe::record` keeps the drop-on-full path |
| `schema_from_json_schema` as public SDK API | `handle.rs:305` | private inside `TableConfig` |
| `vala_sdk::Bifrost` as the *write pool* name | `handle.rs:35` | private `WriterPool`; the name goes to the facade |

Not deleted: `IngestTransport::insert_batch`, `BifrostIngestServiceClient::insert_batch`,
`Gate::insert_batch`, `validate_frame`, `validate_batch_id`, `wyrd_queue`'s
schema owners.

## 7a. Existing client tests

Every client test is re-judged against the new surface. A test that passes only
because it was written around the old signatures is not evidence, and a test
built on a hand-rolled fake of the native layer never was.

| File | Tests | Verdict |
|---|---|---|
| `python/py-wyrd/tests/bifrost/test_query.py` | 11 | **Delete.** Six fake classes (`_NativeStream`, three `NativeClient`s) and `object.__new__(BifrostQueryClient)` + `client._native = …`, which bypasses the constructor to inject them. It asserts the projection layer agrees with a fake, not that the client works. The real behaviors buried in it — stream cancellation waiting for cleanup, a decode failure closing the native stream without masking the error, a missing terminal frame failing closed — move to journeys against `WyrdTestServer`, or to `vala-sdk` Rust unit tests where the stream state machine actually lives. |
| `python/py-wyrd/tests/test_bifrost.py` | 3 | **Rewrite one.** `test_bifrost_rejects_empty_configuration` asserts `Bifrost("", "secret")` raises `server_url must not be empty`; under the new contract an omitted URL auto-resolves, so the test asserts the opposite of intended behavior. Replace with: no argument and no resolvable credential raises `NoCredentialsError`. The submodule-import and `ProducerKey`/`ClientScope`-not-importable tests stay — those boundaries do not move. |
| `python/py-wyrd/tests/bifrost/test_public_typing.py` | 2 | **Rewrite.** A real static fixture the `py:typecheck` lane reads; keep the technique, retarget it at `TableConfig`, `Correlation`, and `QueryResult`. |
| `python/py-wyrd/tests/integration/test_bifrost_e2e.py` | 12 | **Rewrite onto the new surface.** Already real journeys against `WyrdTestServer`. Every `Bifrost(url, key)` + per-row `schema=`/`card_ref=` call site changes; the assertions mostly survive. |
| `python/py-wyrd/tests/integration/test_bifrost_query.py` | 5 | **Rewrite onto the new surface.** Real journeys; `BifrostQueryClient(...)` becomes `Bifrost(...)`. |
| `crates/vala/vala-sdk/src/lib.rs` | 11 | **Keep, adjust signatures.** Stall-sink and pooled-producer tests that assert real behavior: scope collapse, producer cap before registry growth, queue-full propagation, drain-all-producers-after-first-error. Only the `insert` signature changes. |
| `crates/vala/vala-sdk/src/query.rs` | 21 | **Keep, delete one.** Most run against a real local `TcpListener`, which is legitimate. `running_query_client_projects_canonical_contract` (`query.rs:3079`) `include_str!`s its own file and asserts the source text contains `"pub async fn running(&self)"` — it proves nothing about behavior, breaks on any rename, and must be deleted rather than updated. |
| `crates/vala/vala-sdk/src/handle.rs` | 0 | **Gap.** The write pool has no direct unit coverage; the new `Bifrost` facade needs active-table binding, swap-keeps-draining, and no-active-table tests. |
| `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs` | — | **Rewrite onto the new surface.** The real Rust journey file. |
| `typescript/wyrd/tests/unit/bifrost-query.test.ts` | 4 suites | **Delete the `BifrostClient` suites, keep the typing one.** `BifrostClient` and `BifrostQueryClient` are being deleted; suites written against fake natives go with them. |
| `typescript/wyrd/tests/integration/*.test.ts` | 2 files | **Rewrite onto the new surface.** Real journeys. |

Two patterns not to reintroduce: a hand-rolled stand-in for the native
extension, and a test that inspects source text instead of running the code.

## 8. Task breakdown

| # | Task | Surface | Depends on |
|---|---|---|---|
| T1 | `TableConfig` + `Correlation` + `ResolvedTable` in `vala-sdk`; `register` / `describe` HTTP calls | Rust core | — |
| T2 | Make `card_ref` optional through `Producer::enqueue` and the Gate correlation column | `wyrd-queue`, `vala-bifrost-redux` | — |
| T3 | Rename `Bifrost` → `WriterPool`; new `Bifrost` facade over `QueryClient` + pool + active table; `QueryResult` | Rust core | T1, T2 |
| T4 | `vala_sdk::blocking::Bifrost` + `BlockingQueryStream` on the shared runtime | Rust core | T3 |
| T5 | PyO3: `Bifrost`, `AsyncBifrost`, `TableConfig`, `QueryResult`; delete `BifrostQueryClient`; regenerate stubs | Python | T4 |
| T6 | napi + `index.ts`: `Bifrost`, `TableConfig`, `QueryResult`; delete `BifrostClient`/`WyrdClient`; regenerate `.d.ts` | TypeScript | T3 |
| T1b | Optional transport on every signature that takes one: `Bifrost::from_env`, `TableConfig::describe_from_env`, optional Python/TypeScript arguments | all three | T1 |
| T7 | User journeys per language: register → insert → flush → sql/stream → swap table → read again; negatives (no active table, fingerprint conflict, under-privileged token, non-SELECT floor rejection, no resolvable credential) | all three | T5, T6 |
| T8 | Migrate `wyrd-testing::BifrostWriter` and the 25 Bifrost journey call sites onto the new surface | Rust tests | T3 |
| T9 | Execute §7a: delete `tests/bifrost/test_query.py` and the `query.rs` source-grep test, rewrite `test_bifrost.py`'s empty-configuration assertion, retarget the typing fixtures, port the four journey files, and cover the `handle.rs` gap | all three | T5, T6 |

## 8a. Test intent this redesign must prove

Every one of these is a user-observable behavior the current tests either fake
or do not reach:

- Constructing with no arguments resolves a credential and writes.
- Constructing with no argument and no resolvable credential raises, naming the
  chain it tried.
- An explicit URL or key still overrides a resolvable one.
- `insert` before any `use_table` refuses, rather than writing somewhere.
- `register` twice is idempotent; registering a changed schema conflicts.
- `use_table` mid-stream does not lose the previous table's buffered rows.
- `insert` with no `card_ref` writes a row the server correlates to the
  authenticated principal.
- A queue-full refusal reaches the caller as a stable error, not a silent drop.
- `sql` and `stream` return the same rows for the same query.

## 9. Open questions

1. **`WYRD_VALA_412_NO_ACTIVE_TABLE`** is a new error code. Confirm the catalog
   entry and status, or reuse `WYRD_SPEC_400_VALIDATION`.
2. **T2 blast radius**: is a null `card_uid` already accepted end-to-end
   through Gate, Scribe, and Forge, or is the optionality only documented?
3. **No `config.toml` exists.** Credentials have a file floor
   (`~/.config/wyrd/credentials.toml`); endpoints do not. There is no
   `GlobalConfig`, no `wyrd_config_dir()`, and no `WyrdClient::from_global()` in
   this tree, so `WYRD_SERVER_URL` / `WYRD_GRPC_URL` are the only non-default
   endpoint source and no "global config beats environment" precedence exists.
   A `~/.config/wyrd/config.toml` holding `grpc_url`, `http_url`, `tenant`, and
   token-cache settings is net-new work in `wyrd-client`, not something this
   redesign can assume. In scope? If so it is a prerequisite of T1b and its
   precedence against the environment needs deciding. (`crates/wyrd/wyrd-config`
   is unrelated: it owns repository `wyrd.toml` Card-authoring defaults, and its
   schema cannot hold a URL, tenant, or credential.)
4. **`observe::record`** currently takes an explicit table per call and stays a
   free function, so telemetry keeps its own table argument rather than the
   client's active binding. Confirm that is the intent, given it now reads
   differently from the data path.

## 10. Skipped

- `read(limit)` — typed model-instance readback of the active table (Scouter
  has it). `sql("SELECT * FROM <fqn> LIMIT n")` covers it; add when users ask
  for model validation on the read side.
- `list_datasets` / `describe_dataset` as first-class `Bifrost` methods — they
  live on `Bifrost::query()` until a journey needs them at the top level.
- Multi-table writers, a `WyrdClient` root, and a shared-connection cache — add
  when someone writes to two tables and measures the cost.
- Turning the queue's bound into real backpressure (a bounded wait on a full
  channel instead of `try_send`'s instant refusal). It would make
  `WYRD_CLIENT_429_QUEUE_FULL` mean "the sink is dead" rather than "you wrote
  fast", but `blocking_send` panics inside a Tokio runtime, so the async
  surfaces need `send().await` and the sync ones do not. Separate change from
  this redesign.
