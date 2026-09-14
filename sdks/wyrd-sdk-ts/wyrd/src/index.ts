import {
  Table,
  tableFromIPC,
  tableToIPC,
  type RecordBatch,
  type Schema,
} from "apache-arrow";
import { createRequire } from "node:module";

import type { WyrdErrorCode } from "./error-codes.js";

import type {
  NativeBifrostQueryStream,
  NativeLifecycleResult,
  NativeQueryRequest,
  NativeQueryStep,
} from "../index.cjs";

const require = createRequire(import.meta.url);
const nativeBinding = require("../index.cjs") as typeof import("../index.cjs");
const {
  connectBifrost,
  connectCards,
  describeTableConfig,
  openWyrdState,
  tableConfigFromJsonSchema,
} = nativeBinding;
type NativeBifrost = import("../index.cjs").NativeBifrost;
type NativeCards = import("../index.cjs").NativeCards;
type NativeWyrdState = import("../index.cjs").NativeWyrdState;
type NativeTableConfig = import("../index.cjs").NativeTableConfig;

export type VisibilityMode = "published_only" | "fused";
export type FreshnessPolicy = "strict" | "allow_degraded";

export interface BifrostQueryRequest {
  sql: string;
  visibility?: VisibilityMode;
  freshness?: FreshnessPolicy;
  deadlineMs?: number;
}

/**
 * One column's logical type: a scalar variant name, or a single-key object for
 * a parameterized form. `List` and `Struct` carry full field declarations,
 * which is what makes the contract recursive.
 */
export type DataTypeSpec =
  | string
  | { readonly FixedSizeBinary: Readonly<Record<string, number>> }
  | { readonly Timestamp: Readonly<Record<string, string | null>> }
  | { readonly Time32: Readonly<Record<string, string>> }
  | { readonly Time64: Readonly<Record<string, string>> }
  | { readonly Decimal128: Readonly<Record<string, number>> }
  | { readonly List: FieldDescription }
  | { readonly Struct: readonly FieldDescription[] };

/**
 * One column declaration in a table description, exactly as stored.
 *
 * `metadata` carries the Arrow field metadata the physical schema holds,
 * including the stable `PARQUET:field_id`. The server omits it when empty.
 */
export interface FieldDescription {
  readonly name: string;
  readonly data_type: DataTypeSpec;
  readonly nullable: boolean;
  readonly metadata?: Readonly<Record<string, string>>;
}

/** The lightweight table identity shared by the list and describe routes. */
export interface TableEntry {
  readonly namespace: string;
  readonly name: string;
  readonly table_uid: string;
  readonly status: string;
  readonly fingerprint: string;
  readonly registered_at: string;
  readonly updated_at: string;
}

/** One resolved sort key of a table's physical layout. */
export interface SortKey {
  readonly column: string;
  readonly direction: string;
  readonly null_order: string;
}

/** A table's server-resolved partitioning, sort order, and Bloom columns. */
export interface PhysicalLayout {
  readonly partition_granularity: string;
  readonly sort_keys: readonly SortKey[];
  readonly bloom_columns: readonly string[];
}

/**
 * The server's projection of one registered table's stored physical schema.
 *
 * A writer declares `user_fields`, supplies `correlation_fields`, and may
 * supply `managed_candidates`. `canonical_physical_fingerprint` is present only
 * for a canonical signal table.
 */
export interface TableDescription {
  readonly entry: TableEntry;
  readonly user_fields: readonly FieldDescription[];
  readonly correlation_fields: readonly FieldDescription[];
  readonly managed_candidates: readonly FieldDescription[];
  readonly canonical_physical_fingerprint?: string;
  readonly physical_layout: PhysicalLayout;
}

export interface RunningQueryProgress {
  readonly completedParticipants: number;
  readonly totalParticipants: number;
}

export interface RunningQuery {
  readonly requestId: string;
  readonly queryClass: "interactive" | "analytical";
  readonly startedAt: string;
  readonly deadline: string;
  readonly state: "admitted" | "running" | "cancelling";
  readonly progress: RunningQueryProgress;
  readonly cancellationRequested: boolean;
}

export interface CancelRunningQueryResult {
  readonly requestId: string;
  readonly cancellationStarted: boolean;
}

interface RunningQueryWire {
  readonly request_id: string;
  readonly query_class: "interactive" | "analytical";
  readonly started_at: string;
  readonly deadline: string;
  readonly state: "admitted" | "running" | "cancelling";
  readonly progress: {
    readonly completed_participants: number;
    readonly total_participants: number;
  };
  readonly cancellation_requested: boolean;
}

interface CancelRunningQueryWire {
  readonly request_id: string;
  readonly cancellation_started: boolean;
}

export interface QueryTerminal {
  outcome: "success" | "degraded" | "failed";
  freshness: string;
  row_count: number;
  warnings: unknown[];
  source_completion: unknown[];
  error?: unknown;
  /**
   * Arrow IPC end-of-stream delta closing the query's single IPC stream.
   *
   * Present and non-empty for a successful or degraded query, and empty for a
   * failed one, which never closes its stream. A caller distinguishing a
   * complete result from a truncated one reads this rather than inferring
   * completeness from an absent batch.
   */
  arrow_ipc_eos: number[];
}

export type { WyrdErrorCode } from "./error-codes.js";

export class WyrdError extends Error {
  readonly code: WyrdErrorCode;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly remediation: string | undefined;
  /** JSON-safe structured diagnostics supplied by the originating Wyrd error. */
  readonly details: unknown;

  constructor(
    code: WyrdErrorCode,
    status: number,
    title: string,
    detail: string,
    remediation?: string,
    details?: unknown,
  ) {
    super(detail);
    this.name = "WyrdError";
    this.code = code;
    this.status = status;
    this.title = title;
    this.detail = detail;
    this.remediation = remediation;
    this.details = details;
  }
}

export class IncompleteQueryStreamError extends WyrdError {
  constructor(
    status: number,
    title: string,
    detail: string,
    remediation?: string,
    details?: unknown,
  ) {
    super(
      "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
      status,
      title,
      detail,
      remediation,
      details,
    );
    this.name = "IncompleteQueryStreamError";
  }
}

interface NativeErrorMetadata {
  errorCode?: string | null;
  errorStatus?: number | null;
  errorTitle?: string | null;
  errorDetail?: string | null;
  errorRemediation?: string | null;
  errorDetailsJson?: string | null;
}

function projectedError(metadata: NativeErrorMetadata): WyrdError | undefined {
  if (metadata.errorCode === null || metadata.errorCode === undefined) {
    return undefined;
  }
  const detail = metadata.errorDetail ?? "Bifrost query failed";
  const status = metadata.errorStatus ?? 500;
  const title = metadata.errorTitle ?? "Bifrost query failed";
  const remediation = metadata.errorRemediation ?? undefined;
  const details = metadata.errorDetailsJson == null
    ? undefined
    : JSON.parse(metadata.errorDetailsJson) as unknown;
  if (metadata.errorCode === "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE") {
    return new IncompleteQueryStreamError(
      status,
      title,
      detail,
      remediation,
      details,
    );
  }
  return new WyrdError(
    // The native projection only emits codes from the same derive-backed catalog.
    metadata.errorCode as WyrdErrorCode,
    status,
    title,
    detail,
    remediation,
    details,
  );
}

function lifecycleValue<T>(result: NativeLifecycleResult): T {
  const error = projectedError(result);
  if (error !== undefined) {
    throw error;
  }
  if (result.valueJson === null || result.valueJson === undefined) {
    throw new WyrdError(
      "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
      502,
      "Query lifecycle protocol failed",
      "native lifecycle control returned neither a value nor structured error",
    );
  }
  return JSON.parse(result.valueJson) as T;
}

/** Unwrap one closed native construction result, throwing its catalog error. */
function nativeHandle<T>(
  value: T | null | undefined,
  error: NativeErrorMetadata | null | undefined,
): T {
  const projected = error == null ? undefined : projectedError(error);
  if (projected !== undefined) {
    throw projected;
  }
  if (value === null || value === undefined) {
    throw new WyrdError(
      "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
      502,
      "Native construction protocol failed",
      "native construction returned neither a value nor structured error",
    );
  }
  return value;
}

function runningQuery(wire: RunningQueryWire): RunningQuery {
  return {
    requestId: wire.request_id,
    queryClass: wire.query_class,
    startedAt: wire.started_at,
    deadline: wire.deadline,
    state: wire.state,
    progress: {
      completedParticipants: wire.progress.completed_participants,
      totalParticipants: wire.progress.total_participants,
    },
    cancellationRequested: wire.cancellation_requested,
  };
}

async function closeNative(native: NativeBifrostQueryStream): Promise<void> {
  try {
    await native.close();
  } catch {
    // Preserve the original projection error; native cleanup is best effort.
  }
}

export class BifrostQueryStream
  implements AsyncIterableIterator<RecordBatch>
{
  readonly #native: NativeBifrostQueryStream;
  #terminal: QueryTerminal | undefined;
  #done = false;

  constructor(native: NativeBifrostQueryStream) {
    this.#native = native;
  }

  get terminal(): QueryTerminal | undefined {
    return this.#terminal;
  }

  get requestId(): string {
    return this.#native.requestId;
  }

  /**
   * The result's authoritative schema, retained once the stream completes.
   *
   * The server sends it before any batch, so it is the schema of a zero-row
   * result too — which is the whole reason it is read here rather than
   * reconstructed from whatever batches arrived.
   */
  get schema(): Schema | undefined {
    const ipc = this.#native.schemaIpc;
    return ipc === null || ipc === undefined
      ? undefined
      : tableFromIPC(ipc).schema;
  }

  [Symbol.asyncIterator](): AsyncIterableIterator<RecordBatch> {
    return this;
  }

  async next(): Promise<IteratorResult<RecordBatch>> {
    if (this.#done) {
      return { done: true, value: undefined };
    }
    let step: NativeQueryStep;
    try {
      step = await this.#native.next();
    } catch (error) {
      this.#done = true;
      await closeNative(this.#native);
      throw error;
    }
    const error = projectedError(step);
    if (error !== undefined) {
      this.#done = true;
      throw error;
    }
    if (step.ipc !== null && step.ipc !== undefined) {
      let batch: RecordBatch | undefined;
      try {
        batch = tableFromIPC(step.ipc).batches[0];
      } catch (error) {
        this.#done = true;
        await closeNative(this.#native);
        throw error;
      }
      if (batch === undefined) {
        this.#done = true;
        await closeNative(this.#native);
        throw new WyrdError(
          "WYRD_VALA_502_QUERY_STREAM_PROTOCOL",
          502,
          "Query stream protocol failed",
          "native Arrow payload contained no record batch",
        );
      }
      return { done: false, value: batch };
    }
    if (step.terminalJson === null || step.terminalJson === undefined) {
      this.#done = true;
      await closeNative(this.#native);
      throw new IncompleteQueryStreamError(
        502,
        "Query stream incomplete",
        "query stream ended without terminal metadata",
      );
    }
    try {
      this.#terminal = JSON.parse(step.terminalJson) as QueryTerminal;
    } catch (error) {
      this.#done = true;
      await closeNative(this.#native);
      throw error;
    }
    this.#done = true;
    return { done: true, value: undefined };
  }

  async return(): Promise<IteratorResult<RecordBatch>> {
    this.#done = true;
    await this.#native.close();
    return { done: true, value: undefined };
  }
}

/**
 * The physical layout a table asks the server to create.
 *
 * Sort keys reuse {@link SortKey} — the same shape the describe route returns —
 * so a layout read back from the server can be declared again unchanged.
 */
export interface TableLayout {
  partitionGranularity?: "hour" | "day";
  sortKeys?: readonly SortKey[];
  bloomColumns?: readonly string[];
}

/** Optional per-row correlation; an omitted field is a null on the wire. */
export interface Correlation {
  cardRef?: string;
  runId?: string;
}

/** The server-minted identity of a registered table. */
export interface ResolvedTable {
  readonly tableUid: string;
  readonly fingerprint: string;
}

function layoutJson(layout?: TableLayout): string | undefined {
  if (layout === undefined) {
    return undefined;
  }
  return JSON.stringify({
    partition_granularity: layout.partitionGranularity ?? "hour",
    sort_keys: layout.sortKeys ?? [],
    bloom_columns: layout.bloomColumns ?? [],
  });
}

/**
 * Anything that can describe itself as JSON Schema.
 *
 * Zod 4 schemas satisfy this by construction; the shape is named structurally
 * so {@link TableConfig.fromJsonSchema} accepts one without this SDK depending
 * on Zod, or on any other schema library.
 */
export interface JsonSchemaSource {
  toJSONSchema(): Readonly<Record<string, unknown>>;
}

/**
 * One Bifrost table: its name, the columns a model declares, and the physical
 * layout to request.
 *
 * Build it from a JSON Schema document — a Zod 4 schema, a literal, or any
 * peer that describes itself the same way — or fetch an existing table by
 * name. No fingerprint is computed here: the server mints it, so `resolved` is
 * undefined until the table is registered or described.
 */
export class TableConfig {
  readonly #native: NativeTableConfig;

  private constructor(native: NativeTableConfig) {
    this.#native = native;
  }

  /** @internal Wrap one config the native client handed back. */
  static fromNative(native: NativeTableConfig): TableConfig {
    return new TableConfig(native);
  }

  /** @internal The value the native client reads back. */
  get native(): NativeTableConfig {
    return this.#native;
  }

  /**
   * Declare a table's columns from JSON Schema, or from a schema that emits it.
   *
   * A Zod 4 schema carries its own `toJSONSchema()`, so passing one directly is
   * the same one-step declaration Pydantic gives the Python client. The check
   * is structural rather than an `instanceof`, so no schema library is a
   * dependency of this SDK and any peer offering the same method works
   * unchanged.
   *
   * @throws when the resulting document does not map to an Arrow schema, or
   * declares a column the write path already owns.
   */
  static fromJsonSchema(
    table: string,
    schema: Readonly<Record<string, unknown>> | JsonSchemaSource,
    layout?: TableLayout,
  ): TableConfig {
    const document =
      typeof (schema as JsonSchemaSource).toJSONSchema === "function"
        ? (schema as JsonSchemaSource).toJSONSchema()
        : schema;
    return new TableConfig(
      tableConfigFromJsonSchema(table, JSON.stringify(document), layoutJson(layout)),
    );
  }

  /**
   * Fetch an already-registered table's config by name.
   *
   * Transport fields auto-resolve when omitted, exactly as `Bifrost.connect`
   * does.
   */
  static async describe(
    table: string,
    transport: {
      readonly serverUrl?: string;
      readonly credential?: string;
      readonly grpcUrl?: string;
    } = {},
  ): Promise<TableConfig> {
    const described = await describeTableConfig(
      table,
      transport.serverUrl,
      transport.credential,
      transport.grpcUrl,
    );
    return new TableConfig(nativeHandle(described.config, described.error));
  }

  /** `namespace.name` — the name SQL and the ingest batch both use. */
  get fqn(): string {
    return this.#native.fqn;
  }

  /**
   * The declared user columns only; correlation and managed columns are
   * appended by the write path and the server.
   */
  get arrowSchema(): Schema {
    return tableFromIPC(new Uint8Array(this.#native.schemaIpc)).schema;
  }

  /** The server-assigned identity, or undefined while unregistered. */
  get resolved(): ResolvedTable | undefined {
    const resolved = this.#native.resolvedJson;
    if (resolved === null || resolved === undefined) {
      return undefined;
    }
    const wire = JSON.parse(resolved) as {
      table_uid: string;
      fingerprint: string;
    };
    return { tableUid: wire.table_uid, fingerprint: wire.fingerprint };
  }
}

/**
 * Anything that validates one row and returns it typed.
 *
 * A Zod schema satisfies this by construction. The shape is structural, so no
 * schema library is a dependency of this SDK and any peer offering `parse`
 * works unchanged — the same reason {@link TableConfig.fromJsonSchema} accepts
 * a structural JSON Schema source.
 */
export interface RowSchema<T> {
  parse(value: unknown): T;
}

/**
 * Arrow batches from one query, converted on demand.
 *
 * Collected by draining {@link Bifrost.stream}, so a collected result and a
 * streamed one are the same rows read the same way — only the batches are
 * retained rather than yielded.
 */
export class QueryResult {
  readonly #batches: readonly RecordBatch[];
  readonly #terminal: QueryTerminal;
  readonly #schema: Schema;

  constructor(
    batches: readonly RecordBatch[],
    terminal: QueryTerminal,
    schema: Schema,
  ) {
    this.#batches = batches;
    this.#terminal = terminal;
    this.#schema = schema;
  }

  /** The server-supplied result schema, present even with no batches. */
  get schema(): Schema {
    return this.#schema;
  }

  /** Every batch the query produced, in arrival order. */
  get batches(): readonly RecordBatch[] {
    return this.#batches;
  }

  /** The validated terminal frame the server closed the stream with. */
  get terminal(): QueryTerminal {
    return this.#terminal;
  }

  /** Total rows across every batch. */
  get numRows(): number {
    return this.#batches.reduce((total, batch) => total + batch.numRows, 0);
  }

  /**
   * The whole result as one Apache Arrow `Table`.
   *
   * A view over the batches this result already holds, under the schema the
   * server sent — no copy, no re-decode, and no second schema mapping — so a
   * caller reaching for columns, `toArray`, or `get` uses the same Arrow
   * implementation the batches were decoded with, and a zero-row result still
   * reports its selected fields.
   */
  toArrow(): Table {
    return new Table(this.#schema, [...this.#batches]);
  }

  /**
   * The whole result encoded as one Arrow IPC stream.
   *
   * The handoff format for anything outside this process — a file, another
   * Arrow runtime, a worker — and the exact bytes {@link QueryResult.toArrow}
   * represents, because both are built from the same retained batches.
   */
  toBytes(): Uint8Array {
    return tableToIPC(this.toArrow(), "stream");
  }
}

/**
 * The one Bifrost client: query any authorized table, write to the active one.
 *
 * Rows are batched, so a write is durable only once {@link Bifrost.flush} or
 * {@link Bifrost.shutdown} resolves. `insert` is synchronous because enqueueing
 * is a bounded, non-blocking operation that propagates queue-full to the
 * caller; the drains and reads are async because they wait on the server.
 */
export class Bifrost {
  readonly #native: NativeBifrost;

  private constructor(native: NativeBifrost) {
    this.#native = native;
  }

  /**
   * Connect one client, optionally already bound to a write target.
   *
   * Connecting performs IO, so this is a static factory rather than a
   * constructor. Every option is optional: `serverUrl` resolves from
   * `WYRD_SERVER_URL`, `grpcUrl` from `WYRD_GRPC_URL`, and `credential`
   * through `WYRD_ACCESS_TOKEN` → `WYRD_WORKLOAD_TOKEN` + tenant →
   * `WYRD_API_KEY` → `~/.config/wyrd/credentials.toml`.
   */
  static async connect(
    options: {
      readonly table?: TableConfig;
      readonly serverUrl?: string;
      readonly credential?: string;
      readonly grpcUrl?: string;
    } = {},
  ): Promise<Bifrost> {
    const connection = await connectBifrost(
      options.table?.native,
      options.serverUrl,
      options.credential,
      options.grpcUrl,
    );
    return new Bifrost(nativeHandle(connection.bifrost, connection.error));
  }

  /** Create the active table; resolves to `created` or `already_exists`. */
  async register(): Promise<"created" | "already_exists"> {
    return lifecycleValue<"created" | "already_exists">(
      await this.#native.register(),
    );
  }

  /**
   * Bind `table` as the write target, returning the previous binding.
   *
   * The previous table's producer stays pooled, so its buffered rows still
   * flush; a swap loses nothing.
   */
  useTable(table: TableConfig): TableConfig | undefined {
    const previous = this.#native.useTable(table.native);
    return previous === null || previous === undefined
      ? undefined
      : TableConfig.fromNative(previous);
  }

  /** Bind an already-registered table by name, describing it first. */
  async useTableByName(table: string): Promise<void> {
    lifecycleValue<null>(await this.#native.useTableByName(table));
  }

  /** The active write binding, if any. */
  get table(): TableConfig | undefined {
    const native = this.#native.table;
    return native === null || native === undefined
      ? undefined
      : TableConfig.fromNative(native);
  }

  /**
   * Enqueue one row into the active table.
   *
   * Synchronous and non-blocking; durable after {@link Bifrost.flush}. A
   * saturated queue throws the stable refusal rather than dropping the row.
   */
  insert(
    row: Readonly<Record<string, unknown>>,
    correlation: Correlation = {},
  ): void {
    lifecycleValue<null>(
      this.#native.insert(
        JSON.stringify(row),
        correlation.cardRef,
        correlation.runId,
      ),
    );
  }

  /**
   * Write one already-built Arrow batch to `table` and await durability.
   *
   * The precision write door, beside {@link Bifrost.insert}: it names its
   * destination instead of using the active binding, carries correlation as
   * ordinary columns, and is durable when it resolves, so no flush follows it.
   * Build the batch against {@link TableConfig.schema} from
   * `describeTableConfig` - a canonical table compares an incoming block
   * against its declared fields exactly, metadata included.
   */
  async writeBatch(table: string, batch: RecordBatch): Promise<void> {
    const ipc = tableToIPC(new Table(batch), "stream");
    lifecycleValue<null>(await this.#native.writeBatch(table, Buffer.from(ipc)));
  }

  /** Flush every pooled producer and await each durable acknowledgement. */
  async flush(): Promise<void> {
    lifecycleValue<null>(await this.#native.flush());
  }

  /** Drain every producer and stop its background task. */
  async shutdown(): Promise<void> {
    lifecycleValue<null>(await this.#native.shutdown());
  }

  /**
   * Run one SQL SELECT over any authorized table and collect every batch.
   *
   * Drains {@link Bifrost.stream}, so the two cannot disagree about the rows a
   * query returns or about the terminal frame each requires.
   */
  async sql(query: string): Promise<QueryResult>;
  /**
   * Run one SQL SELECT and return each row parsed by `rows`.
   *
   * A purely local projection over the completed result: the query, its
   * authorization, its limits, and its terminal are the same ones raw
   * {@link Bifrost.sql} runs. The schema never reaches the server and says
   * nothing about the table's stored layout.
   *
   * @throws whatever `rows.parse` throws for the first row it rejects, so a
   * partially valid result is never returned as success.
   */
  async sql<T>(query: string, rows: RowSchema<T>): Promise<T[]>;
  async sql<T>(
    query: string,
    rows?: RowSchema<T>,
  ): Promise<QueryResult | T[]> {
    const stream = await this.stream({ sql: query });
    const batches: RecordBatch[] = [];
    for await (const batch of stream) {
      batches.push(batch);
    }
    const terminal = stream.terminal;
    const schema = stream.schema;
    if (terminal === undefined || schema === undefined) {
      throw new IncompleteQueryStreamError(
        502,
        "Query stream incomplete",
        "query stream completed without terminal metadata",
      );
    }
    const result = new QueryResult(batches, terminal, schema);
    if (rows === undefined) {
      return result;
    }
    return result
      .toArrow()
      .toArray()
      .map((row: { toJSON(): Record<string, unknown> }) =>
        rows.parse(row.toJSON()),
      );
  }

  /** Run one SQL SELECT and iterate its batches as they arrive. */
  async stream(
    request: BifrostQueryRequest | string,
  ): Promise<BifrostQueryStream> {
    const query = typeof request === "string" ? { sql: request } : request;
    const nativeRequest: NativeQueryRequest = {
      sql: query.sql,
      visibility: query.visibility ?? "published_only",
      freshness: query.freshness ?? "strict",
      deadlineMs: query.deadlineMs,
    };
    const start = await this.#native.query(nativeRequest);
    const error = projectedError(start);
    if (error !== undefined) {
      throw error;
    }
    const native = start.takeStream();
    if (native === null || native === undefined) {
      throw new IncompleteQueryStreamError(
        502,
        "Query stream incomplete",
        "native query startup returned neither a stream nor structured error",
      );
    }
    return new BifrostQueryStream(native);
  }

  /** Number of distinct table producers currently pooled. */
  get producerCount(): number {
    return this.#native.producerCount;
  }

  /** List active queries visible to the authenticated tenant. */
  async running(): Promise<RunningQuery[]> {
    return lifecycleValue<RunningQueryWire[]>(await this.#native.running()).map(
      runningQuery,
    );
  }

  /** Return one active query by its canonical request ID. */
  async status(requestId: string): Promise<RunningQuery> {
    return runningQuery(
      lifecycleValue<RunningQueryWire>(await this.#native.status(requestId)),
    );
  }

  /** Request server-side cancellation without closing a local stream. */
  async cancel(requestId: string): Promise<CancelRunningQueryResult> {
    const wire = lifecycleValue<CancelRunningQueryWire>(
      await this.#native.cancel(requestId),
    );
    return {
      requestId: wire.request_id,
      cancellationStarted: wire.cancellation_started,
    };
  }

  /** Describe one registered table's stored physical schema. */
  async describeTable(
    namespace: string,
    name: string,
  ): Promise<TableDescription> {
    return lifecycleValue<TableDescription>(
      await this.#native.describeTable(namespace, name),
    );
  }
}

/** Exact Card identity as returned by the registry. */
export interface CardRef {
  readonly kind: string;
  readonly name: string;
  readonly version: string;
  readonly space?: string | null;
  readonly uid?: string | null;
}

/** One Card envelope: `apiVersion`, `kind`, `metadata`, `spec`, and server-derived fields. */
export interface Card {
  readonly apiVersion: "wyrd/v1";
  readonly kind: string;
  readonly metadata: {
    readonly name: string;
    readonly version: string;
    readonly space?: string;
    readonly uid?: string;
    readonly [key: string]: unknown;
  };
  readonly spec: Readonly<Record<string, unknown>>;
  readonly [key: string]: unknown;
}

/** Server outcome for one Card in a composite registration. */
export interface CardRegistrationOutcome {
  readonly card_ref: CardRef;
  readonly spec_hash: string;
  readonly artifact_hash: string | null;
  readonly status: string;
  readonly outcome: string;
  readonly card_blob_uri: string | null;
}

/** Registration receipt: the graph root plus dependency-first outcomes. */
export interface RegistrationReceipt {
  readonly root: CardRef;
  readonly outcomes: readonly CardRegistrationOutcome[];
}

/** Metadata-only Card list filters. */
export interface ListCardsRequest {
  readonly kind?: string;
  readonly space?: string;
  readonly name?: string;
  readonly version_range?: string;
  readonly status?: string;
  readonly filter?: string;
  readonly include_prerelease?: boolean;
  readonly limit?: number;
  readonly cursor?: string;
}

/** One metadata-only Card summary. */
export interface CardSummary {
  readonly card_uid: string;
  readonly kind: string;
  readonly space: string;
  readonly name: string;
  readonly version: string;
  readonly spec_hash: string;
  readonly artifact_hash: string | null;
  readonly status: string;
  readonly [key: string]: unknown;
}

/** One page of Card summaries. */
export interface ListCardsResponse {
  readonly items: readonly CardSummary[];
  readonly next_cursor: string | null;
}

/** Machine-readable result of a published hydration. */
export interface HydrationSummary {
  readonly root: CardRef;
  readonly destination: string;
  readonly mode: "complete" | "metadata";
  readonly card_count: number;
  readonly artifact_count: number;
  readonly downloaded_artifact_count: number;
}

/** One verified artifact payload in a local `WyrdState` bundle. */
export interface HydratedArtifact {
  readonly relative_path: string;
  readonly local_path: string;
  readonly sha256: string;
  readonly size_bytes: number;
  readonly content_type: string | null;
}

/** Card references cross the native boundary as text or serialized JSON. */
function cardRefText(ref: CardRef | string): string {
  return typeof ref === "string" ? ref : JSON.stringify(ref);
}

/**
 * Tenant-scoped Card registry client over the shared Rust `Cards` handle.
 *
 * Registration, reads, hydration, and deletion run in Rust; failures throw
 * a structured {@link WyrdError}.
 */
export class Cards {
  readonly #native: NativeCards;

  private constructor(native: NativeCards) {
    this.#native = native;
  }

  /**
   * Build a registry client without performing IO.
   *
   * Omitted options resolve through the same chain as {@link Bifrost.connect}.
   */
  static connect(
    options: { readonly serverUrl?: string; readonly credential?: string } = {},
  ): Cards {
    const connection = connectCards(options.serverUrl, options.credential);
    return new Cards(nativeHandle(connection.cards, connection.error));
  }

  /** Load a Card tree from disk and register it as one composite. */
  async registerFromPath(path: string): Promise<RegistrationReceipt> {
    return lifecycleValue<RegistrationReceipt>(
      await this.#native.registerFromPath(path),
    );
  }

  /** Fetch one Card envelope by exact reference. */
  async get(ref: CardRef | string): Promise<Card> {
    return lifecycleValue<Card>(await this.#native.get(cardRefText(ref)));
  }

  /** List metadata-only Card summaries. */
  async list(request: ListCardsRequest = {}): Promise<ListCardsResponse> {
    return lifecycleValue<ListCardsResponse>(
      await this.#native.list(JSON.stringify(request)),
    );
  }

  /** Hydrate one Card graph into a published local bundle for {@link WyrdState}. */
  async hydrate(
    ref: CardRef | string,
    destination: string,
    options: { readonly metadataOnly?: boolean } = {},
  ): Promise<HydrationSummary> {
    return lifecycleValue<HydrationSummary>(
      await this.#native.hydrate(
        cardRefText(ref),
        destination,
        options.metadataOnly ?? false,
      ),
    );
  }

  /** Soft-delete one Card by exact reference. */
  async delete(ref: CardRef | string): Promise<void> {
    lifecycleValue<null>(await this.#native.delete(cardRefText(ref)));
  }
}

/**
 * Offline view of a hydrated Card bundle over the shared Rust `WyrdState`.
 *
 * Reads never contact Wyrd; an unknown alias or invalid bundle throws a
 * structured {@link WyrdError}.
 */
export class WyrdState {
  readonly #native: NativeWyrdState;

  private constructor(native: NativeWyrdState) {
    this.#native = native;
  }

  /** Load and validate one hydrated bundle, throwing for an invalid bundle. */
  static fromPath(path: string): WyrdState {
    const state = new WyrdState(openWyrdState(path));
    void state.rootRef;
    return state;
  }

  /** The exact root Card reference. */
  get rootRef(): CardRef {
    return lifecycleValue<CardRef>(this.#native.rootRef());
  }

  /** Every persisted alias in stable sorted order. */
  get aliases(): readonly string[] {
    return lifecycleValue<string[]>(this.#native.aliases());
  }

  /** The stored Card envelope for an alias. */
  card(alias: string): Card {
    return lifecycleValue<Card>(this.#native.card(alias));
  }

  /** The exact Card reference for an alias. */
  cardRef(alias: string): CardRef {
    return lifecycleValue<CardRef>(this.#native.cardRef(alias));
  }

  /** The verified local artifacts for an alias. */
  artifacts(alias: string): readonly HydratedArtifact[] {
    return lifecycleValue<HydratedArtifact[]>(this.#native.artifacts(alias));
  }
}
