import { tableFromIPC, type RecordBatch, type Schema } from "apache-arrow";
import { createRequire } from "node:module";

import type {
  NativeBifrostQueryStream,
  NativeGenAiRequest,
  NativeLifecycleResult,
  NativeQueryRequest,
  NativeQueryStep,
} from "../index.cjs";

const require = createRequire(import.meta.url);
const nativeBinding = require("../index.cjs") as typeof import("../index.cjs");
const { connectBifrost, describeTableConfig, tableConfigFromJsonSchema } =
  nativeBinding;
type NativeBifrost = import("../index.cjs").NativeBifrost;
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

/**
 * One event nested on its owning span, in producer order.
 *
 * `attributes` is producer-defined JSON, and the server omits it entirely
 * without `bifrost_trace_payload:read`.
 */
export interface SpanEvent {
  readonly time_unix_nano: number;
  readonly name: string;
  readonly attributes?: unknown;
  readonly dropped_attributes_count: number;
}

/** One link nested on its owning span, in producer order. */
export interface SpanLink {
  readonly linked_trace_id: string;
  readonly linked_span_id: string;
  readonly trace_state: string;
  readonly flags: number;
  readonly attributes?: unknown;
  readonly dropped_attributes_count: number;
}

/**
 * One complete span, carrying its own events and links.
 *
 * Every optional member is either genuinely absent on the record or
 * payload-gated: without `bifrost_trace_payload:read` the server omits it from
 * the wire rather than returning it empty.
 */
export interface Span {
  readonly span_id: string;
  readonly parent_span_id?: string;
  readonly trace_state: string;
  readonly flags: number;
  readonly name: string;
  readonly kind: number;
  readonly start_time_unix_nano: number;
  readonly end_time_unix_nano: number;
  readonly duration_nano: number;
  readonly status_code?: number;
  readonly status_message?: string;
  readonly attributes?: unknown;
  readonly dropped_attributes_count: number;
  readonly events?: readonly SpanEvent[];
  readonly dropped_events_count: number;
  readonly links?: readonly SpanLink[];
  readonly dropped_links_count: number;
  readonly service_name?: string;
  readonly resource_attributes?: unknown;
  readonly resource_dropped_attributes_count: number;
  readonly resource_schema_url: string;
  readonly scope_name: string;
  readonly scope_version: string;
  readonly scope_attributes?: unknown;
  readonly scope_dropped_attributes_count: number;
  readonly scope_schema_url: string;
}

/** One complete authorized cut of a trace, as returned by `getTrace`. */
export interface TraceDetail {
  readonly trace: {
    readonly trace_id: string;
    readonly spans: readonly Span[];
  };
}

/**
 * One GenAI generation read from the canonical span table.
 *
 * Each promoted scalar is absent when its source attribute was; the two message
 * payloads are additionally gated on `bifrost_genai_payload:read`. They stay
 * `unknown` because a message list is producer-defined JSON, not a fixed wire
 * shape.
 */
export interface GenAiRow {
  readonly conversation_id?: string;
  readonly model?: string;
  readonly provider?: string;
  readonly start_time_unix_nano: number;
  readonly input_tokens?: number;
  readonly output_tokens?: number;
  readonly input_messages?: unknown;
  readonly output_messages?: unknown;
}

/** One page of GenAI generation records. */
export interface GenAiPage {
  readonly rows: readonly GenAiRow[];
  readonly next_page_token?: string;
}

/** Filters for one page of GenAI generation records. */
export interface GenAiQuery {
  readonly since?: string;
  readonly until?: string;
  readonly limit?: number;
  readonly pageToken?: string;
  readonly conversationId?: string;
  readonly model?: string;
  readonly provider?: string;
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

export class WyrdError extends Error {
  readonly code: string;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly remediation: string | undefined;
  /** JSON-safe structured diagnostics supplied by the originating Wyrd error. */
  readonly details: unknown;

  constructor(
    code: string,
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
    metadata.errorCode,
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
 * One Bifrost table: its name, the columns a model declares, and the physical
 * layout to request.
 *
 * Build it from a JSON Schema document — `z.toJSONSchema()` for Zod, or a
 * literal — or fetch an existing table by name. No fingerprint is computed
 * here: the server mints it, so `resolved` is undefined until the table is
 * registered or described.
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

  static fromJsonSchema(
    table: string,
    schema: Readonly<Record<string, unknown>>,
    layout?: TableLayout,
  ): TableConfig {
    return new TableConfig(
      tableConfigFromJsonSchema(table, JSON.stringify(schema), layoutJson(layout)),
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
    return new TableConfig(
      await describeTableConfig(
        table,
        transport.serverUrl,
        transport.credential,
        transport.grpcUrl,
      ),
    );
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
 * Arrow batches from one query, converted on demand.
 *
 * Collected by draining {@link Bifrost.stream}, so a collected result and a
 * streamed one are the same rows read the same way — only the batches are
 * retained rather than yielded.
 */
export class QueryResult {
  readonly #batches: readonly RecordBatch[];
  readonly #terminal: QueryTerminal;

  constructor(batches: readonly RecordBatch[], terminal: QueryTerminal) {
    this.#batches = batches;
    this.#terminal = terminal;
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
    return new Bifrost(
      await connectBifrost(
        options.table?.native,
        options.serverUrl,
        options.credential,
        options.grpcUrl,
      ),
    );
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
  async sql(query: string): Promise<QueryResult> {
    const stream = await this.stream({ sql: query });
    const batches: RecordBatch[] = [];
    for await (const batch of stream) {
      batches.push(batch);
    }
    const terminal = stream.terminal;
    if (terminal === undefined) {
      throw new IncompleteQueryStreamError(
        502,
        "Query stream incomplete",
        "query stream completed without terminal metadata",
      );
    }
    return new QueryResult(batches, terminal);
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

  /**
   * Read one complete authorized cut of a single trace.
   *
   * Trace detail has no continuation token: the bounds narrow the scanned
   * window only, and each span carries its own events and links.
   */
  async getTrace(
    traceId: string,
    bounds: { readonly since?: string; readonly until?: string } = {},
  ): Promise<TraceDetail> {
    return lifecycleValue<TraceDetail>(
      await this.#native.getTrace(traceId, bounds.since, bounds.until),
    );
  }

  /** Read one page of GenAI generation records. */
  async queryGenAi(query: GenAiQuery = {}): Promise<GenAiPage> {
    const request: NativeGenAiRequest = {
      since: query.since,
      until: query.until,
      limit: query.limit,
      pageToken: query.pageToken,
      conversationId: query.conversationId,
      model: query.model,
      provider: query.provider,
    };
    return lifecycleValue<GenAiPage>(await this.#native.queryGenai(request));
  }
}
