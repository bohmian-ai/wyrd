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
  CardRef,
  RegisteredCard,
  VerificationError,
} from "./card-types.js";

import type {
  NativeBifrostQueryStream,
  NativeLifecycleResult,
  NativeQueryRequest,
  NativeQueryStep,
} from "../index.cjs";

const require = createRequire(import.meta.url);
const nativeBinding = require("../index.cjs") as typeof import("../index.cjs");
const {
  cliApply,
  cliDeleteProviderCredential,
  cliGet,
  cliIssueKey,
  cliLoad,
  cliPlan,
  cliPutProviderCredential,
  cliRevokeProviderCredential,
  connectBifrost,
  connectCards,
  connectOperatorConnections,
  connectGateway,
  connectWyrdClient,
  describeTableConfig,
  loadWorkflowFromPath,
  openWyrdState,
  tableConfigFromJsonSchema,
} = nativeBinding;
type NativeBifrost = import("../index.cjs").NativeBifrost;
type NativeCards = import("../index.cjs").NativeCards;
type NativeOperatorConnections = import("../index.cjs").NativeOperatorConnections;
type NativeGateway = import("../index.cjs").NativeGateway;
type NativeWyrdClient = import("../index.cjs").NativeWyrdClient;
type NativeWyrdState = import("../index.cjs").NativeWyrdState;
type NativeWorkflow = import("../index.cjs").NativeWorkflow;
type NativeRun = import("../index.cjs").NativeRun;
type NativeRunOpen = import("../index.cjs").NativeRunOpen;
type NativeTableConfig = import("../index.cjs").NativeTableConfig;

/**
 * One positional SQL bind value: `params[i]` binds placeholder `$(i + 1)`.
 *
 * Values travel as typed data beside the SQL text and are never interpolated
 * into it. A safe integer binds as an integer and any other number as a float.
 */
export type QueryParam = null | boolean | number | string;

export interface BifrostQueryRequest {
  sql: string;
  params?: readonly QueryParam[];
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
 * for a canonical signal table; `compaction_target_file_size_bytes` only when
 * the table declared an explicit Forge compaction file target, and
 * `compaction_type` only when it declared an explicit compaction type.
 */
export interface TableDescription {
  readonly entry: TableEntry;
  readonly user_fields: readonly FieldDescription[];
  readonly correlation_fields: readonly FieldDescription[];
  readonly managed_candidates: readonly FieldDescription[];
  readonly canonical_physical_fingerprint?: string;
  readonly physical_layout: PhysicalLayout;
  readonly compaction_target_file_size_bytes?: number;
  readonly compaction_type?: CompactionType;
}

/**
 * The physical compaction strategy a table asks Forge to apply, in its wire
 * spelling. Omitted, Forge compacts the table `small-files`; a copy-on-write table
 * compacts `full` whatever it declares.
 */
export type CompactionType = "auto" | "full" | "small-files" | "files-with-delete";

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
export type {
  AgentSpec,
  CardKind,
  CardRef,
  DataSpec,
  DriftBaselineState,
  DriftBaselineStatus,
  Metadata as CardMetadata,
  ModelSpec,
  OperatorSpec,
  PromptSpec,
  RegisteredAgentCard,
  RegisteredCard,
  RegisteredDataCard,
  RegisteredModelCard,
  RegisteredOperatorCard,
  RegisteredPromptCard,
  RegisteredServiceCard,
  RegisteredTriggerCard,
  RegisteredUntypedCard,
  RegisteredVerifierCard,
  Relationships as CardRelationships,
  ServiceSpec,
  Status as CardStatus,
  TriggerSpec,
  TypedCardKind,
  VerificationError,
  VerificationStatus,
  VerifierSpec,
} from "./card-types.js";
/** Every type generated from the `wyrd-spec` Card schema, including nested spec types. */
export type * as CardTypes from "./card-types.js";

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

  private constructor(native: NativeBifrostQueryStream) {
    this.#native = native;
  }

  /** @internal Wrap one native stream the Bifrost client started. */
  static fromNative(native: NativeBifrostQueryStream): BifrostQueryStream {
    return new BifrostQueryStream(native);
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

/** Optional physical declarations for {@link TableConfig.fromJsonSchema}. */
export interface TableConfigOptions {
  /** Physical layout to request; omitted, the server default layout applies. */
  readonly layout?: TableLayout;
  /** Explicit Forge compaction file target in bytes. */
  readonly compactionTargetFileSizeBytes?: number;
  /** Forge compaction type; omitted, Forge compacts the table `small-files`. */
  readonly compactionType?: CompactionType;
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
   * `options.layout` requests the table's physical layout.
   * `options.compactionTargetFileSizeBytes` pins the table's Forge compaction
   * file target; omitted, the table follows the server's deployment default.
   * Registration records it once, and a later registration naming a different
   * target is refused rather than silently changing it.
   *
   * `options.compactionType` chooses the table's Forge compaction type;
   * omitted, Forge compacts it `small-files`. Like the target, it is recorded
   * once and a later registration naming a different type is refused with
   * `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`.
   *
   * @throws {@link WyrdError} when the resulting document does not map to an
   * Arrow schema, declares a column the write path already owns, the
   * compaction target is not a non-negative integer, or the compaction type
   * is not a known spelling (`WYRD_SPEC_400_VALIDATION`).
   */
  static fromJsonSchema(
    table: string,
    schema: Readonly<Record<string, unknown>> | JsonSchemaSource,
    options: TableConfigOptions = {},
  ): TableConfig {
    const document =
      typeof (schema as JsonSchemaSource).toJSONSchema === "function"
        ? (schema as JsonSchemaSource).toJSONSchema()
        : schema;
    const declared = tableConfigFromJsonSchema(
      table,
      JSON.stringify(document),
      layoutJson(options.layout),
      options.compactionTargetFileSizeBytes,
      options.compactionType,
    );
    return new TableConfig(nativeHandle(declared.config, declared.error));
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

  /**
   * The explicit Forge compaction file target, declared or described, or
   * undefined when the table follows the server's deployment default.
   */
  get compactionTargetFileSizeBytes(): number | undefined {
    const wire = JSON.parse(this.#native.configJson) as {
      compaction_target_file_size_bytes?: number;
    };
    return wire.compaction_target_file_size_bytes;
  }

  /**
   * The explicit Forge compaction type, declared or described, or undefined
   * when the table compacts with the `small-files` default.
   */
  get compactionType(): CompactionType | undefined {
    const wire = JSON.parse(this.#native.configJson) as {
      compaction_type?: CompactionType;
    };
    return wire.compaction_type;
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
   * `WYRD_API_KEY` → the saved `wyrd auth login` for this server and
   * `WYRD_TENANT` → `~/.config/wyrd/credentials.toml`.
   *
   * `client` reuses an existing, possibly delegated, {@link WyrdClient} for
   * authentication and transport. It cannot be combined with `serverUrl`,
   * `credential`, or `grpcUrl`; doing so throws `WYRD_SPEC_400_VALIDATION`.
   *
   * `clientByteLimitBytes` overrides the handle-wide ingestion byte budget
   * (256 MiB by default); a budget too small to seal one message throws
   * `WYRD_CLIENT_400_CONFIG_INVALID`.
   */
  static async connect(
    options:
      | {
          readonly table?: TableConfig;
          readonly serverUrl?: string;
          readonly credential?: string;
          readonly grpcUrl?: string;
          readonly client?: never;
          readonly clientByteLimitBytes?: number;
        }
      | {
          readonly table?: TableConfig;
          readonly client: WyrdClient;
          readonly serverUrl?: never;
          readonly credential?: never;
          readonly grpcUrl?: never;
          readonly clientByteLimitBytes?: number;
        } = {},
  ): Promise<Bifrost> {
    const connection =
      options.client === undefined
        ? await connectBifrost(
            options.table?.native,
            options.serverUrl,
            options.credential,
            options.grpcUrl,
            options.clientByteLimitBytes,
          )
        : await wyrdClientNative(options.client).connectBifrost(
            options.table?.native,
            options.serverUrl,
            options.credential,
            options.grpcUrl,
            options.clientByteLimitBytes,
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
   * query returns or about the terminal frame each requires. `params` bind
   * the `$1..$n` placeholders in order and are never interpolated into SQL.
   */
  async sql(query: string, params?: readonly QueryParam[]): Promise<QueryResult>;
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
  async sql<T>(
    query: string,
    params: readonly QueryParam[],
    rows: RowSchema<T>,
  ): Promise<T[]>;
  async sql<T>(
    query: string,
    params?: readonly QueryParam[],
    rows?: RowSchema<T>,
  ): Promise<QueryResult | T[]> {
    const stream = await this.stream({ sql: query, params });
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
      deadlineMs: query.deadlineMs,
      params: query.params?.map((value) => {
        switch (typeof value) {
          case "boolean":
            return { kind: "boolean", bool: value };
          case "number":
            return { kind: "number", number: value };
          case "string":
            return { kind: "string", string: value };
          default:
            return { kind: value === null ? "null" : typeof value };
        }
      }),
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
    return BifrostQueryStream.fromNative(native);
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

/** Audience a delegated token is bound to. */
export type TokenAudience = "wyrd" | "bifrost";

/** Reads a {@link WyrdClient}'s native handle for {@link Bifrost.connect}. */
let wyrdClientNative: (client: WyrdClient) => NativeWyrdClient;

/**
 * Authenticated Wyrd client over the shared Rust `WyrdClient`.
 *
 * Token exchange, caching, and renewal run in Rust; failures throw a
 * structured {@link WyrdError}.
 */
export class WyrdClient {
  readonly #native: NativeWyrdClient;

  static {
    wyrdClientNative = (client) => client.#native;
  }

  private constructor(native: NativeWyrdClient) {
    this.#native = native;
  }

  /**
   * Build a client without performing IO.
   *
   * Omitted options resolve from the environment, then the saved
   * `wyrd auth login` for this server (the one for `WYRD_TENANT` when set,
   * otherwise the newest), then
   * `~/.config/wyrd/credentials.toml`. A `WYRD_TENANT` that matches none of this
   * server's saved logins, or a saved login that cannot be used, raises
   * `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE`.
   */
  static connect(
    options: {
      readonly serverUrl?: string;
      readonly credential?: string;
      readonly grpcUrl?: string;
    } = {},
  ): WyrdClient {
    const result = connectWyrdClient(
      options.serverUrl,
      options.credential,
      options.grpcUrl,
    );
    return new WyrdClient(nativeHandle(result.client, result.error));
  }

  /** The effective HTTP server URL this client sends requests to. */
  get serverUrl(): string {
    return this.#native.serverUrl;
  }

  /**
   * The effective gRPC endpoint: the explicit `grpcUrl` when one was given,
   * else the server URL's scheme and host on the public gRPC port `50051`.
   */
  get grpcUrl(): string {
    return this.#native.grpcUrl;
  }

  /**
   * Return a current bearer for this client's credential.
   *
   * Hand it to a third-party client, such as an OpenAI SDK pointed at the
   * Gateway. The token is renewed in Rust when it nears expiry, so call this
   * again rather than holding the value.
   */
  async accessToken(): Promise<string> {
    const result = await this.#native.accessToken();
    return nativeHandle(result.token, result.error);
  }

  /**
   * Return a client that acts for the holder of `subjectToken` (RFC 8693).
   *
   * This client's credential is the actor. The issued token's subject is the
   * inbound principal, its actor is this client, and its permissions are the
   * intersection of both. The first exchange runs here; the returned client
   * re-exchanges before expiry.
   */
  async onBehalfOf(
    subjectToken: string,
    options: { readonly audience?: TokenAudience } = {},
  ): Promise<WyrdClient> {
    const result = await this.#native.onBehalfOf(
      subjectToken,
      options.audience ?? "bifrost",
    );
    return new WyrdClient(nativeHandle(result.client, result.error));
  }
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
    options: {
      readonly serverUrl?: string;
      readonly credential?: string;
    } = {},
  ): Cards {
    const connection = connectCards(
      options.serverUrl,
      options.credential,
    );
    return new Cards(nativeHandle(connection.cards, connection.error));
  }

  /** Load a Card tree from disk and register it as one composite. */
  async registerFromPath(path: string): Promise<RegistrationReceipt> {
    return lifecycleValue<RegistrationReceipt>(
      await this.#native.registerFromPath(path),
    );
  }

  /**
   * Fetch one registered Card envelope by exact reference.
   *
   * The envelope is discriminated by `kind`: `spec` and `status` are typed for
   * Data, Model, Prompt, Agent, Verifier, Service, Trigger, and Operator Cards,
   * including a Drift Verifier's `status.verification.baseline` state. Other
   * kinds return {@link RegisteredUntypedCard}, whose `spec` is a plain record.
   */
  async get(ref: CardRef | string): Promise<RegisteredCard> {
    return lifecycleValue<RegisteredCard>(
      await this.#native.get(cardRefText(ref)),
    );
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

  /** Load registered Workflows through this registry client. */
  get workflow(): WorkflowCards {
    return WorkflowCards.fromNative(this.#native);
  }
}

/**
 * Selects one registered Workflow: its exact `space`, `name`, and `version`,
 * or its `uid`. Mixing the two shapes is a type error and a runtime refusal.
 */
export type WorkflowSelector =
  | {
      readonly space: string;
      readonly name: string;
      readonly version: string;
      readonly uid?: never;
    }
  | {
      readonly uid: string;
      readonly space?: never;
      readonly name?: never;
      readonly version?: never;
    };

/** Registered-Workflow loading for one {@link Cards} client. */
export class WorkflowCards {
  readonly #native: NativeCards;

  private constructor(native: NativeCards) {
    this.#native = native;
  }

  /** @internal Built by {@link Cards.workflow}. */
  static fromNative(native: NativeCards): WorkflowCards {
    return new WorkflowCards(native);
  }

  /**
   * Load one registered Workflow and the exact Card versions it pins.
   *
   * @example
   * ```ts
   * const workflow = await cards.workflow.load({
   *   space: "reviews",
   *   name: "code-review",
   *   version: "1.0.0",
   * });
   * const same = await cards.workflow.load({ uid: workflowUid });
   * ```
   *
   * @throws {WyrdError} `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no such
   * Workflow exists, `WYRD_PERMISSION_403_DENIED_RBAC` when the credential
   * cannot read Cards, or `WYRD_WORKFLOW_400_INVALID_CARD_REF` for a mixed
   * or incomplete selector or a malformed field.
   */
  async load(selector: WorkflowSelector): Promise<Workflow> {
    const loaded = await this.#native.loadWorkflow(JSON.stringify(selector));
    return Workflow.fromNative(nativeHandle(loaded.workflow, loaded.error));
  }
}

/** Any value JSON can carry: Workflow inputs, outputs, and error details. */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

/** Lifecycle status of a Workflow run. */
export type WorkflowRunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "timed_out";

/** Lifecycle status of one Workflow step. */
export type WorkflowStepStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "unstarted";

/** Catalog error recorded on a failed run or step. */
export interface WorkflowRunError {
  /** Stable Wyrd error code. */
  readonly code: string;
  /** Safe diagnostic message. */
  readonly message: string;
  /** Safe structured details. */
  readonly details: JsonValue;
  /** Operator-facing remediation. */
  readonly remediation: string;
}

/** Result of one Workflow step. */
export interface WorkflowStepResult {
  /** Step lifecycle status. */
  readonly status: WorkflowStepStatus;
  /** Final text output, present only for a succeeded step. */
  readonly text: string | null;
  /** Structured output, present only for a succeeded step. */
  readonly structured_output: JsonValue | null;
  /** Number of attempts begun. */
  readonly attempts: number;
  /** RFC 3339 first attempt start time, `null` if the step never started. */
  readonly started_at: string | null;
  /** RFC 3339 terminal time, `null` while the step is active. */
  readonly ended_at: string | null;
  /** Terminal error, present only for a failed step. */
  readonly error: WorkflowRunError | null;
}

/** Snapshot of one Workflow run. */
export interface WorkflowRun {
  /** Run identity. */
  readonly run_id: string;
  /** Pinned Workflow Card, `null` for an unregistered local run. */
  readonly workflow: CardRef | null;
  /** Run lifecycle status. */
  readonly status: WorkflowRunStatus;
  /** Declared outputs, populated only for a succeeded run. */
  readonly outputs: Record<string, JsonValue>;
  /** Step results keyed by step ID. */
  readonly steps: Record<string, WorkflowStepResult>;
  /** RFC 3339 creation time. */
  readonly created_at: string;
  /** RFC 3339 start time, `null` before the run starts. */
  readonly started_at: string | null;
  /** RFC 3339 terminal time, `null` while the run is active. */
  readonly ended_at: string | null;
  /** Primary error for a `failed` or `timed_out` run. */
  readonly error: WorkflowRunError | null;
}

/**
 * Runnable Workflow loaded from an authored file or from the registry.
 *
 * Loading validates the whole graph; running happens in Rust.
 */
export class Workflow {
  readonly #native: NativeWorkflow;

  private constructor(native: NativeWorkflow) {
    this.#native = native;
  }

  /** @internal Built by {@link Workflow.fromPath} or {@link WorkflowCards.load}. */
  static fromNative(native: NativeWorkflow): Workflow {
    return new Workflow(native);
  }

  /**
   * Load an authored Workflow file and the Cards it references.
   *
   * Local files need no server. Registry refs are read with the ambient
   * client configuration (`WYRD_SERVER_URL`, `WYRD_API_KEY`).
   *
   * @example
   * ```ts
   * const workflow = await Workflow.fromPath("workflows/code-review/workflow.yaml");
   * console.log(workflow.stepIds);
   * ```
   *
   * @throws {WyrdError} `WYRD_CLIENT_401_NO_CREDENTIALS` when the file
   * references a registered Card and no credential is configured, or
   * `WYRD_REGISTRY_404_CARD_NOT_FOUND` when a referenced Card is missing.
   */
  static async fromPath(path: string): Promise<Workflow> {
    const loaded = await loadWorkflowFromPath(path);
    return Workflow.fromNative(nativeHandle(loaded.workflow, loaded.error));
  }

  /** Step IDs in declaration order. */
  get stepIds(): string[] {
    return this.#native.stepIds();
  }

  /**
   * Run this Workflow with its declared inputs.
   *
   * Validation and route refusals throw before any step runs; step failures
   * are recorded in the returned run.
   */
  async run(input: Record<string, JsonValue> = {}): Promise<WorkflowRun> {
    return lifecycleValue<WorkflowRun>(
      await this.#native.run(JSON.stringify(input)),
    );
  }
}

/** The judgment of one direct verification, returned by {@link Observe.verify}. */
export interface Judgment {
  /** Whether the expectations held: true only for a `passed` verdict. */
  readonly passed: boolean;
  /** Transient identity found only in this response, the audit row, and the trace. */
  readonly execution_id: string;
  readonly verifier: CardRef;
  readonly subject: CardRef;
  readonly kind:
    | "drift_psi"
    | "drift_spc"
    | "drift_custom"
    | "eval_assertion"
    | "eval_llm_judge"
    | "eval_other"
    | "unknown";
  readonly verdict: "passed" | "failed" | "inconclusive";
  readonly summary: string;
  readonly counts:
    | { readonly implementation: "drift"; readonly drifted_features: number; readonly total_features: number }
    | {
        readonly implementation: "eval";
        readonly passed_tasks: number;
        readonly failed_tasks: number;
        readonly total_tasks: number;
        readonly pass_rate_percent: number;
      };
  /** `{ drift: DriftReport }` or `{ eval: { results, skipped } }`. */
  readonly detail: { readonly drift: unknown } | { readonly eval: unknown };
}

/** The provider an Operator connection authenticates to. */
export type OperatorProvider = "slack" | "pager_duty" | "http";

/** A connection's lifecycle state; Operators naming a disabled one fail closed. */
export type OperatorConnectionStatus = "active" | "disabled";

/** Write-only HTTP connection credential, tagged by `scheme`. */
export type HttpConnectionAuth =
  | { readonly scheme: "bearer"; readonly token: string }
  | {
      readonly scheme: "basic";
      readonly username: string;
      readonly password: string;
    }
  | {
      readonly scheme: "header";
      readonly name: string;
      readonly value: string;
    };

/** Redacted HTTP auth metadata a view returns; the credential is never read back. */
export type HttpAuthScheme =
  | { readonly scheme: "bearer" }
  | { readonly scheme: "basic" }
  | { readonly scheme: "header"; readonly name: string };

/** A connection create request: provider config plus its secret. */
export type CreateOperatorConnectionRequest =
  | {
      readonly provider: "slack";
      readonly name: string;
      readonly workspace_id: string;
      readonly bot_token: string;
    }
  | {
      readonly provider: "pager_duty";
      readonly name: string;
      readonly integration_key: string;
    }
  | {
      readonly provider: "http";
      readonly name: string;
      readonly origin: string;
      readonly auth: HttpConnectionAuth;
    };

/**
 * A connection update: the same provider, with any config, status, or secret
 * field to replace; omitted fields are preserved.
 */
export type UpdateOperatorConnectionRequest =
  | {
      readonly provider: "slack";
      readonly workspace_id?: string;
      readonly bot_token?: string;
      readonly status?: OperatorConnectionStatus;
    }
  | {
      readonly provider: "pager_duty";
      readonly integration_key?: string;
      readonly status?: OperatorConnectionStatus;
    }
  | {
      readonly provider: "http";
      readonly origin?: string;
      readonly auth?: HttpConnectionAuth;
      readonly status?: OperatorConnectionStatus;
    };

/** Fields every connection view carries regardless of provider. */
interface OperatorConnectionViewBase {
  readonly connection_id: string;
  readonly name: string;
  readonly status: OperatorConnectionStatus;
  readonly created_at: string;
  readonly updated_at: string;
}

/**
 * A connection's redacted metadata with provider config flattened alongside
 * it; no view ever carries a secret.
 */
export type OperatorConnectionView = OperatorConnectionViewBase &
  (
    | { readonly provider: "slack"; readonly workspace_id: string }
    | { readonly provider: "pager_duty" }
    | {
        readonly provider: "http";
        readonly origin: string;
        readonly auth: HttpAuthScheme;
      }
  );

/**
 * Tenant-scoped Operator connection client over the shared Rust handle.
 *
 * The server encrypts each secret, authorizes and audits each request, and
 * returns only redacted metadata; failures throw a structured
 * {@link WyrdError}. Rotating a secret never requires a Card revision.
 */
export class OperatorConnections {
  readonly #native: NativeOperatorConnections;

  private constructor(native: NativeOperatorConnections) {
    this.#native = native;
  }

  /**
   * Build an Operator connection client without performing IO.
   *
   * Omitted options resolve through the same chain as {@link Cards.connect}.
   */
  static connect(
    options: {
      readonly serverUrl?: string;
      readonly credential?: string;
    } = {},
  ): OperatorConnections {
    const connection = connectOperatorConnections(
      options.serverUrl,
      options.credential,
    );
    return new OperatorConnections(
      nativeHandle(connection.connections, connection.error),
    );
  }

  /** Create one connection; requires `operators:write`. */
  async create(
    request: CreateOperatorConnectionRequest,
  ): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(
      await this.#native.create(JSON.stringify(request)),
    );
  }

  /** List the caller tenant's connections; requires `operators:read`. */
  async list(): Promise<readonly OperatorConnectionView[]> {
    return lifecycleValue<readonly OperatorConnectionView[]>(
      await this.#native.list(),
    );
  }

  /** Read one connection; requires `operators:read`. */
  async get(connectionId: string): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(
      await this.#native.get(connectionId),
    );
  }

  /** Update config, re-enable, or rotate the secret; requires `operators:write`. */
  async update(
    connectionId: string,
    request: UpdateOperatorConnectionRequest,
  ): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(
      await this.#native.update(connectionId, JSON.stringify(request)),
    );
  }

  /** Disable one connection; requires `operators:write`. */
  async disable(connectionId: string): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(
      await this.#native.disable(connectionId),
    );
  }
}

/** One media item an Eval observation names for its judge Prompt. */
export type EvalMediaRef = {
  /** The `${media:id}` binding slot this artifact fills in the judge Prompt. */
  readonly id: string;
  /** The supported media kind, chosen explicitly rather than from the URI. */
  readonly kind: "image" | "document";
  /** Durable object-storage locator; the server reads the bytes at judge time. */
  readonly uri: string;
  /** IANA media type, such as `image/png`. */
  readonly mediaType?: string;
};

/** The optional per-emission data an Eval observation may carry. */
export type EvalOptions = {
  /** The observed interaction's session; runs and sessions are many-to-many. */
  readonly sessionId?: string;
  /** Media descriptors the judge Prompt binds by `id`. */
  readonly media?: readonly EvalMediaRef[];
  /** Explicit lower-case hex trace identity, which wins over the active span. */
  readonly traceId?: string;
  /** Explicit lower-case hex span identity within `traceId`. */
  readonly spanId?: string;
};

/** Refuse one observation input the strict serializer cannot carry exactly. */
function invalidObservationInput(field: string, path: string, reason: string): WyrdError {
  return new WyrdError(
    "WYRD_SPEC_400_VALIDATION",
    400,
    "Validation failed",
    `${field} is invalid: ${path} ${reason}`,
    "Check the submitted Wyrd request fields against the published schema and retry.",
    { field, path, reason },
  );
}

/** Whether `key` is a canonical element index below `length`, which JSON keeps. */
function isArrayIndex(key: string, length: number): boolean {
  return String(Number(key)) === key && Number.isInteger(Number(key)) && Number(key) >= 0 && Number(key) < length;
}

/**
 * Serialize one observation input as JSON text, refusing what `JSON.stringify`
 * would silently drop or coerce.
 *
 * `JSON.stringify` omits `undefined`, functions, symbols, symbol keys,
 * non-enumerable properties, and non-element array properties, turns non-finite
 * numbers into `null`, cannot keep an unsafe integer exact, throws on bigint
 * and cycles, and flattens a `Map` or `Date` through its own rules. Rust only
 * ever sees the resulting text, so the one place such a change is detectable
 * is here, before it happens. Only plain objects, arrays, strings, booleans,
 * `null`, and finite safe numbers pass; durable validation stays in Rust.
 * Traversal reads every property and element exactly once into a plain
 * snapshot, and that snapshot is what gets stringified, so a getter cannot
 * return one value to validation and another to serialization.
 *
 * @throws a `WYRD_SPEC_400_VALIDATION` {@link WyrdError} naming the offending
 * path.
 */
function strictJson(field: string, value: unknown): string {
  const ancestors = new Set<object>();
  const check = (node: unknown, path: string): unknown => {
    if (node === null || typeof node === "string" || typeof node === "boolean") {
      return node;
    }
    if (typeof node === "number") {
      if (!Number.isFinite(node)) {
        throw invalidObservationInput(field, path, "is not a finite number");
      }
      if (Number.isInteger(node) && !Number.isSafeInteger(node)) {
        throw invalidObservationInput(field, path, "is an integer beyond the safe range");
      }
      return node;
    }
    if (typeof node !== "object") {
      throw invalidObservationInput(field, path, `is an unsupported ${typeof node} value`);
    }
    if (ancestors.has(node)) {
      throw invalidObservationInput(field, path, "is a cycle");
    }
    ancestors.add(node);
    let snapshot: unknown;
    if (Array.isArray(node)) {
      for (const key of Reflect.ownKeys(node)) {
        if (key !== "length" && !(typeof key === "string" && isArrayIndex(key, node.length))) {
          throw invalidObservationInput(field, path, "has a non-element array property");
        }
      }
      const items: unknown[] = [];
      for (let index = 0; index < node.length; index += 1) {
        items.push(check(node[index], `${path}[${index}]`));
      }
      snapshot = items;
    } else {
      const prototype = Object.getPrototypeOf(node) as unknown;
      if (prototype !== Object.prototype && prototype !== null) {
        throw invalidObservationInput(field, path, "is not a plain object");
      }
      const keys = Reflect.ownKeys(node);
      for (const key of keys) {
        if (typeof key === "symbol") {
          throw invalidObservationInput(field, path, "has a symbol key");
        }
        if (!Object.prototype.propertyIsEnumerable.call(node, key)) {
          throw invalidObservationInput(field, path, `has a non-enumerable property ${key}`);
        }
      }
      snapshot = Object.fromEntries(
        (keys as string[]).map((key) => [key, check((node as Record<string, unknown>)[key], `${path}.${key}`)]),
      );
    }
    ancestors.delete(node);
    return snapshot;
  };
  return JSON.stringify(check(value, "$"));
}

/**
 * The application's `@opentelemetry/api`, resolved once on first use; `null`
 * when it is not installed. Loaded lazily rather than imported so the SDK does
 * not make OpenTelemetry a required dependency.
 */
let otelApi: typeof import("@opentelemetry/api") | null | undefined;

/**
 * Trace and span identity of Node's active OpenTelemetry span.
 *
 * Node's OTel context never becomes Rust's current `tracing` span across
 * N-API, so the shared Rust fallback cannot see it; this boundary reads it and
 * passes the IDs down the ordinary explicit-options path. Both IDs or neither:
 * no installed API, no active span, or an invalid context supplies nothing.
 */
function activeSpanIds(): { readonly traceId?: string; readonly spanId?: string } {
  if (otelApi === undefined) {
    try {
      otelApi = require("@opentelemetry/api") as typeof import("@opentelemetry/api");
    } catch {
      otelApi = null;
    }
  }
  const spanContext = otelApi?.trace.getActiveSpan()?.spanContext();
  if (spanContext === undefined || !otelApi?.isSpanContextValid(spanContext)) {
    return {};
  }
  return { traceId: spanContext.traceId, spanId: spanContext.spanId };
}

/** The closed own-key set of one {@link EvalMediaRef}. */
const MEDIA_KEYS: ReadonlySet<string | symbol> = new Set(["id", "kind", "uri", "mediaType"]);

/**
 * Project Eval media descriptors to their wire names and serialize them.
 *
 * The projection would silently drop any key outside the closed descriptor
 * shape, so each original descriptor's own keys are checked first; declared
 * fields are then read once each and the snapshot goes through
 * {@link strictJson}.
 *
 * @throws a `WYRD_SPEC_400_VALIDATION` {@link WyrdError} naming the offending
 * descriptor.
 */
function mediaJson(media?: readonly EvalMediaRef[]): string | undefined {
  if (media === undefined) {
    return undefined;
  }
  return strictJson(
    "media",
    media.map((item, index) => {
      for (const key of Reflect.ownKeys(item)) {
        if (!MEDIA_KEYS.has(key) || !Object.prototype.propertyIsEnumerable.call(item, key)) {
          throw invalidObservationInput("media", `$[${index}]`, `has a symbol, hidden, or undeclared key ${String(key)}`);
        }
      }
      const { id, kind, uri, mediaType } = item;
      return { id, kind, uri, ...(mediaType === undefined ? {} : { media_type: mediaType }) };
    }),
  );
}

/**
 * The observation emits and the direct judgment available on one scoped
 * {@link Run}.
 *
 * `drift` and `eval` are synchronous: their fixed tables were described at
 * {@link WyrdState.startBifrost}, so an emit only projects and enqueues.
 * Returning is queue admission, never a durable acknowledgement —
 * {@link WyrdState.shutdown} is the barrier.
 */
export class Observe {
  readonly #native: NativeRun;

  private constructor(native: NativeRun) {
    this.#native = native;
  }

  /** @internal Wrap the native run this surface emits through. */
  static fromNative(native: NativeRun): Observe {
    return new Observe(native);
  }

  /**
   * Emit one Drift observation as one row per feature.
   *
   * @throws a {@link WyrdError} for a payload that is not a flat object of
   * strings, finite numbers, or booleans, before startup, after shutdown, or
   * when the producer is saturated.
   */
  drift(
    features: Readonly<Record<string, string | number | boolean>>,
    options: { readonly sessionId?: string } = {},
  ): void {
    lifecycleValue<null>(
      this.#native.drift(strictJson("features", features), options.sessionId),
    );
  }

  /**
   * Emit one Eval observation carrying its context and identity.
   *
   * With neither `traceId` nor `spanId` the active OpenTelemetry span supplies
   * both; a `spanId` without its `traceId` is refused.
   *
   * @throws a {@link WyrdError} for a context or media descriptor the strict
   * serializer refuses, a malformed identifier,
   * before startup, after shutdown, or when the producer is saturated.
   */
  eval(context: unknown, options: EvalOptions = {}): void {
    const contextJson = strictJson("context", context);
    const media = mediaJson(options.media);
    const identity = options.traceId === undefined && options.spanId === undefined
      ? activeSpanIds()
      : options;
    lifecycleValue<null>(
      this.#native.eval(
        contextJson,
        options.sessionId,
        media,
        identity.traceId,
        identity.spanId,
      ),
    );
  }

  /**
   * Emit one row into a registered `vala.datasets.<name>` table.
   *
   * The first call for a table describes it; later calls reuse the cached
   * schema and producer, so only the first awaits a lookup.
   *
   * @throws a {@link WyrdError} for a table outside `vala.datasets`, an unknown
   * or unauthorized table, before startup, or after shutdown.
   */
  async record(table: string, row: unknown): Promise<void> {
    lifecycleValue<null>(await this.#native.record(table, strictJson("row", row)));
  }

  /**
   * Judge this view's subject with a bound Verifier and return its judgment.
   *
   * `verifier` names a Verifier bound in `verified_by` to this view's subject.
   * An Eval Verifier takes one context object plus optional media; a Drift
   * Verifier takes an array of flat feature rows. A `failed` verdict resolves
   * normally. Nothing is observed, recorded, enqueued, or dispatched, Bifrost
   * need not be started, and the request is never replayed.
   *
   * @throws a {@link WyrdError}: `WYRD_SDK_404_UNKNOWN_VERIFIER` for an
   * unbound Verifier and `WYRD_SDK_400_INVALID_OBSERVATION` for input of the
   * wrong shape, both before any network IO, and otherwise the server's
   * verification refusal.
   */
  async verify(
    verifier: string,
    input: unknown,
    options: { readonly media?: readonly EvalMediaRef[] } = {},
  ): Promise<Judgment> {
    const judgment = lifecycleValue<Omit<Judgment, "passed">>(
      await this.#native.verify(
        verifier,
        strictJson("input", input),
        mediaJson(options.media),
      ),
    );
    return { ...judgment, passed: judgment.verdict === "passed" };
  }
}

/**
 * One invocation, and one Card-scoped view of it.
 *
 * Reached through {@link WyrdState.run}. Opening a run is local: no network IO
 * and no server-side Run resource. Views are immutable — {@link Run.forCard}
 * returns a sibling instead of retargeting this one — so concurrent emits
 * cannot observe a moved subject.
 */
export class Run {
  readonly #native: NativeRun;

  private constructor(native: NativeRun) {
    this.#native = native;
  }

  /** @internal Unwrap one closed native open result. */
  static fromOpen(open: NativeRunOpen): Run {
    return new Run(nativeHandle(open.run, open.error));
  }

  /** The UUIDv7 invocation identity this run and every view of it share. */
  get runId(): string {
    return this.#native.runId;
  }

  /**
   * The alias this view was opened with; {@link WyrdState.cardRef} returns
   * its exact typed reference.
   */
  get alias(): string {
    return this.#native.alias;
  }

  /** The emit surface for this view. */
  get observe(): Observe {
    return Observe.fromNative(this.#native);
  }

  /**
   * An immutable sibling view scoped to a registered alias.
   *
   * @throws a {@link WyrdError} when the bundle does not register `alias`. No
   * network IO occurs.
   */
  forCard(alias: string): Run {
    return Run.fromOpen(this.#native.forCard(alias));
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
  card(alias: string): RegisteredCard {
    return lifecycleValue<RegisteredCard>(this.#native.card(alias));
  }

  /** The exact Card reference for an alias. */
  cardRef(alias: string): CardRef {
    return lifecycleValue<CardRef>(this.#native.cardRef(alias));
  }

  /** The verified local artifacts for an alias. */
  artifacts(alias: string): readonly HydratedArtifact[] {
    return lifecycleValue<HydratedArtifact[]>(this.#native.artifacts(alias));
  }

  /**
   * Connect this state's one Bifrost writer and describe the fixed tables.
   *
   * The options are {@link Bifrost.connect}'s and resolve through the same
   * chain when omitted. Startup describes `vala.drift.observations` and
   * `vala.eval.observations` before resolving, so a run can never enqueue
   * against a missing, unauthorized, or incompatible system table. `table`
   * keeps its existing Bifrost meaning and does not choose a run's destination.
   * `clientByteLimitBytes` overrides the handle-wide ingestion byte budget
   * (256 MiB by default).
   *
   * @throws a {@link WyrdError} for a second start, a closed state, a byte
   * budget too small to seal one message (`WYRD_CLIENT_400_CONFIG_INVALID`), a
   * missing credential, an undialable ingest channel, or a fixed table that is
   * absent, unauthorized, or incompatible.
   */
  async startBifrost(
    options: {
      readonly table?: TableConfig;
      readonly serverUrl?: string;
      readonly credential?: string;
      readonly grpcUrl?: string;
      readonly clientByteLimitBytes?: number;
    } = {},
  ): Promise<void> {
    lifecycleValue<null>(
      await this.#native.startBifrost(
        options.table?.native,
        options.serverUrl,
        options.credential,
        options.grpcUrl,
        options.clientByteLimitBytes,
      ),
    );
  }

  /**
   * Open one invocation over this state, targeting `card` or the root Service.
   *
   * Local only: no network IO and no server-side Run resource. `card` selects
   * a registered alias before the run mints its `runId`; later
   * {@link Run.forCard} views share that id.
   *
   * @throws a {@link WyrdError} when `card` is not registered in this bundle.
   */
  run(card?: string): Run {
    return Run.fromOpen(this.#native.run(card));
  }

  /**
   * Drain every producer of this state's writer without closing it.
   *
   * The explicit durability barrier for a test or a finite job.
   *
   * @throws a {@link WyrdError} before startup, after shutdown, or for the
   * first producer or sink failure.
   */
  async flush(): Promise<void> {
    lifecycleValue<null>(await this.#native.flush());
  }

  /**
   * Drain every producer of this state's writer and close it to writes.
   *
   * Call this once at graceful shutdown, not after each observation: queue
   * admission is not a durable acknowledgement, so an abrupt exit before it
   * resolves can lose pending rows. After an ambiguous failure, retry
   * `shutdown()` on the same state rather than replacing the writer; a
   * successfully closed state stays closed.
   *
   * @throws a {@link WyrdError} for the first producer or sink failure.
   */
  async shutdown(): Promise<void> {
    lifecycleValue<null>(await this.#native.shutdown());
  }
}

/** Provider model identity, `{provider, model}`. */
export interface ModelRef {
  readonly provider: string;
  readonly model: string;
}

/** Gateway operation a deployment may serve. */
export type GatewayOperation =
  | "chat_completions"
  | "responses"
  | "embeddings"
  | "images"
  | "audio"
  | "batches";

/**
 * Redacted credential source. A tenant-submitted managed secret projects to
 * the bare discriminator. There is no write counterpart to this type: the SDK
 * reads provider credentials and never mutates one. Submitting, rotating,
 * revoking, and deleting a credential are CLI and scoped MCP administration
 * paths, because a name-only revoke or delete cannot be told apart from one
 * aimed at a managed secret.
 */
export type ProviderCredentialSourceView =
  | { readonly environment: { readonly binding: string } }
  | {
      readonly external_secret: {
        readonly backend: string;
        readonly reference: string;
      };
    }
  | "managed_secret";

/** Redacted provider credential as returned by every read and write. */
export interface ProviderCredentialView {
  readonly name: string;
  readonly provider: string;
  readonly source: ProviderCredentialSourceView;
  readonly state: "active" | "revoked";
  readonly created_at: string;
  readonly updated_at: string;
  readonly rotated_at: string | null;
  readonly revoked_at: string | null;
}

/** Provider wire adapter; built-in adapters require their reserved provider. */
export type ProviderAdapter =
  | "openai"
  | "anthropic"
  | "gemini"
  | {
      readonly vertex: { readonly project: string; readonly location: string };
    }
  | { readonly openai_compatible: { readonly base_url: string } };

/** How a deployment authenticates to its provider. */
export type ProviderAuth =
  | "none"
  | { readonly bearer: { readonly credential: string } }
  | {
      readonly api_key_header: {
        readonly header: string;
        readonly credential: string;
      };
    };

/** One named provider deployment. */
export interface ProviderDeployment {
  readonly name: string;
  readonly model: ModelRef;
  readonly adapter: ProviderAdapter;
  readonly auth: ProviderAuth;
  readonly capabilities: readonly GatewayOperation[];
  readonly routing_weight: number;
}

/** Scope a fallback rule applies to. */
export type FallbackScope =
  | "global"
  | { readonly operation: { readonly operation: GatewayOperation } }
  | { readonly model: { readonly model: ModelRef } };

/** Ordered fallback candidates for one scope. */
export interface FallbackRule {
  readonly scope: FallbackScope;
  readonly candidates: readonly ModelRef[];
}

/** Tenant fallback policy; the default has no rules. */
export interface GatewayFallbackPolicy {
  readonly rules: readonly FallbackRule[];
}

/** Subject a budget applies to. */
export type GatewayPolicySubject =
  | "tenant"
  | { readonly principal: { readonly principal_id: string } }
  | { readonly role: { readonly role_name: string } };

/** Subject a rate limit applies to. */
export type GatewayLimitSubject =
  | "tenant"
  | { readonly principal: { readonly principal_id: string } };

/** Provider or model a limit targets. */
export type GatewayPolicyTarget =
  | "all"
  | { readonly provider: { readonly provider: string } }
  | { readonly model: { readonly model: ModelRef } };

/** One rate limit; at least one ceiling is set. */
export interface GatewayLimit {
  readonly subject: GatewayLimitSubject;
  readonly target: GatewayPolicyTarget;
  readonly requests_per_minute: number | null;
  readonly tokens_per_minute: number | null;
  readonly concurrent_calls: number | null;
}

/** One spend budget; `amount` is a non-negative decimal string. */
export interface GatewayBudget {
  readonly subject: GatewayPolicySubject;
  readonly period: "calendar_day_utc" | "calendar_month_utc";
  readonly amount: string;
  readonly currency: string;
}

/** One priced usage dimension; `price` is a decimal string. */
export interface GatewayPriceRate {
  readonly dimension: string;
  readonly unit: string;
  readonly price: string;
}

/** One versioned model price list. */
export interface GatewayModelPricing {
  readonly model: ModelRef;
  readonly version: string;
  readonly currency: string;
  readonly effective_at: string;
  readonly active: boolean;
  readonly rates: readonly GatewayPriceRate[];
}

/** Tenant governance policy: limits, budgets, pricing, unknown-cost handling. */
export interface GatewayGovernancePolicy {
  readonly limits: readonly GatewayLimit[];
  readonly budgets: readonly GatewayBudget[];
  readonly pricing: readonly GatewayModelPricing[];
  readonly unknown_cost: "reject" | "allow_unpriced";
}

/** Capture mode for gateway calls. */
export type GatewayCaptureMode = "disabled" | "metadata" | "payload";

/** Capture policy write; payload fields are required only in payload mode. */
export interface GatewayCapturePolicyWrite {
  readonly mode: GatewayCaptureMode;
  readonly payload_fields: readonly ("request" | "response")[];
}

/** Versioned capture policy as stored by the server. */
export interface GatewayCapturePolicy extends GatewayCapturePolicyWrite {
  readonly version: number;
}

/**
 * Tenant gateway administration over the shared Rust `Gateway` handle.
 *
 * Bodies are validated in Rust and by the server; credential reads are always
 * redacted, and failures throw a structured {@link WyrdError}. A decode
 * failure names the argument and its position and quotes none of the body.
 *
 * Provider credential mutation is absent here and on the exported native
 * class beneath it, so no JavaScript value can reach a managed secret write
 * from this SDK. Use the Wyrd CLI or the scoped MCP tool to submit, rotate,
 * revoke, or delete a credential.
 */
export class Gateway {
  readonly #native: NativeGateway;

  private constructor(native: NativeGateway) {
    this.#native = native;
  }

  /**
   * Build an administration client without performing IO.
   *
   * Omitted options resolve through the same chain as {@link Bifrost.connect}.
   */
  static connect(
    options: {
      readonly serverUrl?: string;
      readonly credential?: string;
    } = {},
  ): Gateway {
    return new Gateway(connectGateway(options.serverUrl, options.credential));
  }

  /** Read one redacted provider credential. */
  async credential(name: string): Promise<ProviderCredentialView> {
    return lifecycleValue(await this.#native.credential(name));
  }

  /** List redacted provider credentials ordered by name. */
  async credentials(): Promise<ProviderCredentialView[]> {
    return lifecycleValue(await this.#native.credentials());
  }

  /** Create or replace a provider deployment. */
  async putDeployment(
    deployment: ProviderDeployment,
  ): Promise<ProviderDeployment> {
    return lifecycleValue(
      await this.#native.putDeployment(JSON.stringify(deployment)),
    );
  }

  /** Read one provider deployment. */
  async deployment(name: string): Promise<ProviderDeployment> {
    return lifecycleValue(await this.#native.deployment(name));
  }

  /** List provider deployments ordered by name. */
  async deployments(): Promise<ProviderDeployment[]> {
    return lifecycleValue(await this.#native.deployments());
  }

  /** Delete a provider deployment; an absent name succeeds. */
  async deleteDeployment(name: string): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteDeployment(name));
  }

  /** Replace the tenant fallback policy. */
  async putFallbackPolicy(
    policy: GatewayFallbackPolicy,
  ): Promise<GatewayFallbackPolicy> {
    return lifecycleValue(
      await this.#native.putFallbackPolicy(JSON.stringify(policy)),
    );
  }

  /** Read the tenant fallback policy, or the default. */
  async fallbackPolicy(): Promise<GatewayFallbackPolicy> {
    return lifecycleValue(await this.#native.fallbackPolicy());
  }

  /** Restore the default fallback policy. */
  async deleteFallbackPolicy(): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteFallbackPolicy());
  }

  /** Replace the tenant governance policy. */
  async putGovernancePolicy(
    policy: GatewayGovernancePolicy,
  ): Promise<GatewayGovernancePolicy> {
    return lifecycleValue(
      await this.#native.putGovernancePolicy(JSON.stringify(policy)),
    );
  }

  /** Read the tenant governance policy, or the default. */
  async governancePolicy(): Promise<GatewayGovernancePolicy> {
    return lifecycleValue(await this.#native.governancePolicy());
  }

  /** Restore the default governance policy. */
  async deleteGovernancePolicy(): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteGovernancePolicy());
  }

  /** Replace the tenant capture policy; returns its versioned view. */
  async putCapturePolicy(
    policy: GatewayCapturePolicyWrite,
  ): Promise<GatewayCapturePolicy> {
    return lifecycleValue(
      await this.#native.putCapturePolicy(JSON.stringify(policy)),
    );
  }

  /** Read the tenant capture policy, or the disabled default. */
  async capturePolicy(): Promise<GatewayCapturePolicy> {
    return lifecycleValue(await this.#native.capturePolicy());
  }
}

/** One loader diagnostic reported by {@link cli.plan}. */
export interface PlanDiagnostic {
  readonly code: string;
  readonly status: number;
  readonly severity: string;
  readonly path: string;
  readonly span?: unknown;
  readonly message: string;
  readonly remediation: string;
  readonly details?: unknown;
}

/** Deterministic local registration plan printed by `wyrd plan --format json`. */
export interface PlanReport {
  readonly ok: boolean;
  readonly cards: readonly {
    readonly kind: string;
    readonly space: string | null;
    readonly name: string;
    readonly version: string | null;
  }[];
  readonly diagnostics: readonly PlanDiagnostic[];
}

/** Result of `wyrd load --format json`. */
export interface LoadOutput {
  readonly card_ref: CardRef;
  readonly materialized: boolean;
}

/** Card selector: a `uid` with `kind`, or `kind`, `space`, and `name`; `version` narrows either. */
export interface CliCardSelector {
  readonly kind?: string;
  readonly space?: string;
  readonly name?: string;
  readonly version?: string;
  readonly uid?: string;
}

/** Response of `wyrd auth issue-key`; `key` is the plaintext API key, returned exactly once. */
export interface IssueKeyResponse {
  readonly key_id: string;
  readonly key: string;
  readonly prefix: string;
  readonly card_ref: CardRef;
  readonly created_at: string;
  readonly expires_at: string;
}

/** Provider credential submission for {@link cli.putProviderCredential}. */
export interface ProviderCredentialWrite {
  readonly name: string;
  readonly provider: string;
  readonly source:
    | { readonly environment: { readonly binding: string } }
    | {
        readonly external_secret: {
          readonly backend: string;
          readonly reference: string;
        };
      }
    | { readonly managed_secret: { readonly secret: string } };
}

/** Endpoint override shared by the networked {@link cli} commands. */
export interface CliServerOptions {
  /** Wyrd server base URL; the credential always comes from the ambient chain. */
  readonly server?: string;
}

/**
 * The `wyrd` CLI in process: the same Rust command implementation the
 * installed `wyrd` executable runs.
 *
 * Each command takes its options as typed arguments, returns the value the
 * command prints with `--format json`, and throws a {@link WyrdError} instead
 * of exiting. Networked commands read their credential from the ambient chain
 * (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
 * `credentials.toml`), never from an argument; `server` re-points only the
 * endpoint.
 */
export const cli = {
  /** Validate a local Card tree without contacting a server (`wyrd plan`). */
  plan(path: string): PlanReport {
    return lifecycleValue(cliPlan(path));
  },

  /** Register a local Card tree and return its receipt (`wyrd apply`). */
  async apply(path: string, options: CliServerOptions = {}): Promise<RegistrationReceipt> {
    return lifecycleValue(await cliApply(path, options.server));
  },

  /** Hydrate a Card's reachable graph into `outputDir` (`wyrd get`). */
  async get(
    selector: CliCardSelector,
    outputDir: string,
    options: CliServerOptions & { readonly metadataOnly?: boolean } = {},
  ): Promise<HydrationSummary> {
    return lifecycleValue(
      await cliGet(selector, outputDir, options.metadataOnly, options.server),
    );
  },

  /** Load one Card and materialize its artifacts (`wyrd load`). */
  async load(
    selector: CliCardSelector,
    options: CliServerOptions & { readonly path?: string } = {},
  ): Promise<LoadOutput> {
    return lifecycleValue(await cliLoad(selector, options.path, options.server));
  },

  /** Issue an API key bound to one exact Card (`wyrd auth issue-key`). */
  async issueKey(
    card: {
      readonly kind: string;
      readonly name: string;
      readonly version: string;
      readonly space: string;
      readonly label?: string;
      readonly expiresInSeconds?: number;
    },
    options: CliServerOptions = {},
  ): Promise<IssueKeyResponse> {
    return lifecycleValue(await cliIssueKey(card, options.server));
  },

  /**
   * Create or rotate a provider credential (`wyrd gateway credential put`).
   * A rejected body is reported without quoting it; the view is redacted.
   */
  async putProviderCredential(
    write: ProviderCredentialWrite,
    options: CliServerOptions = {},
  ): Promise<ProviderCredentialView> {
    return lifecycleValue(
      await cliPutProviderCredential(JSON.stringify(write), options.server),
    );
  },

  /** Terminally revoke a provider credential (`wyrd gateway credential revoke`). */
  async revokeProviderCredential(
    name: string,
    options: CliServerOptions = {},
  ): Promise<ProviderCredentialView> {
    return lifecycleValue(await cliRevokeProviderCredential(name, options.server));
  },

  /** Delete an unreferenced provider credential; an absent name succeeds. */
  async deleteProviderCredential(
    name: string,
    options: CliServerOptions = {},
  ): Promise<void> {
    lifecycleValue<null>(await cliDeleteProviderCredential(name, options.server));
  },
};
