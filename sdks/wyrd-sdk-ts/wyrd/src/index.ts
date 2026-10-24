import type { Span as OtelSpan } from "@opentelemetry/api";
import { DataType, Table, tableFromIPC, tableToIPC, type Field, type RecordBatch, type Schema } from "apache-arrow";
import { AsyncLocalStorage } from "node:async_hooks";
import { createRequire } from "node:module";

import type { WyrdErrorCode } from "./error-codes.js";
import type {
  BifrostTableDescription as TableDescriptionWire,
  BifrostTableEntry as TableEntry,
  CancelRunningQueryResponse,
  CardRef,
  CardDeclarationOutcome,
  CardSummary,
  CompactionTypeWire as CompactionType,
  CreateOperatorConnectionRequest,
  CreateServicePrincipalRequest,
  CreateServicePrincipalResponse,
  CredentialListResponse as CredentialList,
  CredentialMetadata,
  DataTypeSpec,
  FallbackRule,
  FallbackScope,
  FieldSpec2 as FieldDescription,
  GatewayBudget,
  GatewayCaptureMode,
  GatewayCapturePolicy,
  GatewayCapturePolicyWrite,
  GatewayFallbackPolicy,
  GatewayGovernancePolicy,
  GatewayLimit,
  GatewayLimitSubject,
  GatewayModelPricing,
  GatewayOperation,
  GatewayPolicySubject,
  GatewayPolicyTarget,
  GatewayPriceRate,
  HttpAuthScheme,
  HttpConnectionAuth,
  IssuedCredential,
  JsonValue,
  ListCardsRequest,
  ListCardsResponse,
  ModelRef,
  OperatorConnectionStatus,
  OperatorConnectionView,
  PartitionFieldWire as PartitionField,
  PhysicalLayoutWire as PhysicalLayout,
  PrincipalKindTag as PrincipalKind,
  PrincipalPage,
  PrincipalQuery,
  PrincipalRoles,
  PrincipalSummary,
  ProviderAdapter,
  ProviderAuth,
  ProviderCredentialSourceView,
  ProviderCredentialView,
  ProviderDeployment,
  QueryClass,
  QueryParam,
  QueryTerminalFrame as QueryTerminal,
  RegisteredAgentCard,
  RegisteredCard,
  RegisteredDataCard,
  RegisteredModelCard,
  RegisteredPromptCard,
  RegisteredServiceCard,
  RegisteredVerifierCard,
  DeclarationReceipt,
  RevokePrincipalRequest,
  RoleAssignment,
  RoleAssignmentChange,
  RoleSource,
  RunningQueryLifecycleState,
  RunningQuerySummary,
  SortKeyWire as SortKey,
  SpanRecord,
  TokenAudience,
  UpdateOperatorConnectionRequest,
  NamedResponseFormatVariant2 as VerdictResponseFormat,
  RequirementResult as WireRequirementResult,
  VerificationExecutionState as WireVerificationExecutionState,
  VerificationOutcome as WireVerificationOutcome,
  VerificationResult as WireVerificationResult,
  WorkloadRestriction,
} from "./card-types.js";

import type {
  NativeBifrostQueryStream,
  NativeLifecycleResult,
  NativeQueryStep,
  NativeExecution,
  NativeVerifierInput,
} from "../index.cjs";

const require = createRequire(import.meta.url);
const nativeBinding = require("../index.cjs") as typeof import("../index.cjs");
const {
  connectWyrdClient,
  openWyrdState,
  storedTimestampText,
  tableConfigFromArrowIpc,
  tableConfigFromJsonSchema,
  telemetryFlushError,
  telemetryProviderExists,
  timestampFormat,
  timestampKinds,
  timestampText,
  variantToValue,
} = nativeBinding;
type NativeBifrost = import("../index.cjs").NativeBifrost;
type NativeCards = import("../index.cjs").NativeCards;
type NativeOperatorConnections = import("../index.cjs").NativeOperatorConnections;
type NativePrincipals = import("../index.cjs").NativePrincipals;
type NativeGateway = import("../index.cjs").NativeGateway;
type NativeWyrdClient = import("../index.cjs").NativeWyrdClient;
type NativeWyrdState = import("../index.cjs").NativeWyrdState;
type NativeRun = import("../index.cjs").NativeRun;
type NativeRunOpen = import("../index.cjs").NativeRunOpen;
type NativeTableConfig = import("../index.cjs").NativeTableConfig;

/** Aggregate participant progress of one active query, in the SDK's camelCase projection. */
export interface RunningQueryProgress {
  readonly completedParticipants: number;
  readonly totalParticipants: number;
}

/** One active query visible to the tenant, projected from the `RunningQuerySummary` wire contract. */
export interface RunningQuery {
  readonly requestId: string;
  readonly queryClass: QueryClass;
  readonly startedAt: string;
  readonly deadline: string;
  readonly state: RunningQueryLifecycleState;
  readonly progress: RunningQueryProgress;
  readonly cancellationRequested: boolean;
}

/** Idempotent cancellation result, projected from the `CancelRunningQueryResponse` wire contract. */
export interface CancelRunningQueryResult {
  readonly requestId: string;
  readonly cancellationStarted: boolean;
}

export type { WyrdErrorCode } from "./error-codes.js";
/** Wire contracts generated from the `wyrd-spec` Rust types, under their SDK names. */
export type {
  CardDeclarationOutcome,
  CardSummary,
  CompactionType,
  CreateOperatorConnectionRequest,
  CreateServicePrincipalRequest,
  CreateServicePrincipalResponse,
  CredentialList,
  CredentialMetadata,
  DataTypeSpec,
  FallbackRule,
  FallbackScope,
  FieldDescription,
  GatewayBudget,
  GatewayCaptureMode,
  GatewayCapturePolicy,
  GatewayCapturePolicyWrite,
  GatewayFallbackPolicy,
  GatewayGovernancePolicy,
  GatewayLimit,
  GatewayLimitSubject,
  GatewayModelPricing,
  GatewayOperation,
  GatewayPolicySubject,
  GatewayPolicyTarget,
  GatewayPriceRate,
  HttpAuthScheme,
  HttpConnectionAuth,
  IssuedCredential,
  JsonValue,
  ListCardsRequest,
  ListCardsResponse,
  ModelRef,
  OperatorConnectionStatus,
  OperatorConnectionView,
  PartitionField,
  PhysicalLayout,
  PrincipalKind,
  PrincipalPage,
  PrincipalQuery,
  PrincipalRoles,
  PrincipalSummary,
  ProviderAdapter,
  ProviderAuth,
  ProviderCredentialSourceView,
  ProviderCredentialView,
  ProviderDeployment,
  QueryParam,
  QueryTerminal,
  DeclarationReceipt,
  RevokePrincipalRequest,
  RoleAssignment,
  RoleAssignmentChange,
  RoleSource,
  SortKey,
  TableEntry,
  TokenAudience,
  UpdateOperatorConnectionRequest,
};
export type {
  AgentSpec,
  CardKind,
  CardRef,
  DataSpec,
  DriftBaselineState,
  DriftBaselineStatus,
  ExecutionRoute,
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
  SpanRecord,
  Status as CardStatus,
  TriggerSpec,
  TypedCardKind,
  VerificationError,
  VerificationExecutionStatus,
  VerificationStatus,
  VerificationVerdict,
  VerifierSpec,
  WorkloadKind,
  WorkloadRestriction,
  WorkloadRestrictionFamily,
  WorkloadRestrictionUnrestricted,
} from "./card-types.js";
/** The JSON custom `data` a requirement carries when read without a type. */
export type VerdictData = NonNullable<WireRequirementResult["data"]>;

/** The judgment of one requirement, its custom `data` typed `T`. */
export type RequirementResult<T = VerdictData> = Omit<WireRequirementResult, "data"> & { readonly data?: T };

/** Overall verdict with its requirement and step breakdown, custom `data` typed `T`. */
export type VerificationOutcome<T = VerdictData> = Omit<WireVerificationOutcome, "requirements"> & {
  readonly requirements: readonly RequirementResult<T>[];
};

/** The attributable result of one Verifier execution, custom `data` typed `T`. */
export type VerificationResult<T = VerdictData> = Omit<WireVerificationResult, "outcome"> & {
  readonly outcome: VerificationOutcome<T>;
};

/** A verification execution's status, its result's custom `data` typed `T`. */
export type VerificationExecutionState<T = VerdictData> = Omit<WireVerificationExecutionState, "result"> & {
  readonly result?: VerificationResult<T> | null;
};

/**
 * Anything that turns unknown custom Verdict `data` into a `T`, throwing when
 * it does not fit. A Zod schema satisfies it by construction; no schema
 * library is a dependency of this SDK.
 */
export interface VerdictDataParser<T> {
  parse(value: unknown): T;
}

/**
 * Decode every present requirement `data` of `result` with `parser`.
 *
 * Absent data stays absent; failed requirements decode too. Whatever the
 * parser throws, the first mismatch raises `WYRD_SDK_422_VERIFICATION_DATA_INVALID`
 * without returning a partly typed result. It names the requirement and the
 * data root's empty `path`, never the data, the thrown value, or a deeper
 * path, which may hold a returned dictionary key.
 */
function typedResult<T>(result: VerificationResult, parser: VerdictDataParser<T>): VerificationResult<T> {
  const requirements = result.outcome.requirements.map((requirement): RequirementResult<T> => {
    const { data, ...rest } = requirement;
    if (data === undefined) {
      return rest;
    }
    try {
      return { ...rest, data: parser.parse(data) };
    } catch {
      throw new WyrdError(
        "WYRD_SDK_422_VERIFICATION_DATA_INVALID",
        422,
        "Verification data does not match the requested type",
        `requirement \`${requirement.id}\` data does not match the requested type`,
        "Match the requested type to the check's declared data schema, use a union or decoder for mixed shapes, or read the result without a type.",
        { requirement: requirement.id, path: "" },
      );
    }
  });
  return { ...result, outcome: { ...result.outcome, requirements } };
}

/**
 * The `verdict` response format for a Prompt's `response_format` or a code
 * check's `check.response_format`, with `data` as the custom data schema.
 *
 * `data` is a JSON Schema document or a schema that emits one, such as a Zod
 * object, so the same schema can also parse typed results. Omitted, the
 * format is the plain Verdict.
 *
 * @param data - The custom data's object schema.
 * @returns The expanded `{ kind: "verdict", data }` format.
 */
export function verdictFormat(data?: Readonly<Record<string, unknown>> | JsonSchemaSource): VerdictResponseFormat {
  return data === undefined
    ? { kind: "verdict" }
    : { kind: "verdict", data: JSON.parse(jsonSchemaText(data)) as VerdictResponseFormat["data"] };
}

/** Every type generated from the `wyrd-spec` Card schema, including nested spec types. */
export type * as CardTypes from "./card-types.js";

/**
 * Every failure the SDK raises: one stable catalog code plus its HTTP status,
 * title, detail, remediation, and structured details, read as properties
 * rather than parsed from the message.
 */
export class WyrdError extends Error {
  readonly code: WyrdErrorCode;
  readonly status: number;
  readonly title: string;
  readonly detail: string;
  readonly remediation: string | undefined;
  /** JSON-safe structured diagnostics supplied by the originating Wyrd error. */
  readonly details: unknown;

  /**
   * Build one catalog error; the SDK raises these, and a test double may too.
   *
   * @param code - Stable catalog code, such as `WYRD_SPEC_400_VALIDATION`.
   * @param status - HTTP-equivalent status.
   * @param title - Stable catalog title.
   * @param detail - Scrubbed human-readable detail, also the `message`.
   * @param remediation - Operator-facing remediation, when the catalog has one.
   * @param details - JSON-safe structured diagnostics, when present.
   */
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

/** A query stream that ended before its validated terminal frame. */
export class IncompleteQueryStreamError extends WyrdError {
  /**
   * Build the `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE` error.
   *
   * @param status - HTTP-equivalent status.
   * @param title - Stable catalog title.
   * @param detail - Scrubbed human-readable detail.
   * @param remediation - Operator-facing remediation, when present.
   * @param details - JSON-safe structured diagnostics, when present.
   */
  constructor(status: number, title: string, detail: string, remediation?: string, details?: unknown) {
    super("WYRD_VALA_502_QUERY_STREAM_INCOMPLETE", status, title, detail, remediation, details);
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
  const details = metadata.errorDetailsJson == null ? undefined : (JSON.parse(metadata.errorDetailsJson) as unknown);
  if (metadata.errorCode === "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE") {
    return new IncompleteQueryStreamError(status, title, detail, remediation, details);
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

/**
 * Server projection of one registered table's stored physical schema.
 *
 * `table_version` is the exact version the schema and layout describe, as a
 * `bigint`; its canonical decimal string is only the wire spelling.
 */
export type TableDescription = Omit<TableDescriptionWire, "table_version"> & { readonly table_version: bigint };

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
function nativeHandle<T>(value: T | null | undefined, error: NativeErrorMetadata | null | undefined): T {
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

function runningQuery(wire: RunningQuerySummary): RunningQuery {
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

/**
 * One running query's Arrow batches, iterated as they arrive.
 *
 * Returned by {@link Bifrost.stream}; `for await` drains it, and the
 * validated terminal frame is read from {@link BifrostQueryStream.terminal}
 * once iteration completes.
 */
export class BifrostQueryStream implements AsyncIterableIterator<RecordBatch> {
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

  /** The validated terminal frame, present once iteration has completed. */
  get terminal(): QueryTerminal | undefined {
    return this.#terminal;
  }

  /** The server's canonical request ID, for {@link Bifrost.status} and {@link Bifrost.cancel}. */
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
    return ipc === null || ipc === undefined ? undefined : tableFromIPC(ipc).schema;
  }

  /**
   * @internal Each top-level timestamp column's Bifrost timestamp type,
   * decided in Rust from the retained schema; empty until the stream completes.
   */
  get timestampKinds(): Readonly<Record<string, string>> {
    const ipc = this.#native.schemaIpc;
    return ipc === null || ipc === undefined ? {} : timestampKinds(ipc);
  }

  /**
   * Iterate this stream with `for await`.
   *
   * @returns This stream.
   */
  [Symbol.asyncIterator](): AsyncIterableIterator<RecordBatch> {
    return this;
  }

  /**
   * Read the next batch, or complete once the validated terminal arrives.
   *
   * @returns The next batch, or `done` after the terminal frame.
   * @throws {@link WyrdError} for a failed query, and
   * {@link IncompleteQueryStreamError} for a stream that ends without its
   * terminal; the stream is closed either way.
   */
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

  /**
   * Stop iterating and close the response, cancelling the transfer.
   *
   * @returns `done`.
   */
  async return(): Promise<IteratorResult<RecordBatch>> {
    this.#done = true;
    await this.#native.close();
    return { done: true, value: undefined };
  }
}

/**
 * The physical layout a table asks the server to create.
 *
 * Partition fields and sort keys reuse {@link PartitionField} and
 * {@link SortKey} — the shapes the describe route returns — so a layout read
 * back from the server can be declared again unchanged. `partitionBy` holds
 * one `hour` or `day` field on the time column plus at most two `identity`,
 * `bucket`, or `truncate` dimensions; omitted or empty, the table partitions
 * hourly on the time column. `timeColumn` names the non-null UTC timestamp
 * column that partitions the table and leads its default sort order; omitted,
 * it is `wyrd_ingested_at`. Any instant is accepted. `bloomColumns` is the
 * complete Bloom-filtered set; omitted or empty, no column is filtered.
 */
export interface TableLayout {
  partitionBy?: readonly PartitionField[];
  timeColumn?: string;
  sortKeys?: readonly SortKey[];
  bloomColumns?: readonly string[];
}

/** Optional per-row correlation; an omitted field is a null on the wire. */
export interface Correlation {
  cardUid?: string;
  runId?: string;
}

/** The server-minted identity of a registered table. */
export interface ResolvedTable {
  readonly tableUid: string;
  readonly fingerprint: string;
}

/** A JSON Schema document, or a schema that emits one, as JSON text. */
function jsonSchemaText(schema: Readonly<Record<string, unknown>> | JsonSchemaSource): string {
  const source = schema as JsonSchemaSource;
  return JSON.stringify(
    typeof source.toJSONSchema === "function" ? source.toJSONSchema({ override: zodTimestampFormat }) : schema,
  );
}

function layoutJson(layout?: TableLayout): string | undefined {
  if (layout === undefined) {
    return undefined;
  }
  return JSON.stringify({
    partition_by: layout.partitionBy ?? [],
    time_column: layout.timeColumn,
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
  toJSONSchema(params?: {
    override?(context: { zodSchema: unknown; jsonSchema: Record<string, unknown> }): void;
  }): Readonly<Record<string, unknown>>;
}

/**
 * Mark a Zod local date-time with the `TIMESTAMP_NTZ` format.
 *
 * Zod's `toJSONSchema` hook, read structurally so Zod stays out of this SDK's
 * dependencies. `z.iso.datetime()` already declares `date-time`, which is
 * `TIMESTAMP_LTZ`; a local-only one declares no format, so without this it
 * would register as text. Other schema libraries never call the hook.
 */
function zodTimestampFormat(context: { zodSchema: unknown; jsonSchema: Record<string, unknown> }): void {
  const def = (
    context.zodSchema as {
      _zod?: { def?: { format?: string; local?: boolean; offset?: boolean } };
    }
  )._zod?.def;
  if (def?.format === "datetime" && def.local === true && def.offset !== true) {
    context.jsonSchema.format = TimestampNTZ.format;
  }
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
   * Time fields declare Wyrd timestamp columns: `TimestampNTZ.schema(z)`,
   * `TimestampLTZ.schema(z)`, and `TimestampTZ.schema(z)` declare their own
   * type, `z.iso.datetime()` declares `TIMESTAMP_LTZ`, and
   * `z.iso.datetime({ local: true })` declares `TIMESTAMP_NTZ`. Zod refuses to
   * describe `z.date()` in JSON Schema, so it is refused here too.
   *
   * `options.layout` requests the table's physical layout.
   * `options.compactionTargetFileSizeBytes` pins the table's Forge compaction
   * file target; omitted, the new table stores the server's deployment
   * default, resolved once at creation.
   * Registration records it once, and a later registration naming a different
   * target is refused rather than silently changing it.
   *
   * `options.compactionType` chooses the table's Forge compaction type;
   * omitted, Forge compacts it `small-files`. Like the target, it is recorded
   * once and a later registration naming a different type is refused with
   * `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH`.
   *
   * @param table - The table's `namespace.name`.
   * @param schema - A JSON Schema document, or a schema that emits one.
   * @param options - Optional layout and Forge compaction declarations.
   * @returns The declared, still unregistered, table config.
   * @throws {@link WyrdError} when the resulting document does not map to an
   * Arrow schema (`WYRD_VALA_400_SCHEMA_PARSE`, including a model that allows
   * extra keys), declares a column the write path already owns, the
   * compaction target is not a non-negative integer, or the compaction type
   * is not a known spelling (`WYRD_SPEC_400_VALIDATION`).
   */
  static fromJsonSchema(
    table: string,
    schema: Readonly<Record<string, unknown>> | JsonSchemaSource,
    options: TableConfigOptions = {},
  ): TableConfig {
    const declared = tableConfigFromJsonSchema(
      table,
      jsonSchemaText(schema),
      layoutJson(options.layout),
      options.compactionTargetFileSizeBytes,
      options.compactionType,
    );
    return new TableConfig(nativeHandle(declared.config, declared.error));
  }

  /**
   * Declare a table's columns from an explicit Arrow schema.
   *
   * Use this for column types JSON Schema cannot express, such as `Int32`, a
   * non-UTC timestamp, or a decimal. `schema` holds user columns only.
   *
   * @param table - The table's `namespace.name`.
   * @param schema - The user columns as an `apache-arrow` schema.
   * @param options - Optional layout and Forge compaction declarations, as
   * for {@link TableConfig.fromJsonSchema}.
   * @returns The declared, still unregistered, table config.
   * @throws {@link WyrdError} `WYRD_SPEC_400_VALIDATION` when the table is
   * not `namespace.name`, a column is one the write path already owns, or a
   * compaction option is invalid.
   */
  static fromArrow(table: string, schema: Schema, options: TableConfigOptions = {}): TableConfig {
    const declared = tableConfigFromArrowIpc(
      table,
      Buffer.from(tableToIPC(new Table(schema), "stream")),
      layoutJson(options.layout),
      options.compactionTargetFileSizeBytes,
      options.compactionType,
    );
    return new TableConfig(nativeHandle(declared.config, declared.error));
  }

  /**
   * Fetch an already-registered table's config by name.
   *
   * @param table - The table's `namespace.name`.
   * @param options - `client` to describe as; omitted, the ambient client
   * ({@link WyrdClient.connect} with no options) is used.
   * @returns The described config, carrying its server identity.
   * @throws {@link WyrdError} when no credential resolves, the server is
   * unreachable, or the table is unknown or unauthorized.
   */
  static async describe(table: string, options: ClientOptions = {}): Promise<TableConfig> {
    const described = await wyrdClientNative(clientOf(options)).describeTableConfig(table);
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

/** Brands a timestamp text with the Bifrost timestamp type it was checked as. */
declare const timestampBrand: unique symbol;

/** `TIMESTAMP_NTZ` text: an RFC 3339 wall-clock reading with no offset. */
export type TimestampNTZ = string & { readonly [timestampBrand]: "TIMESTAMP_NTZ" };
/** `TIMESTAMP_LTZ` text: one RFC 3339 instant, rendered in UTC. */
export type TimestampLTZ = string & { readonly [timestampBrand]: "TIMESTAMP_LTZ" };
/** `TIMESTAMP_TZ` text: one RFC 3339 instant in the writer's offset. */
export type TimestampTZ = string & { readonly [timestampBrand]: "TIMESTAMP_TZ" };

/**
 * A Zod-style namespace, named structurally so this SDK never depends on Zod.
 *
 * Zod's `meta` and `refine` both return the string schema they are called on,
 * so a field built from it has the type `meta` returns.
 */
export interface StringSchemaFactory {
  string(): { meta(meta: Record<string, unknown>): unknown };
}

/** The string schema type a {@link StringSchemaFactory} builds. */
export type StringSchemaOf<Z extends StringSchemaFactory> = ReturnType<ReturnType<Z["string"]>["meta"]>;

/**
 * One Wyrd timestamp type: its name, its JSON Schema format, and the checks
 * that make a text a value of it.
 */
export interface WyrdTimestampType<T extends string> {
  /** `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ`, or `TIMESTAMP_TZ`. */
  readonly name: string;
  /** The JSON Schema `format` that declares a column of this type. */
  readonly format: string;
  /**
   * Check `text` and return its canonical text.
   *
   * The text is parsed by the same Rust parser an insert uses, so a value this
   * accepts is one Bifrost accepts.
   *
   * @throws TypeError when the text is not this type: an offset for
   * `TIMESTAMP_NTZ`, no offset for the others, or not RFC 3339.
   */
  parse(text: string): T;
  /**
   * A string schema for a field of this type, built with the caller's Zod.
   *
   * It carries this type's format, so a table declared from it registers this
   * column type, and it accepts only text {@link WyrdTimestampType.parse}
   * accepts.
   */
  schema<Z extends StringSchemaFactory>(z: Z): StringSchemaOf<Z>;
}

/**
 * Build one Wyrd timestamp type from its name.
 *
 * The format, the check, and the canonical text all come from the Rust owner,
 * so the three types restate nothing.
 */
function wyrdTimestamp<T extends string>(name: string): WyrdTimestampType<T> {
  const format = timestampFormat(name) as string;
  const accepts = (text: string): boolean => timestampText(name, text) != null;
  return {
    name,
    format,
    parse(text: string): T {
      const canonical = timestampText(name, text);
      if (canonical == null) {
        throw new TypeError(`${name} does not accept ${JSON.stringify(text)}`);
      }
      return canonical as T;
    },
    schema<Z extends StringSchemaFactory>(z: Z): StringSchemaOf<Z> {
      const field = z.string().meta({ format }) as {
        refine(check: (value: string) => boolean, message: string): StringSchemaOf<Z>;
      };
      return field.refine(accepts, `not a ${name}`);
    },
  };
}

/** `TIMESTAMP_NTZ`: a wall-clock reading with no instant. */
export const TimestampNTZ = wyrdTimestamp<TimestampNTZ>("TIMESTAMP_NTZ");
/** `TIMESTAMP_LTZ`: one instant. */
export const TimestampLTZ = wyrdTimestamp<TimestampLTZ>("TIMESTAMP_LTZ");
/** `TIMESTAMP_TZ`: one instant plus the writer's wall-clock reading. */
export const TimestampTZ = wyrdTimestamp<TimestampTZ>("TIMESTAMP_TZ");

/**
 * One stored timestamp cell as its Wyrd timestamp type's text.
 *
 * Apache Arrow hands a microsecond timestamp over as fractional milliseconds,
 * which round back to the exact microseconds; a `TIMESTAMP_TZ` cell is its
 * `{utc, local}` struct. Rust renders the text.
 */
function timestampValue(kind: string, value: unknown): unknown {
  if (value === null || value === undefined) {
    return value;
  }
  const micros = (millis: unknown): number => Math.round((millis as number) * 1000);
  const stored = value as { utc: unknown; local: unknown };
  const text =
    typeof value === "number"
      ? storedTimestampText(kind, micros(value))
      : storedTimestampText(kind, micros(stored.utc), micros(stored.local));
  if (text == null) {
    throw new TypeError(`stored ${kind} value is out of range`);
  }
  return text;
}

/** The Arrow extension name every Variant column carries. */
const VARIANT_EXTENSION = "arrow.parquet.variant";

/**
 * Project one row value onto its native JavaScript value.
 *
 * A Variant cell decodes through the shared Rust owner (objects become plain
 * objects, arrays arrays, and an integer outside the safe range a `bigint`),
 * a Struct becomes a plain object, a List an array, and a Map an object keyed
 * by its entry keys, recursively. Every scalar stays as Apache Arrow produced
 * it, so 64-bit integers are `bigint`.
 */
function nativeValue(field: Field, value: unknown): unknown {
  if (value === null || value === undefined) {
    return value;
  }
  if (field.metadata.get("ARROW:extension:name") === VARIANT_EXTENSION) {
    const cell = value as { metadata: Uint8Array; value: Uint8Array };
    return variantToValue(cell.metadata, cell.value);
  }
  const children = field.type.children as Field[] | null;
  if (DataType.isStruct(field.type)) {
    const struct = value as Record<string, unknown>;
    return Object.fromEntries((children ?? []).map((child) => [child.name, nativeValue(child, struct[child.name])]));
  }
  if (DataType.isList(field.type)) {
    return Array.from(value as Iterable<unknown>, (item) => nativeValue(children?.[0] as Field, item));
  }
  if (DataType.isMap(field.type)) {
    const entry = children?.[0]?.type.children as Field[];
    return Object.fromEntries(
      Array.from(value as Iterable<[unknown, unknown]>, ([key, item]) => [
        String(key),
        nativeValue(entry[1] as Field, item),
      ]),
    );
  }
  return value;
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

  /**
   * Hold one completed query's batches; {@link Bifrost.sql} builds these.
   *
   * @param batches - Every batch the query produced, in arrival order.
   * @param terminal - The validated terminal frame.
   * @param schema - The server-supplied result schema.
   */
  constructor(batches: readonly RecordBatch[], terminal: QueryTerminal, schema: Schema) {
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
   *
   * @returns The batches as one Arrow `Table`.
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
   *
   * @returns One Arrow IPC stream.
   */
  toBytes(): Uint8Array {
    return tableToIPC(this.toArrow(), "stream");
  }
}

/** Reads a {@link Bifrost}'s native handle for {@link record}. */
let bifrostNative: (bifrost: Bifrost) => NativeBifrost;

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

  static {
    bifrostNative = (bifrost) => bifrost.#native;
  }

  private constructor(native: NativeBifrost) {
    this.#native = native;
  }

  /** @internal Wrap one native handle; {@link Bifrost.connect} is the public constructor. */
  static fromNative(native: NativeBifrost): Bifrost {
    return new Bifrost(native);
  }

  /**
   * Connect one client, optionally already bound to a write target.
   *
   * Connecting performs IO, so this is a static factory rather than a
   * constructor.
   *
   * @param options - `client` to read and write as, possibly delegated;
   * omitted, the ambient client ({@link WyrdClient.connect} with no options)
   * is used. `table` binds the write target. `clientByteLimitBytes` overrides
   * the handle-wide ingestion byte budget (256 MiB by default).
   * @returns The connected client.
   * @throws {@link WyrdError}: `WYRD_CLIENT_401_NO_CREDENTIALS` when no
   * credential resolves, `WYRD_CLIENT_400_CONFIG_INVALID` for a byte budget
   * too small to seal one message, or the transport error when the ingest
   * channel cannot be dialed.
   */
  static async connect(
    options: ClientOptions & { readonly table?: TableConfig; readonly clientByteLimitBytes?: number } = {},
  ): Promise<Bifrost> {
    const connection = await wyrdClientNative(clientOf(options)).connectBifrost(
      options.table?.native,
      options.clientByteLimitBytes,
    );
    return new Bifrost(nativeHandle(connection.bifrost, connection.error));
  }

  /**
   * Create the active table.
   *
   * @returns `created`, or `already_exists` for an identical registration.
   * @throws {@link WyrdError} with no active table, for a fingerprint or
   * compaction conflict, or for a transport or authorization failure.
   */
  async register(): Promise<"created" | "already_exists"> {
    return lifecycleValue<"created" | "already_exists">(await this.#native.register());
  }

  /**
   * Bind `table` as the write target, returning the previous binding.
   *
   * The previous table's producer stays pooled, so its buffered rows still
   * flush; a swap loses nothing.
   *
   * @param table - The new write target.
   * @returns The previous binding, or undefined when there was none.
   * @throws {@link WyrdError} only for a config this SDK did not build.
   */
  useTable(table: TableConfig): TableConfig | undefined {
    const previous = this.#native.useTable(table.native);
    return previous === null || previous === undefined ? undefined : TableConfig.fromNative(previous);
  }

  /**
   * Bind an already-registered table by name, describing it first.
   *
   * @param table - The table's `namespace.name`.
   * @throws {@link WyrdError} when the table is unknown or unauthorized, or
   * for a transport failure.
   */
  async useTableByName(table: string): Promise<void> {
    lifecycleValue<null>(await this.#native.useTableByName(table));
  }

  /** The active write binding, if any. */
  get table(): TableConfig | undefined {
    const native = this.#native.table;
    return native === null || native === undefined ? undefined : TableConfig.fromNative(native);
  }

  /**
   * Enqueue one row into the active table.
   *
   * Synchronous and non-blocking; durable after {@link Bifrost.flush}. A
   * saturated queue throws the stable refusal rather than dropping the row.
   *
   * @param row - One row object matching the active table's columns.
   * @param correlation - Optional `cardUid` and `runId` stamped on the row.
   * @throws {@link WyrdError} with no active table, for a malformed
   * `cardUid`, or when the queue is full.
   */
  insert(row: Readonly<Record<string, unknown>>, correlation: Correlation = {}): void {
    lifecycleValue<null>(this.#native.insert(JSON.stringify(row), correlation.cardUid, correlation.runId));
  }

  /**
   * Write one already-built Arrow batch to `table` and await durability.
   *
   * The precision write door, beside {@link Bifrost.insert}: it names its
   * destination instead of using the active binding, carries correlation as
   * ordinary columns, and is durable when it resolves, so no flush follows it.
   * Build the batch against {@link TableConfig.schema} from
   * `describeTableConfig`. Columns match the table's declared columns by name,
   * in any order; the server judges every column's type and refuses a stale
   * declaration.
   *
   * @param table - The destination's `namespace.name`.
   * @param batch - One Arrow batch matching the table's declared fields.
   * @throws {@link WyrdError} for a batch the table refuses, or for a
   * transport or authorization failure.
   */
  async writeBatch(table: string, batch: RecordBatch): Promise<void> {
    const ipc = tableToIPC(new Table(batch), "stream");
    lifecycleValue<null>(await this.#native.writeBatch(table, Buffer.from(ipc)));
  }

  /**
   * Flush every pooled producer and await each durable acknowledgement.
   *
   * @throws {@link WyrdError} for the first producer or sink failure.
   */
  async flush(): Promise<void> {
    lifecycleValue<null>(await this.#native.flush());
  }

  /**
   * Drain every producer and stop its background task.
   *
   * @throws {@link WyrdError} for the first producer or sink failure.
   */
  async shutdown(): Promise<void> {
    lifecycleValue<null>(await this.#native.shutdown());
  }

  /**
   * Run one SQL SELECT over any authorized table and collect every batch.
   *
   * Drains {@link Bifrost.stream}, so the two cannot disagree about the rows a
   * query returns or about the terminal frame each requires. `params` bind
   * the `$1..$n` placeholders in order and are never interpolated into SQL.
   *
   * @param query - One SELECT statement.
   * @param params - Values bound to `$1..$n`, in order.
   * @returns Every batch, the terminal frame, and the result schema.
   * @throws {@link WyrdError} for an invalid or refused query, a transport
   * failure, or a failed query; {@link IncompleteQueryStreamError} when the
   * stream ends without its terminal.
   */
  async sql(query: string, params?: readonly QueryParam[]): Promise<QueryResult>;
  /**
   * Run one SQL SELECT and return each row parsed by `rows`.
   *
   * A purely local projection over the completed result: the query, its
   * authorization, its limits, and its terminal are the same ones raw
   * {@link Bifrost.sql} runs. The schema never reaches the server and says
   * nothing about the table's stored layout. Every cell reaches `rows` as its
   * native value: a Struct is a plain object, a List an array, a Map an
   * object, a Variant its decoded JSON value, and a 64-bit integer a
   * `bigint`. A top-level timestamp column's cell is its Wyrd timestamp
   * type's text: `TIMESTAMP_NTZ` with no offset, `TIMESTAMP_LTZ` in UTC, and
   * `TIMESTAMP_TZ` in the writer's offset.
   *
   * @param query - One SELECT statement.
   * @param params - Values bound to `$1..$n`, in order.
   * @param rows - Parses one row object, such as a Zod schema.
   * @returns Every row, parsed.
   * @throws whatever `rows.parse` throws for the first row it rejects, so a
   * partially valid result is never returned as success, and the errors of
   * the raw form.
   */
  async sql<T>(query: string, params: readonly QueryParam[], rows: RowSchema<T>): Promise<T[]>;
  async sql<T>(query: string, params?: readonly QueryParam[], rows?: RowSchema<T>): Promise<QueryResult | T[]> {
    const stream = await this.stream(query, params);
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
    const kinds = stream.timestampKinds;
    return result
      .toArrow()
      .toArray()
      .map((row: { toJSON(): Record<string, unknown> }) => {
        const values = row.toJSON();
        for (const field of schema.fields) {
          const kind = kinds[field.name];
          values[field.name] =
            kind === undefined ? nativeValue(field, values[field.name]) : timestampValue(kind, values[field.name]);
        }
        return rows.parse(values);
      });
  }

  /**
   * Run one SQL SELECT and iterate its batches as they arrive.
   *
   * @param query - One SELECT statement.
   * @param params - Values bound to `$1..$n`, in order.
   * @param options - `deadlineMs`, the query deadline from 1 to 4294967295
   * milliseconds; omitted, the server's default deadline applies.
   * @returns The running stream.
   * @throws {@link WyrdError} for an invalid request, a refused or
   * unauthorized query, or a transport failure when the query starts; later
   * failures throw during iteration.
   */
  async stream(
    query: string,
    params?: readonly QueryParam[],
    options: { readonly deadlineMs?: number } = {},
  ): Promise<BifrostQueryStream> {
    const start = await this.#native.query({
      sql: query,
      deadlineMs: options.deadlineMs,
      params: params?.map((value) => {
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
    });
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

  /**
   * Rows dropped by the fire-and-forget observation path; always zero for
   * {@link Bifrost.insert}, which refuses rather than drops.
   */
  get dropped(): number {
    return this.#native.dropped;
  }

  /**
   * List active queries visible to the authenticated tenant.
   *
   * @returns Every active query.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async running(): Promise<RunningQuery[]> {
    return lifecycleValue<RunningQuerySummary[]>(await this.#native.running()).map(runningQuery);
  }

  /**
   * Return one active query by its canonical request ID.
   *
   * @param requestId - The query's {@link BifrostQueryStream.requestId}.
   * @returns The query's lifecycle state and progress.
   * @throws {@link WyrdError} for a malformed ID, an unknown query, or a
   * transport or authorization failure.
   */
  async status(requestId: string): Promise<RunningQuery> {
    return runningQuery(lifecycleValue<RunningQuerySummary>(await this.#native.status(requestId)));
  }

  /**
   * Request server-side cancellation without closing a local stream.
   *
   * @param requestId - The query's {@link BifrostQueryStream.requestId}.
   * @returns Whether cancellation started.
   * @throws {@link WyrdError} for a malformed ID, an unknown query, or a
   * transport or authorization failure.
   */
  async cancel(requestId: string): Promise<CancelRunningQueryResult> {
    const wire = lifecycleValue<CancelRunningQueryResponse>(await this.#native.cancel(requestId));
    return {
      requestId: wire.request_id,
      cancellationStarted: wire.cancellation_started,
    };
  }

  /**
   * Read one trace's span tree plus each trace linking directly to it.
   *
   * Spans come back deduplicated, ordered by start time, then trace id, then
   * span id. A trace no span names reads as an empty array.
   *
   * @param traceId - The 32-character lowercase hex W3C trace id.
   * @returns The canonical span records.
   * @throws {@link WyrdError} `WYRD_SPEC_400_VALIDATION` for a malformed or
   * all-zero id, before any request; otherwise a query, authorization, or
   * transport failure.
   */
  async traceTimeline(traceId: string): Promise<SpanRecord[]> {
    return lifecycleValue<SpanRecord[]>(await this.#native.traceTimeline(traceId));
  }

  /**
   * List the compact entries of every table the caller may read.
   *
   * Visible built-in tables and the tenant's own, without schemas; call
   * {@link Bifrost.describeTable} for one table's schema when it is needed.
   *
   * @returns The server's authorized table entries.
   * @throws {@link WyrdError} when the principal lacks `bifrost_table:read`,
   * or for a transport failure.
   */
  async listTables(): Promise<TableEntry[]> {
    return lifecycleValue<TableEntry[]>(await this.#native.listTables());
  }

  /**
   * Describe one registered table's stored physical schema.
   *
   * @param namespace - The table's namespace, such as `vala.datasets`.
   * @param name - The table's name within `namespace`.
   * @returns The server's projection of the stored schema.
   * @throws {@link WyrdError} when the table is unknown or unauthorized, or
   * for a transport failure.
   */
  async describeTable(namespace: string, name: string): Promise<TableDescription> {
    const wire = lifecycleValue<TableDescriptionWire>(await this.#native.describeTable(namespace, name));
    // The native client already refused any non-canonical or out-of-range spelling.
    return { ...wire, table_version: BigInt(wire.table_version) };
  }
}

/** Machine-readable result of a published hydration. */
export interface PullSummary {
  readonly root: CardRef;
  readonly destination: string;
  readonly mode: "complete" | "metadata";
  readonly card_count: number;
  readonly artifact_count: number;
  readonly downloaded_artifact_count: number;
}

/** Card references cross the native boundary as text or serialized JSON. */
function cardRefText(ref: CardRef | string): string {
  return typeof ref === "string" ? ref : JSON.stringify(ref);
}

/**
 * Record one telemetry observation, fire-and-forget.
 *
 * Telemetry names its own `table` and `schema` per call rather than using the
 * client's active binding, so an instrumented process writes its signals
 * alongside whatever the application writes. If the producer refuses the row,
 * for example because its queue is full, the row is dropped and counted on
 * {@link Bifrost.dropped} instead of throwing.
 *
 * @param bifrost - The client whose producers carry the row.
 * @param table - The destination table's `namespace.name`.
 * @param schema - The table's JSON Schema, or a schema that emits one, mapped
 * to the Arrow schema of the row.
 * @param row - One row object matching `schema`.
 * @param correlation - Optional `cardUid` and `runId` stamped on the row;
 * omitted, the row is uncorrelated.
 * @throws {@link WyrdError} `WYRD_VALA_400_SCHEMA_PARSE` for a schema that
 * does not map to Arrow, or `WYRD_SPEC_400_VALIDATION` for unparsable schema
 * text or a malformed `cardUid`.
 */
export function record(
  bifrost: Bifrost,
  table: string,
  schema: Readonly<Record<string, unknown>> | JsonSchemaSource,
  row: Readonly<Record<string, unknown>>,
  correlation: Correlation = {},
): void {
  lifecycleValue<null>(
    bifrostNative(bifrost).record(
      table,
      jsonSchemaText(schema),
      JSON.stringify(row),
      correlation.cardUid,
      correlation.runId,
    ),
  );
}

/** Reads a {@link WyrdClient}'s native handle for the surfaces that call the server. */
let wyrdClientNative: (client: WyrdClient) => NativeWyrdClient;

/**
 * The one identity option every server-facing surface takes.
 *
 * The server URL, credential, and gRPC URL are {@link WyrdClient.connect}
 * options only; every other surface takes the client built from them.
 */
export interface ClientOptions {
  /**
   * Client to call the server as; omitted, the ambient client
   * ({@link WyrdClient.connect} with no options) is resolved.
   */
  readonly client?: WyrdClient;
}

/** The caller's client, or the ambient one. */
function clientOf(options: ClientOptions): WyrdClient {
  return options.client ?? WyrdClient.connect();
}

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

  /** @internal Wrap a native client another handle resolved, such as a state's. */
  static fromNative(native: NativeWyrdClient): WyrdClient {
    return new WyrdClient(native);
  }

  /**
   * Build a client without performing IO.
   *
   * Omitted options resolve from the environment, then the saved
   * `wyrd auth login` for this server (the one for `WYRD_TENANT` when set,
   * otherwise the newest), then
   * `~/.config/wyrd/credentials.toml`.
   *
   * @param options - `serverUrl`, the Wyrd server's HTTP base URL (else
   * `WYRD_SERVER_URL`). `credential`, an API key (`wyrd_sk_…`), which is
   * exchanged for an access token, or an access token, which is presented
   * as-is (else `WYRD_ACCESS_TOKEN`, then `WYRD_WORKLOAD_TOKEN` with its
   * tenant, then `WYRD_API_KEY`, then a saved login). `grpcUrl`, the Bifrost
   * ingest endpoint (else `WYRD_GRPC_URL`, else the server's host on port
   * `50051`). `workloadUid`, the exact Service Card UID the client
   * runs as, as {@link WyrdClient.forWorkload} selects it (else
   * `WYRD_WORKLOAD_UID`, else rootless work).
   * @returns The client.
   * @throws {@link WyrdError}: `WYRD_CLIENT_401_NO_CREDENTIALS` when no
   * credential resolves, `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` when a
   * `WYRD_TENANT` matches none of this server's saved logins or a saved login
   * cannot be used, and `WYRD_SPEC_400_VALIDATION` when `workloadUid` is not
   * a UUIDv7.
   */
  static connect(
    options: {
      readonly serverUrl?: string;
      readonly credential?: string;
      readonly grpcUrl?: string;
      readonly workloadUid?: string;
    } = {},
  ): WyrdClient {
    const result = connectWyrdClient(options.serverUrl, options.credential, options.grpcUrl, options.workloadUid);
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

  /** The exact Service Card UID this client runs as, or `undefined` for rootless work. */
  get workloadUid(): string | undefined {
    return this.#native.workloadUid ?? undefined;
  }

  /** The deployment environment correlation tag requests carry, or `undefined`. */
  get deploymentEnvironment(): string | undefined {
    return this.#native.deploymentEnvironment ?? undefined;
  }

  /**
   * Return a client that runs as the exact Service root `workloadUid`.
   *
   * The returned client shares this client's credential and connection pools
   * but keeps its own token cache: an API key or workload token requests the
   * root in its exchange and every renewal, and every request carries it as
   * `x-wyrd-workload-uid`. No network call is made; the server validates the
   * root on first use.
   *
   * @param workloadUid - The exact Service Card UID to run as.
   * @returns The rooted client.
   * @throws {@link WyrdError}: `WYRD_SPEC_400_VALIDATION` when `workloadUid`
   * is not a UUIDv7.
   */
  forWorkload(workloadUid: string): WyrdClient {
    const result = this.#native.forWorkload(workloadUid);
    return new WyrdClient(nativeHandle(result.client, result.error));
  }

  /**
   * Return a current bearer for this client's credential.
   *
   * Hand it to a third-party client, such as an OpenAI SDK pointed at the
   * Gateway. The token is renewed in Rust when it nears expiry, so call this
   * again rather than holding the value.
   *
   * @returns The current bearer.
   * @throws {@link WyrdError} when the key exchange is refused or the server
   * is unreachable.
   */
  async accessToken(): Promise<string> {
    const result = await this.#native.accessToken();
    return nativeHandle(result.token, result.error);
  }

  /**
   * A `fetch` that authenticates every request to Wyrd as this client.
   *
   * Hand it to any client that takes a `fetch`: a provider SDK pointed at the
   * gateway, `new OpenAI({ baseURL, apiKey: "wyrd", fetch: wyrd.httpAuth })`,
   * or an MCP transport against `/mcp`. Each request drops `authorization`,
   * `x-api-key`, and `x-goog-api-key`, which on a client wired to Wyrd hold
   * only a placeholder provider key, and carries this client's current access
   * token as `X-Wyrd-Access-Token: Bearer <token>`, plus `x-wyrd-workload-uid`
   * when the client runs as a workload root. The token is read per request,
   * so a long-lived client keeps working after any one token expires.
   *
   * @returns The authenticating `fetch`; a request rejects with the client's
   * {@link WyrdError} when the credential is missing or refused.
   */
  get httpAuth(): typeof fetch {
    return async (input, init) => {
      const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
      for (const name of ["authorization", "x-api-key", "x-goog-api-key"]) headers.delete(name);
      headers.set("x-wyrd-access-token", `Bearer ${await this.accessToken()}`);
      const workloadUid = this.workloadUid;
      if (workloadUid !== undefined) headers.set("x-wyrd-workload-uid", workloadUid);
      return fetch(input, { ...init, headers });
    };
  }

  /**
   * Return a client that acts for the holder of `subjectToken` (RFC 8693).
   *
   * This client's credential is the actor. The issued token's subject is the
   * inbound principal, its actor is this client, and its permissions are the
   * intersection of both. The first exchange runs here; the returned client
   * re-exchanges before expiry.
   *
   * @param subjectToken - The inbound principal's bearer.
   * @param options - `audience`, the service the token is bound to;
   * `bifrost` by default.
   * @returns The delegated client.
   * @throws {@link WyrdError}: `WYRD_SPEC_400_VALIDATION` for an unknown
   * audience, else the server's exchange refusal or a transport failure.
   */
  async onBehalfOf(subjectToken: string, options: { readonly audience?: TokenAudience } = {}): Promise<WyrdClient> {
    const result = await this.#native.onBehalfOf(subjectToken, options.audience ?? "bifrost");
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
   * Connect a registry client, completing any credential exchange the client
   * requires before resolving. A presented-as-is bearer or fresh cached token
   * is reused without a request, so a later call may still be refused.
   *
   * @param options - `client` to call the registry as; omitted, the ambient
   * client is resolved.
   * @returns The registry client.
   * @throws {@link WyrdError} when the ambient client cannot be resolved or a
   * required credential exchange is refused or unreachable.
   */
  static async connect(options: ClientOptions = {}): Promise<Cards> {
    const connection = await wyrdClientNative(clientOf(options)).cards();
    return new Cards(nativeHandle(connection.handle, connection.error));
  }

  /**
   * Load a Card file or a bundle directory and declare it as one composite.
   *
   * @param path - A Card file, or a directory of Card files.
   * @returns The graph root and each Card's dependency-first outcome.
   * @throws {@link WyrdError} for a file that does not load or validate, or
   * the registry's refusal.
   */
  async declareFromPath(path: string): Promise<DeclarationReceipt> {
    return lifecycleValue<DeclarationReceipt>(await this.#native.declareFromPath(path));
  }

  /**
   * Fetch one declared Card envelope by exact reference, without artifacts.
   *
   * The envelope is discriminated by `kind`: `spec` and `status` are typed for
   * Data, Model, Prompt, Agent, Verifier, Service, Trigger, and Operator Cards,
   * including a Drift Verifier's `status.verification.baseline` state. Other
   * kinds return {@link RegisteredUntypedCard}, whose `spec` is a plain record.
   *
   * @param ref - A Card reference, or its `Kind/name@version[#uid]` text.
   * @returns The declared envelope.
   * @throws {@link WyrdError} for a malformed reference, an unknown Card, or
   * a transport or authorization failure.
   */
  async get(ref: CardRef | string): Promise<RegisteredCard> {
    return lifecycleValue<RegisteredCard>(await this.#native.get(cardRefText(ref)));
  }

  /**
   * List metadata-only Card summaries.
   *
   * @param request - Optional filters, page size, and cursor.
   * @returns One page of summaries and the next page's cursor.
   * @throws {@link WyrdError} for a malformed filter or a transport or
   * authorization failure.
   */
  async list(request: ListCardsRequest = {}): Promise<ListCardsResponse> {
    return lifecycleValue<ListCardsResponse>(await this.#native.list(JSON.stringify(request)));
  }

  /**
   * Resolve the latest Active version of one named Card to its exact reference.
   *
   * @param kind - The Card kind, such as `Model`.
   * @param name - The Card name.
   * @returns The exact reference, carrying its `uid`.
   * @throws {@link WyrdError}: `WYRD_SPEC_400_VALIDATION` for a malformed
   * kind or name, else the registry's not-found or a transport or
   * authorization failure.
   */
  async resolveLatest(kind: string, name: string): Promise<CardRef> {
    return lifecycleValue<CardRef>(await this.#native.resolveLatest(kind, name));
  }

  /**
   * Pull a declared Card's reachable graph into a local bundle for
   * {@link WyrdState}.
   *
   * Publication is atomic: a failed pull leaves any prior bundle intact.
   *
   * @param ref - The root Card's reference, or its text form.
   * @param destination - The bundle directory to publish.
   * @param options - `metadataOnly` skips artifact payload downloads.
   * @returns What was published.
   * @throws {@link WyrdError} for a malformed reference, an unresolvable
   * graph, an invalid destination, or an artifact that cannot be downloaded
   * or verified.
   */
  async pull(
    ref: CardRef | string,
    destination: string,
    options: { readonly metadataOnly?: boolean } = {},
  ): Promise<PullSummary> {
    return lifecycleValue<PullSummary>(
      await this.#native.pull(cardRefText(ref), destination, options.metadataOnly ?? false),
    );
  }

  /**
   * Soft-delete one Card by exact reference.
   *
   * @param ref - A Card reference, or its text form.
   * @throws {@link WyrdError} for a malformed reference, an unknown Card, or
   * a transport or authorization failure.
   */
  async delete(ref: CardRef | string): Promise<void> {
    lifecycleValue<null>(await this.#native.delete(cardRefText(ref)));
  }
}

/** The provider an Operator connection authenticates to. */
export type OperatorProvider = OperatorConnectionView["provider"];

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
   * Connect an Operator connection client, completing any credential exchange the client
   * requires before resolving. A presented-as-is bearer or fresh cached token
   * is reused without a request, so a later call may still be refused.
   *
   * @param options - `client` to call the server as; omitted, the ambient
   * client is resolved.
   * @returns The connection client.
   * @throws {@link WyrdError} when the ambient client cannot be resolved or a
   * required credential exchange is refused or unreachable.
   */
  static async connect(options: ClientOptions = {}): Promise<OperatorConnections> {
    const connection = await wyrdClientNative(clientOf(options)).operatorConnections();
    return new OperatorConnections(nativeHandle(connection.handle, connection.error));
  }

  /**
   * Create one connection; requires `operators:write`.
   *
   * @param request - The provider config and its write-only secret.
   * @returns The connection's redacted view.
   * @throws {@link WyrdError} for a malformed request, a duplicate name, or
   * a transport or authorization failure.
   */
  async create(request: CreateOperatorConnectionRequest): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(await this.#native.create(JSON.stringify(request)));
  }

  /**
   * List the caller tenant's connections; requires `operators:read`.
   *
   * @returns Every connection's redacted view.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async list(): Promise<readonly OperatorConnectionView[]> {
    return lifecycleValue<readonly OperatorConnectionView[]>(await this.#native.list());
  }

  /**
   * Read one connection; requires `operators:read`.
   *
   * @param connectionId - The connection's UUID.
   * @returns The connection's redacted view.
   * @throws {@link WyrdError} for a malformed ID, an unknown connection, or a
   * transport or authorization failure.
   */
  async get(connectionId: string): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(await this.#native.get(connectionId));
  }

  /**
   * Update config, re-enable, or rotate the secret; requires `operators:write`.
   *
   * @param connectionId - The connection's UUID.
   * @param request - The fields to replace; omitted fields are preserved.
   * @returns The connection's redacted view.
   * @throws {@link WyrdError} for a malformed argument, an unknown
   * connection, a provider mismatch, or a transport or authorization failure.
   */
  async update(connectionId: string, request: UpdateOperatorConnectionRequest): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(await this.#native.update(connectionId, JSON.stringify(request)));
  }

  /**
   * Disable one connection; requires `operators:write`.
   *
   * @param connectionId - The connection's UUID.
   * @returns The connection's redacted view.
   * @throws {@link WyrdError} for a malformed ID, an unknown connection, or a
   * transport or authorization failure.
   */
  async disable(connectionId: string): Promise<OperatorConnectionView> {
    return lifecycleValue<OperatorConnectionView>(await this.#native.disable(connectionId));
  }
}

/**
 * Tenant principal client over the shared Rust handle: Service principals,
 * their credentials, discovery, and direct Role assignment.
 *
 * A Role change reaches the principal at its next token; tokens already
 * issued keep their Roles. Failures throw a structured {@link WyrdError}.
 */
export class Principals {
  readonly #native: NativePrincipals;

  private constructor(native: NativePrincipals) {
    this.#native = native;
  }

  /**
   * Connect a principal client, completing any credential exchange the client
   * requires before resolving. A presented-as-is bearer or fresh cached token
   * is reused without a request, so a later call may still be refused.
   *
   * @param options - `client` to call the server as; omitted, the ambient
   * client is resolved.
   * @returns The principal client.
   * @throws {@link WyrdError} when the ambient client cannot be resolved or a
   * required credential exchange is refused or unreachable.
   */
  static async connect(options: ClientOptions = {}): Promise<Principals> {
    const connection = await wyrdClientNative(clientOf(options)).principals();
    return new Principals(nativeHandle(connection.handle, connection.error));
  }

  /**
   * Create a Service account; requires `service_accounts:write`, and a
   * family `workload_restriction` also requires tenant administration.
   *
   * @param request - The account's name, Roles, optional description, and
   * optional workload restriction (omitted means unrestricted).
   * @returns The principal id and its first credential.
   * @throws {@link WyrdError} for a malformed request or a transport or authorization failure.
   */
  async createServicePrincipal(request: CreateServicePrincipalRequest): Promise<CreateServicePrincipalResponse> {
    return lifecycleValue(await this.#native.createServicePrincipal(JSON.stringify(request)));
  }

  /**
   * Issue one more credential; requires `service_accounts:write`.
   *
   * @param principalId - The Service account.
   * @returns The credential and its plaintext secret.
   * @throws {@link WyrdError} for an unknown principal or a transport or authorization failure.
   */
  async issueCredential(principalId: string): Promise<IssuedCredential> {
    return lifecycleValue(await this.#native.issueCredential(principalId));
  }

  /**
   * List a principal's credential metadata; requires `service_accounts:write`.
   *
   * @param principalId - The principal whose credentials to list.
   * @returns The credentials, without secrets.
   * @throws {@link WyrdError} for an unknown principal or a transport or authorization failure.
   */
  async listCredentials(principalId: string): Promise<CredentialList> {
    return lifecycleValue(await this.#native.listCredentials(principalId));
  }

  /**
   * Revoke one credential of a principal; requires `service_accounts:write`.
   *
   * @param principalId - The credential's principal.
   * @param credentialId - The credential to revoke.
   * @throws {@link WyrdError} when the credential is not the principal's, or a transport or authorization failure.
   */
  async revokeCredential(principalId: string, credentialId: string): Promise<void> {
    lifecycleValue<null>(await this.#native.revokeCredential(principalId, credentialId));
  }

  /**
   * Revoke a principal and every credential it holds.
   *
   * @param principalId - The principal to revoke.
   * @param request - Its kind and the audited reason.
   * @throws {@link WyrdError} for an unknown principal or a transport or authorization failure.
   */
  async revokePrincipal(principalId: string, request: RevokePrincipalRequest): Promise<void> {
    lifecycleValue<null>(await this.#native.revokePrincipal(principalId, JSON.stringify(request)));
  }

  /**
   * Replace a Service principal's workload restriction. A family restriction
   * makes the account that family's one runtime account and requires tenant
   * administration (`*`).
   *
   * @param principalId - The Service principal to restrict.
   * @param restriction - `{ mode: "unrestricted" }` or a family restriction.
   * @returns The principal with its new restriction.
   * @throws {@link WyrdError} `WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT` when the family already has a runtime account, or for an unknown principal or a transport or authorization failure.
   */
  async setWorkloadRestriction(principalId: string, restriction: WorkloadRestriction): Promise<PrincipalSummary> {
    return lifecycleValue(await this.#native.setWorkloadRestriction(principalId, JSON.stringify(restriction)));
  }

  /**
   * One page of assignable principals ordered by id; requires `service_accounts:write`.
   *
   * @param query - Exact filters, page size, and the previous page's `next`.
   * @returns The page.
   * @throws {@link WyrdError} for an invalid filter or a transport or authorization failure.
   */
  async list(query: PrincipalQuery = {}): Promise<PrincipalPage> {
    return lifecycleValue(await this.#native.list(JSON.stringify(query)));
  }

  /**
   * A principal's Role assignments; requires `service_accounts:write`.
   *
   * @param principalId - The principal to read.
   * @returns Its assignments ordered by Role and source.
   * @throws {@link WyrdError} for a principal that is not assignable, or a transport or authorization failure.
   */
  async roles(principalId: string): Promise<PrincipalRoles> {
    return lifecycleValue(await this.#native.roles(principalId));
  }

  /**
   * Idempotently grant a direct Role; requires tenant administration.
   *
   * @param principalId - The principal to grant to.
   * @param role - The Role name.
   * @returns Whether the call changed anything and the resulting assignments.
   * @throws {@link WyrdError} for an unknown Role, a principal that is not
   * assignable, or a transport or authorization failure.
   */
  async grantRole(principalId: string, role: string): Promise<RoleAssignmentChange> {
    return lifecycleValue(await this.#native.grantRole(principalId, role));
  }

  /**
   * Idempotently revoke a direct Role; identity-provider assignments stay.
   *
   * @param principalId - The principal to revoke from.
   * @param role - The Role name.
   * @returns Whether the call changed anything and the resulting assignments.
   * @throws {@link WyrdError} for an unknown Role, a principal that is not
   * assignable, or a transport or authorization failure.
   */
  async revokeRole(principalId: string, role: string): Promise<RoleAssignmentChange> {
    return lifecycleValue(await this.#native.revokeRole(principalId, role));
  }
}

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
 * Node's active OpenTelemetry span as W3C `traceparent`/`tracestate`; `null`
 * when no API is installed, no span is active, or its context is invalid.
 * Every native call reads it on entry, so the Wyrd requests it sends join the
 * caller's trace. Given `producerTable`, a queued write's `bifrost.enqueue`
 * PRODUCER span opens and ends under the active span first, and its context
 * is returned instead.
 */
function activeTraceContext(producerTable?: string | null): { traceparent: string; tracestate?: string } | null {
  const api = otel();
  if (api === null) {
    return null;
  }
  let span = api.trace.getActiveSpan();
  if (typeof producerTable === "string") {
    span = api.trace.getTracer("wyrd.bifrost").startSpan("bifrost.enqueue", {
      kind: api.SpanKind.PRODUCER,
      attributes: {
        "messaging.system": "bifrost",
        "messaging.operation.type": "create",
        "messaging.destination.name": producerTable,
      },
    });
    span.end();
  }
  const spanContext = span?.spanContext();
  if (spanContext === undefined || !api.isSpanContextValid(spanContext)) {
    return null;
  }
  const flags = spanContext.traceFlags.toString(16).padStart(2, "0");
  return {
    traceparent: `00-${spanContext.traceId}-${spanContext.spanId}-${flags}`,
    tracestate: spanContext.traceState?.serialize(),
  };
}

nativeBinding.setTraceSource(activeTraceContext);

/** One entered {@link Run.scope}: the state it was opened on and its subject and Run. */
interface ScopedRun {
  readonly state: WyrdState | undefined;
  readonly cardUid: string;
  readonly runId: string;
}

/** Every entered {@link Run.scope}, innermost last, across awaits. */
const STATE_RUNS = new AsyncLocalStorage<readonly ScopedRun[]>();

/** Identity headers a state-bound `httpAuth` owns; an explicit differing value conflicts. */
const CONTEXT_HEADERS = ["x-wyrd-workload-uid", "wyrd-card-uid", "wyrd-run-id"] as const;

/** The application's `@opentelemetry/api`, or `null` when it is not installed. */
function otel(): typeof import("@opentelemetry/api") | null {
  if (otelApi === undefined) {
    try {
      otelApi = require("@opentelemetry/api") as typeof import("@opentelemetry/api");
    } catch {
      otelApi = null;
    }
  }
  return otelApi;
}

/**
 * One slot value: JSON for a `json` slot, a trace-ID string for a `trace`
 * slot, a feature object (or one metric value) for a `samples` slot, or a
 * {@link verify.File} or {@link verify.ArtifactRef} for a `media` slot.
 */
export type VerifierInput = JsonValue | verify.File | verify.ArtifactRef;

/** Slot values keyed by the Verifier's input slot names. */
export type VerifierInputs = Readonly<Record<string, VerifierInput>>;

/** How `verify` and `submit` read their `inputs`. */
export interface VerifierInputOptions {
  /**
   * Pass `inputs` whole as the one value of a single-slot Verifier, even when
   * it is a JSON object that would otherwise read as slot names.
   */
  readonly positional?: boolean;
  /**
   * The caller's group, such as a conversation or order ID, that grouped
   * Verifiers judge together. Never an input slot; omitted, the execution is
   * ungrouped.
   */
  readonly group_id?: string;
}

/** What `submit` did: Window samples were recorded, or Run evidence was queued. */
export type Submission =
  | { readonly kind: "recorded" }
  | { readonly kind: "queued"; readonly execution: verify.Execution };

/**
 * Project slot values onto the native boundary, refusing values JSON cannot
 * carry exactly. A plain object is named slot values unless `positional` is
 * set; any other value is the one positional value, sent without a slot.
 */
function nativeInputs(inputs: VerifierInput | VerifierInputs, options?: VerifierInputOptions): NativeVerifierInput[] {
  const named =
    !options?.positional &&
    typeof inputs === "object" &&
    inputs !== null &&
    !Array.isArray(inputs) &&
    !(inputs instanceof verify.File) &&
    !(inputs instanceof verify.ArtifactRef);
  if (!named) {
    return [nativeInput(undefined, inputs as VerifierInput)];
  }
  return Object.entries(inputs as VerifierInputs).map(([slot, value]) => nativeInput(slot, value));
}

/** Project one value, filling `slot` or, without one, the only slot. */
function nativeInput(slot: string | undefined, value: VerifierInput): NativeVerifierInput {
  if (value instanceof verify.File) {
    const bytes = value.bytes && Buffer.from(value.bytes.buffer, value.bytes.byteOffset, value.bytes.byteLength);
    return { slot, file: { path: value.path, bytes, ...value.options } };
  }
  if (value instanceof verify.ArtifactRef) {
    return { slot, artifact: cardRefText(value.artifact), artifactPath: value.path };
  }
  return { slot, valueJson: strictJson(slot === undefined ? "inputs" : `inputs.${slot}`, value) };
}

/**
 * Evidence values for `media` slots and the handle of one queued Verifier
 * execution: `verify.File`, `verify.ArtifactRef`, and `verify.Execution`.
 */
export namespace verify {
  /** How a {@link File} declares its format and display name. */
  export interface FileOptions {
    /** Canonical format name, such as `pdf`; omitted, the filename extension or the slot's only format decides. */
    readonly format?: string;
    /** Display name sent with the declaration; it selects no storage path. */
    readonly filename?: string;
  }

  /**
   * A local file for a `media` slot. Constructing one reads nothing: `verify`
   * and `submit` hash it, upload it only when the server lacks its digest, and
   * pin it by SHA-256. It never becomes an Artifact Card.
   */
  export class File {
    private constructor(
      /** The file on disk, when it is one. */
      readonly path: string | undefined,
      /** The in-memory contents, when they are. */
      readonly bytes: Uint8Array | undefined,
      /** Declared format and display name. */
      readonly options: FileOptions,
    ) {}

    /** A file on disk; its name supplies the filename and default format. */
    static path(path: string, options: FileOptions = {}): File {
      return new File(path, undefined, options);
    }

    /** In-memory bytes; declare `format` unless the slot accepts only one. */
    static bytes(bytes: Uint8Array, options: FileOptions = {}): File {
      return new File(undefined, bytes, options);
    }
  }

  /** A registered Artifact's file for a `media` slot, read with its own identity and authorization. */
  export class ArtifactRef {
    constructor(
      /** The exact Artifact Card. */
      readonly artifact: CardRef | string,
      /** The file inside the Artifact, when it holds several. */
      readonly path?: string,
    ) {}
  }

  /**
   * One queued Verifier execution, addressed by its server-issued ID.
   *
   * Any process can rebuild it with {@link Execution.connect}; the
   * queue, retries, and results stay server-owned.
   */
  export class Execution {
    readonly #native: NativeExecution;

    private constructor(native: NativeExecution) {
      this.#native = native;
    }

    /** @internal Wrap one native execution handle. */
    static fromNative(native: NativeExecution): Execution {
      return new Execution(native);
    }

    /**
     * Rebuild the handle of execution `id`, such as in another process.
     *
     * @param id - The execution ID `submit` returned.
     * @param options - `client` to read as; omitted, the ambient client is resolved.
     * @throws {@link WyrdError} for a malformed ID, or when the client cannot be
     * resolved or its credential exchange is refused.
     */
    static async connect(id: string, options: ClientOptions = {}): Promise<Execution> {
      const connection = await wyrdClientNative(clientOf(options)).connectExecution(id);
      return new Execution(nativeHandle(connection.execution, connection.error));
    }

    /** The execution ID. */
    get id(): string {
      return this.#native.id;
    }

    /**
     * Read the execution's status, terminal error, and visible result.
     *
     * @throws {@link WyrdError} not-found for an execution not visible yet or at
     * all, and the authorization or transport failure.
     */
    async status(): Promise<VerificationExecutionState> {
      return lifecycleValue(await this.#native.status());
    }

    /**
     * Poll with capped backoff until a result, the execution's own error, or
     * `timeoutMs`. A result is never invented from a status.
     *
     * @throws {@link WyrdError} the settled execution's error (including an
     * asynchronous idempotency conflict), `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`,
     * or a non-transient read failure.
     */
    async wait(options: { readonly timeoutMs: number }): Promise<VerificationResult> {
      return lifecycleValue(await this.#native.wait(options.timeoutMs));
    }

    /**
     * {@link Execution.status}, with a present result's custom `data` parsed
     * by `parser`.
     *
     * @param parser - Parses each present `data`, such as a Zod schema.
     * @throws {@link WyrdError} the errors of {@link Execution.status}, then
     * `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not parse.
     */
    async statusTyped<T>(parser: VerdictDataParser<T>): Promise<VerificationExecutionState<T>> {
      const { result, ...state } = await this.status();
      return result == null ? state : { ...state, result: typedResult(result, parser) };
    }

    /**
     * {@link Execution.wait}, with the result's custom `data` parsed by
     * `parser`; a mismatch never repeats the execution.
     *
     * @param options - `timeoutMs`, as for {@link Execution.wait}.
     * @param parser - Parses each present `data`, such as a Zod schema.
     * @throws {@link WyrdError} the errors of {@link Execution.wait}, then
     * `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not parse.
     */
    async waitTyped<T>(
      options: { readonly timeoutMs: number },
      parser: VerdictDataParser<T>,
    ): Promise<VerificationResult<T>> {
      return typedResult(await this.wait(options), parser);
    }
  }
}

/**
 * The dataset emit and the Verifier execution available on one scoped {@link Run}.
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
   * Emit one row into a registered `vala.datasets.<name>` table.
   *
   * The first call for a table describes it; later calls reuse the cached
   * schema and producer, so only the first awaits a lookup.
   *
   * @param table - The table's `vala.datasets.<name>`.
   * @param row - One JSON row matching the table's columns.
   * @throws a {@link WyrdError} for a table outside `vala.datasets`, an unknown
   * or unauthorized table, before startup, or after shutdown.
   */
  async record(table: string, row: unknown): Promise<void> {
    lifecycleValue<null>(await this.#native.record(table, strictJson("row", row)));
  }

  /**
   * Execute a bound Verifier now on named slot values and return its result.
   *
   * `verifier` names a Verifier bound to this view's subject. Files are hashed
   * and uploaded during the call. A `failed` verdict resolves normally; the
   * request is never replayed.
   *
   * @param verifier - The bound Verifier's name.
   * @param inputs - Values keyed by input slot name, or the one value of a
   * single-slot Verifier: a scalar, array, `verify.File`, or `verify.ArtifactRef`.
   * @param options - `{ positional: true }` passes a JSON object whole as that
   * one value; `group_id` names the caller group the execution belongs to.
   * @throws a {@link WyrdError}: `WYRD_SDK_404_UNKNOWN_VERIFIER`,
   * `WYRD_VERIFICATION_400_INPUT_INVALID`, `WYRD_VERIFICATION_413_INPUT_TOO_LARGE`,
   * `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` for a scheduled-window Verifier,
   * or the server's refusal.
   */
  async verify(
    verifier: string,
    inputs: VerifierInput | VerifierInputs,
    options?: VerifierInputOptions,
  ): Promise<VerificationResult> {
    return lifecycleValue(await this.#native.verify(verifier, nativeInputs(inputs, options), options?.group_id));
  }

  /**
   * {@link Observe.verify}, with every requirement's custom `data` parsed by
   * `parser` after the ordinary execution; a mismatch never repeats it.
   *
   * @param verifier - The bound Verifier's name.
   * @param inputs - As {@link Observe.verify}.
   * @param parser - Parses each present `data`, such as a Zod schema.
   * @param options - As {@link Observe.verify}.
   * @throws a {@link WyrdError}: the errors of {@link Observe.verify}, then
   * `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not parse.
   */
  async verifyTyped<T>(
    verifier: string,
    inputs: VerifierInput | VerifierInputs,
    parser: VerdictDataParser<T>,
    options?: VerifierInputOptions,
  ): Promise<VerificationResult<T>> {
    return typedResult(await this.verify(verifier, inputs, options), parser);
  }

  /**
   * Submit slot values: Window samples are recorded for the Verifier's
   * schedule, and Run evidence is queued for one execution. Acceptance confirms
   * staging only; read the outcome through the execution's `wait`.
   *
   * @param verifier - The bound Verifier's name.
   * @param inputs - As {@link Observe.verify}.
   * @param options - As {@link Observe.verify}.
   * @throws a {@link WyrdError} for the input refusals of {@link Observe.verify},
   * the Bifrost lifecycle refusals for samples, or the server's refusal.
   */
  async submit(
    verifier: string,
    inputs: VerifierInput | VerifierInputs,
    options?: VerifierInputOptions,
  ): Promise<Submission> {
    const submission = await this.#native.submit(verifier, nativeInputs(inputs, options), options?.group_id);
    if (submission.execution == null && submission.error == null) {
      return { kind: "recorded" };
    }
    const execution = nativeHandle(submission.execution, submission.error);
    return { kind: "queued", execution: verify.Execution.fromNative(execution) };
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
  /** The state this view was opened on, so only its `httpAuth` adopts the view. */
  readonly #state: WyrdState | undefined;

  private constructor(native: NativeRun, state: WyrdState | undefined) {
    this.#native = native;
    this.#state = state;
  }

  /** @internal Unwrap one closed native open result of `state`. */
  static fromOpen(open: NativeRunOpen, state?: WyrdState): Run {
    return new Run(nativeHandle(open.run, open.error), state);
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
   * @param alias - A bundle alias.
   * @returns The sibling view, sharing this run's `runId`.
   * @throws a {@link WyrdError} when the bundle does not register `alias`. No
   * network IO occurs.
   */
  forCard(alias: string): Run {
    return Run.fromOpen(this.#native.forCard(alias), this.#state);
  }

  /**
   * Run `fn` inside this view's scope: spans started in it, across awaits,
   * carry this run and Card once {@link WyrdState.startTelemetry} installed
   * telemetry, and requests through the state's {@link WyrdState.httpAuth}
   * carry this run and Card. Without `@opentelemetry/api` spans are untouched.
   *
   * @param fn - The work to scope.
   * @returns What `fn` returns.
   */
  scope<T>(fn: () => T): T {
    const entry: ScopedRun = { state: this.#state, cardUid: this.#native.subjectUid, runId: this.#native.runId };
    return STATE_RUNS.run([...(STATE_RUNS.getStore() ?? []), entry], () => {
      const api = otel();
      if (api === null) {
        return fn();
      }
      const scoped = api.context
        .active()
        .setValue(api.createContextKey("wyrd.run_scope"), [this.#native.subjectUid, this.#native.runId]);
      return api.context.with(scoped, fn);
    });
  }
}

/**
 * A hydrated Prompt Card: its stored envelope, exact registered identity, and
 * the metadata the shared Rust runtime derives from its spec. The derived
 * fields are read-only views; they are never stored or sent back.
 */
export interface PromptCard extends RegisteredPromptCard {
  /** The exact reference, carrying the registered version and UID. */
  readonly ref: CardRef;
  /** Text placeholder names rendering requires, in first-appearance order. */
  readonly variables: readonly string[];
  /** Media placeholder names (`${media:name}`) left to bind. */
  readonly mediaVariables: readonly string[];
  /** The declared structured-output JSON Schema; `null` for a plain-text Prompt. */
  readonly responseSchema: Readonly<Record<string, JsonValue>> | null;
  /** The structured-output schema name; `null` for a plain-text Prompt. */
  readonly responseSchemaName: string | null;
}

/** The native projection of a hydrated Prompt: its envelope plus derived metadata. */
interface NativeHydratedPrompt {
  card: RegisteredPromptCard;
  variables: string[];
  media_variables: string[];
  response_schema: Readonly<Record<string, JsonValue>> | null;
  response_schema_name: string | null;
}

/** Shape a native hydrated Prompt projection as the public {@link PromptCard}. */
function promptCard(prompt: NativeHydratedPrompt, ref: CardRef): PromptCard {
  return {
    ...prompt.card,
    ref,
    variables: prompt.variables,
    mediaVariables: prompt.media_variables,
    responseSchema: prompt.response_schema,
    responseSchemaName: prompt.response_schema_name,
  };
}

/** A hydrated Agent Card, navigable to the registered Prompt Card it pins. */
export interface AgentCard extends RegisteredAgentCard {
  /** The exact Prompt reference the Agent's spec stores. */
  readonly promptRef: CardRef;
  /** The hydrated Prompt Card this Agent pins; a local read. */
  prompt(): PromptCard;
}

/** A Card {@link WyrdState.get} returns, discriminated by `kind`. */
export type HydratedCard = AgentCard | PromptCard | Exclude<RegisteredCard, RegisteredAgentCard | RegisteredPromptCard>;

/**
 * The fields of an Agents SDK span's data the processor reads. Declared here,
 * not imported, so the package's types never require the optional
 * `@openai/agents-core`; every one of its `SpanData` values is assignable.
 */
interface AgentsSpanData {
  readonly type: string;
  readonly name?: unknown;
  readonly model?: unknown;
  readonly usage?: AgentsUsage;
  readonly _response?: { readonly model?: unknown; readonly usage?: AgentsUsage };
}

/** The token counts an Agents SDK model span may supply. */
interface AgentsUsage {
  readonly input_tokens?: unknown;
  readonly output_tokens?: unknown;
}

/** The fields of an Agents SDK span the processor reads. */
interface AgentsSpan {
  readonly traceId: string;
  readonly spanId: string;
  readonly parentId: string | null;
  readonly spanData: AgentsSpanData;
  readonly error: unknown;
  readonly startedAt: string | null;
  readonly endedAt: string | null;
}

/** The fields of an Agents SDK trace the processor reads. */
interface AgentsTrace {
  readonly traceId: string;
  readonly name: string;
}

/**
 * An OpenAI Agents SDK trace processor that records each Agents SDK trace and
 * span as an OpenTelemetry span on the global tracer provider.
 *
 * Pass it to the Agents SDK yourself: `addTraceProcessor` keeps the SDK's
 * default OpenAI exporter alongside it, `setTraceProcessors` replaces it. Wyrd
 * never registers it. Spans export wherever the application's OpenTelemetry
 * pipeline does.
 *
 * An SDK trace becomes a span started in the active context, so it nests
 * under any enclosing OpenTelemetry span and carries the {@link Run.scope}
 * active when it started; each SDK span becomes a child of its SDK parent,
 * else its trace. Framework timestamps are kept when valid. Agent, tool, and
 * model spans follow the GenAI conventions for operation, agent and tool
 * name, model, and token usage, read when the span ends. Prompts, completions,
 * tool arguments and results, and error messages are never recorded: an SDK
 * error sets ERROR status and `error.type` only.
 *
 * A repeated start is ignored, as are ends with no start and every callback
 * after {@link WyrdAgentsTraceProcessor.shutdown}, which ends unfinished spans
 * with ERROR. Neither `shutdown` nor `forceFlush` touches the provider.
 * Without `@opentelemetry/api` installed it records nothing.
 */
export class WyrdAgentsTraceProcessor {
  /** The OpenTelemetry span for each open SDK trace or span, by SDK id. */
  readonly #open = new Map<string, OtelSpan>();
  /** Set by {@link WyrdAgentsTraceProcessor.shutdown}; later callbacks are ignored. */
  #closed = false;

  /** Start the span for `trace` in the active context. */
  async onTraceStart(trace: AgentsTrace): Promise<void> {
    this.#start(trace.traceId, [], trace.name, null);
  }

  /** End the span for `trace`. */
  async onTraceEnd(trace: AgentsTrace): Promise<void> {
    this.#end(trace.traceId, undefined, null);
  }

  /** Start a child of `span`'s SDK parent, else its trace, else the active context. */
  async onSpanStart(span: AgentsSpan): Promise<void> {
    const parents = span.parentId ? [span.parentId, span.traceId] : [span.traceId];
    this.#start(span.spanId, parents, agentsSpanName(span.spanData), span.startedAt);
  }

  /** Record `span`'s GenAI attributes and error status, then end it. */
  async onSpanEnd(span: AgentsSpan): Promise<void> {
    this.#end(span.spanId, span, span.endedAt);
  }

  /** End every unfinished span with ERROR and ignore later callbacks. */
  async shutdown(): Promise<void> {
    this.#closed = true;
    for (const span of this.#open.values()) {
      span.setStatus({ code: 2 /* SpanStatusCode.ERROR */, message: AGENTS_INTERRUPTED });
      span.end();
    }
    this.#open.clear();
  }

  /** Nothing to flush: the application's tracer provider owns export. */
  async forceFlush(): Promise<void> {}

  /**
   * Start the span for SDK id `id` under the first open SDK id in `parents`,
   * placed in the active context so the Run scope is the one active now.
   */
  #start(id: string, parents: readonly string[], name: string, startedAt: string | null): void {
    const api = otel();
    if (api === null || this.#closed || this.#open.has(id)) {
      return;
    }
    const parent = parents.map((candidate) => this.#open.get(candidate)).find((span) => span !== undefined);
    const active = api.context.active();
    const parentContext = parent === undefined ? active : api.trace.setSpan(active, parent);
    const span = api.trace
      .getTracer("wyrd.agents")
      .startSpan(name, { startTime: agentsTime(startedAt) }, parentContext);
    this.#open.set(id, span);
  }

  /** Record `span`'s final data on, and end, the open span for SDK id `id`. */
  #end(id: string, span: AgentsSpan | undefined, endedAt: string | null): void {
    const open = this.#open.get(id);
    if (open === undefined) {
      return;
    }
    this.#open.delete(id);
    if (span !== undefined) {
      const { name, attributes } = agentsGenAi(span.spanData);
      if (name !== undefined) {
        open.updateName(name);
      }
      open.setAttributes(attributes);
      if (span.error != null) {
        open.setAttribute("error.type", "_OTHER");
        open.setStatus({ code: 2 /* SpanStatusCode.ERROR */ });
      }
    }
    open.end(agentsTime(endedAt));
  }
}

/**
 * Classify a rejected `forceFlush` without reading any message into the
 * result. OpenTelemetry JS rejects with an array of per-processor errors and
 * marks an expired bound only in the message text, so a `TimeoutError` name or
 * a message naming the timeout is a timeout; the cause is the error type name.
 */
function telemetryFlushFailure(rejection: unknown): { timedOut: boolean; cause: string } {
  const errors = Array.isArray(rejection) ? rejection : [rejection];
  // ponytail: OTEL JS has no typed timeout error; match its message until it exports one.
  const timedOut = errors.some(
    (error) => error instanceof Error && (error.name === "TimeoutError" || /timeout/i.test(error.message)),
  );
  const first: unknown = errors[0];
  const cause = first instanceof Error ? first.name : typeof first;
  return { timedOut, cause: timedOut ? `forceFlush timed out (${cause})` : cause };
}

/** The fixed, payload-free status description of a span ended by processor shutdown. */
const AGENTS_INTERRUPTED = "span did not finish before the trace processor shut down";

/** An SDK ISO-8601 timestamp as epoch milliseconds, or `undefined` for callback time. */
function agentsTime(timestamp: string | null | undefined): number | undefined {
  const millis = timestamp == null ? Number.NaN : Date.parse(timestamp);
  return Number.isFinite(millis) ? millis : undefined;
}

/** The GenAI start-time span name for agent and tool spans, else the SDK name or type. */
function agentsSpanName(data: AgentsSpanData): string {
  const name = typeof data.name === "string" && data.name !== "" ? data.name : undefined;
  if (data.type === "agent" && name !== undefined) {
    return `invoke_agent ${name}`;
  }
  if (data.type === "function" && name !== undefined) {
    return `execute_tool ${name}`;
  }
  return name ?? data.type;
}

/**
 * The final span name, if it changes, and the GenAI attributes `data`
 * supplies. Only identity, model, and token counts are read; content never is.
 */
function agentsGenAi(data: AgentsSpanData): { name?: string; attributes: Record<string, string | number> } {
  if (data.type === "agent") {
    return {
      attributes: suppliedAttributes({ "gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": data.name }),
    };
  }
  if (data.type === "function") {
    return {
      attributes: suppliedAttributes({ "gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": data.name }),
    };
  }
  const call = data.type === "generation" ? data : data.type === "response" ? data._response : undefined;
  if (call === undefined) {
    return { attributes: {} };
  }
  const attributes = suppliedAttributes({
    "gen_ai.operation.name": "chat",
    "gen_ai.request.model": call.model,
    "gen_ai.usage.input_tokens": call.usage?.input_tokens,
    "gen_ai.usage.output_tokens": call.usage?.output_tokens,
  });
  return { name: typeof call.model === "string" && call.model !== "" ? `chat ${call.model}` : "chat", attributes };
}

/** The string and number values of `supplied`; any other value is omitted. */
function suppliedAttributes(supplied: Record<string, unknown>): Record<string, string | number> {
  const attributes: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(supplied)) {
    if (typeof value === "string" || typeof value === "number") {
      attributes[key] = value;
    }
  }
  return attributes;
}

/** The explicit fields {@link WyrdState.configure} fixes; omitted ones default. */
export type ConfigureOptions = {
  /** The loaded Service's name; must match it. */
  readonly serviceName?: string;
  /** The loaded Service's exact version; must match it. */
  readonly serviceVersion?: string;
  /** The deployment environment correlation tag copied onto rows; it never selects or refuses execution. */
  readonly deploymentEnvironment?: string;
};

/**
 * Offline view of a hydrated Card bundle over the shared Rust `WyrdState`.
 *
 * Reads never contact Wyrd; an unknown alias or invalid bundle throws a
 * structured {@link WyrdError}. The state's identity is fixed when it is
 * created: Bifrost startup and every verify run as its client.
 */
export class WyrdState {
  readonly #native: NativeWyrdState;
  /** The tracer provider {@link WyrdState.startTelemetry} installed. */
  #telemetry: { forceFlush(): Promise<void> } | undefined;

  /**
   * Flush completed spans from the provider {@link WyrdState.startTelemetry}
   * installed, within its configured `forceFlushTimeoutMillis`. Resolves to
   * the payload-free failure, or `undefined` when there is no managed provider
   * or it flushed; never rejects.
   */
  async #flushTelemetry(): Promise<{ timedOut: boolean; cause: string } | undefined> {
    try {
      await this.#telemetry?.forceFlush();
      return undefined;
    } catch (error) {
      return telemetryFlushFailure(error);
    }
  }

  /**
   * Throw the managed telemetry failure, carrying the native drain's code,
   * else the native drain's own error.
   */
  static #settle(telemetry: { timedOut: boolean; cause: string } | undefined, native: NativeLifecycleResult): void {
    if (telemetry !== undefined) {
      throw projectedError(telemetryFlushError(telemetry.timedOut, telemetry.cause, native.errorCode ?? null));
    }
    lifecycleValue<null>(native);
  }

  private constructor(native: NativeWyrdState) {
    this.#native = native;
  }

  /**
   * Load and validate one Card bundle.
   *
   * @param path - The bundle directory {@link Cards.pull} published.
   * @param options - `client`, whose principal every server call of this
   * state runs as; omitted, the ambient client is resolved once, at the first
   * server call, and kept.
   * @returns The loaded state.
   * @throws {@link WyrdError} for an incomplete, malformed, or inconsistent
   * bundle.
   */
  static fromPath(path: string, options: ClientOptions = {}): WyrdState {
    const state = new WyrdState(
      options.client === undefined ? openWyrdState(path) : wyrdClientNative(options.client).openWyrdState(path),
    );
    void state.rootRef;
    return state;
  }

  /**
   * Declare a Card file and return the state of its graph, ready to run.
   *
   * The file and the files its `./path` references name are declared under
   * the registry's usual version rules, so an unchanged rerun declares
   * nothing new; the declared graph is then pulled and loaded as
   * {@link WyrdState.fromPath} loads a bundle. Every call runs as one client.
   * Without `bundleDir` the graph is held in memory and no files are kept.
   *
   * @param path - The root Service Card file.
   * @param options - `client`, whose principal every call runs as (omitted,
   * the ambient client), and `bundleDir`, a directory the complete bundle is
   * published to and kept in, loadable offline with {@link WyrdState.fromPath}.
   * @returns The ready state.
   * @throws {@link WyrdError} `WYRD_LOADER_400_INVALID_ENVELOPE` for an
   * unreadable or invalid Card file, the registry's refusal (such as
   * `WYRD_REGISTRY_409_SPEC_DRIFT` or `WYRD_PERMISSION_403_DENIED_RBAC`), or a
   * {@link WyrdState.fromPath} error. A refusal after the registry accepted
   * the declaration leaves it declared.
   */
  static async declare(
    path: string,
    options: ClientOptions & { readonly bundleDir?: string } = {},
  ): Promise<WyrdState> {
    const declared = await wyrdClientNative(clientOf(options)).declareWyrdState(path, options.bundleDir);
    return new WyrdState(nativeHandle(declared.state, declared.error));
  }

  /** The exact root Card reference. */
  get rootRef(): CardRef {
    return lifecycleValue<CardRef>(this.#native.rootRef());
  }

  /** The root Service Card the bundle was hydrated from, as stored. */
  get service(): RegisteredServiceCard {
    return lifecycleValue<RegisteredServiceCard>(this.#native.service());
  }

  /** Every persisted alias in stable sorted order. */
  get aliases(): readonly string[] {
    return lifecycleValue<string[]>(this.#native.aliases());
  }

  /**
   * The stored Card envelope for an alias.
   *
   * @param alias - A bundle alias.
   * @returns The envelope.
   * @throws {@link WyrdError} `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias.
   */
  card(alias: string): RegisteredCard {
    return lifecycleValue<RegisteredCard>(this.#native.card(alias));
  }

  /**
   * The exact Card reference for an alias.
   *
   * @param alias - A bundle alias.
   * @returns The exact reference.
   * @throws {@link WyrdError} `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias.
   */
  cardRef(alias: string): CardRef {
    return lifecycleValue<CardRef>(this.#native.cardRef(alias));
  }

  /**
   * The Verifier Card an alias names.
   *
   * @param alias - A bundle alias of a Verifier Card.
   * @returns The envelope; its building blocks are `spec.inputs`,
   * `spec.steps`, `spec.requirements`, `spec.decide`, and
   * `spec.context_capture`.
   * @throws {@link WyrdError} `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown
   * alias, or `WYRD_SDK_400_CARD_KIND_MISMATCH` for another kind.
   */
  verifier(alias: string): RegisteredVerifierCard {
    return lifecycleValue<RegisteredVerifierCard>(this.#native.verifier(alias));
  }

  /**
   * The hydrated Card an alias names, discriminated by `kind`.
   *
   * Agents and Prompts are their hydrated {@link AgentCard} and
   * {@link PromptCard}; every other kind is its stored envelope. A local read.
   *
   * @param alias - A bundle alias.
   * @returns The hydrated Card; narrow it by `kind`.
   * @throws {@link WyrdError} `WYRD_SDK_404_UNKNOWN_ALIAS` for an unknown alias.
   */
  get(alias: string): HydratedCard {
    const card = this.card(alias);
    if (card.kind === "Agent") {
      return this.agent(alias);
    }
    if (card.kind === "Prompt") {
      return this.prompt(alias);
    }
    return card;
  }

  /**
   * The hydrated Agent Card an alias names.
   *
   * @param alias - A bundle alias of an Agent Card.
   * @returns The envelope, navigable to its Prompt Card by `prompt()`.
   * @throws {@link WyrdError} as {@link WyrdState.verifier}.
   */
  agent(alias: string): AgentCard {
    const {
      card,
      prompt_ref: promptRef,
      prompt,
    } = lifecycleValue<{
      card: RegisteredAgentCard;
      prompt_ref: CardRef;
      prompt: NativeHydratedPrompt;
    }>(this.#native.agent(alias));
    const hydrated = promptCard(prompt, promptRef);
    return { ...card, promptRef, prompt: () => hydrated };
  }

  /**
   * The hydrated Prompt Card an alias names.
   *
   * @param alias - A bundle alias of a Prompt Card.
   * @returns The envelope with its exact identity at `ref` and its derived metadata.
   * @throws {@link WyrdError} as {@link WyrdState.verifier}.
   */
  prompt(alias: string): PromptCard {
    return promptCard(lifecycleValue<NativeHydratedPrompt>(this.#native.prompt(alias)), this.cardRef(alias));
  }

  /**
   * The Model Card an alias names.
   *
   * @param alias - A bundle alias of a Model Card.
   * @returns The envelope.
   * @throws {@link WyrdError} as {@link WyrdState.verifier}.
   */
  model(alias: string): RegisteredModelCard {
    return lifecycleValue<RegisteredModelCard>(this.#native.model(alias));
  }

  /**
   * The Data Card an alias names.
   *
   * @param alias - A bundle alias of a Data Card.
   * @returns The envelope.
   * @throws {@link WyrdError} as {@link WyrdState.verifier}.
   */
  data(alias: string): RegisteredDataCard {
    return lifecycleValue<RegisteredDataCard>(this.#native.data(alias));
  }

  /**
   * Connect this state's one Bifrost writer and describe the fixed table.
   *
   * Runs as the state's client. Startup describes `vala.drift.observations`
   * before resolving, so submitted samples can never enqueue against a
   * missing, unauthorized, or incompatible system table.
   *
   * @param options - `table` keeps its {@link Bifrost.connect} meaning as the
   * active write binding and does not choose a run's destination.
   * `clientByteLimitBytes` overrides the handle-wide ingestion byte budget
   * (256 MiB by default).
   * @throws {@link WyrdError} for a second start, a closed state, a byte
   * budget too small to seal one message (`WYRD_CLIENT_400_CONFIG_INVALID`), a
   * missing credential, an undialable ingest channel, or a fixed table that is
   * absent, unauthorized, or incompatible.
   */
  async startBifrost(
    options: { readonly table?: TableConfig; readonly clientByteLimitBytes?: number } = {},
  ): Promise<void> {
    lifecycleValue<null>(await this.#native.startBifrost(options.table?.native, options.clientByteLimitBytes));
  }

  /**
   * Install the global OpenTelemetry tracer provider exporting spans to this
   * state's server as the state's client.
   *
   * Spans export through the stock OTLP/HTTP exporter `spanExporter` builds, which
   * asks the client for a fresh access token on every export; spans started
   * inside {@link Run.scope} carry that run and Card. Node's `AsyncLocalStorage`
   * context manager is registered unless the application registered one. A
   * second call is idempotent, and {@link WyrdState.shutdown} flushes only
   * this provider. Needs the optional `@opentelemetry/api`,
   * `@opentelemetry/context-async-hooks`, `@opentelemetry/sdk-trace-base`, and
   * `@opentelemetry/exporter-trace-otlp-proto` peers.
   *
   * @throws a {@link WyrdError}: `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` when
   * a global tracer provider is already registered (add `spanExporter` to it
   * instead), or the credential error when the state's client cannot resolve.
   */
  async startTelemetry(): Promise<void> {
    if (this.#telemetry !== undefined) {
      return;
    }
    const native = this.#native.client();
    const client = WyrdClient.fromNative(nativeHandle(native.client, native.error));
    const { startTelemetry } = await import("./otel.js");
    // An overlapping call on this state may have installed during the await.
    if (this.#telemetry !== undefined) {
      return;
    }
    const provider = startTelemetry(client);
    if (provider === undefined) {
      throw projectedError(telemetryProviderExists());
    }
    this.#telemetry = provider;
  }

  /**
   * Open one invocation over this state, targeting `alias` or the root Service.
   *
   * Local only: no network IO and no server-side Run resource. `alias`
   * selects a registered Card before the run mints its `runId`; later
   * {@link Run.forCard} views share that id.
   *
   * @param alias - A bundle alias; omitted, the root Service.
   * @returns The run's view of that Card.
   * @throws {@link WyrdError} when `alias` is not registered in this bundle.
   */
  run(alias?: string): Run {
    return Run.fromOpen(this.#native.run(alias), this);
  }

  /**
   * Fix this state's Service release and deployment environment before its
   * first use. Local, with no network IO.
   *
   * Omitted `serviceName` and `serviceVersion` default to the loaded Service;
   * an omitted `deploymentEnvironment` defaults to a non-empty
   * `WYRD_DEPLOYMENT_ENVIRONMENT`, else stays `undefined`. Without a call, the
   * first Run, client, Bifrost, telemetry, or shutdown use applies the same
   * defaults. Another loaded state keeps its own configuration.
   *
   * @param options - The explicit name, version, and environment.
   * @throws {@link WyrdError} `WYRD_SDK_409_STATE_CONFIG_FROZEN` once
   * configuration is fixed, even after shutdown, or
   * `WYRD_CLIENT_400_CONFIG_INVALID` for an empty field or a mismatched name
   * or version.
   */
  configure(options: ConfigureOptions = {}): void {
    lifecycleValue<null>(
      this.#native.configure({
        serviceName: options.serviceName,
        serviceVersion: options.serviceVersion,
        deploymentEnvironment: options.deploymentEnvironment,
      }),
    );
  }

  /** The configured deployment environment correlation tag, or `undefined`; reading it fixes defaults. */
  get deploymentEnvironment(): string | undefined {
    return lifecycleValue<string | null>(this.#native.deploymentEnvironment()) ?? undefined;
  }

  /**
   * A `fetch` that authenticates every request to Wyrd as this state's client,
   * in this state's context.
   *
   * Hand it to provider clients created once:
   * `new OpenAI({ baseURL, apiKey: "wyrd", fetch: state.httpAuth })`. Each
   * request carries what {@link WyrdClient.httpAuth} sends, plus the Service as
   * `x-wyrd-workload-uid`, the configured `x-wyrd-deployment-environment`, this
   * state's innermost {@link Run.scope} as `wyrd-card-uid` and `wyrd-run-id`
   * (the Service and no Run outside one), and the active W3C trace context
   * unless the request names its own `traceparent`. The context is read when
   * the request starts, before any token refresh. Reading it fixes defaults
   * and resolves the client without network IO.
   *
   * The deployment environment is a pass-through correlation tag, never an
   * identity: the state's configured tag replaces an explicit one, and an
   * unconfigured state sends the request's own tag, if any, unchanged.
   *
   * @returns The authenticating `fetch`; a request rejects with
   * `WYRD_CLIENT_400_CONFIG_INVALID`, before it is sent, when an explicit
   * identity header differs from that context, else as
   * {@link WyrdClient.httpAuth} does.
   * @throws {@link WyrdError} the credential error when the state's client
   * cannot resolve.
   */
  get httpAuth(): typeof fetch {
    const native = this.#native.client();
    const auth = WyrdClient.fromNative(nativeHandle(native.client, native.error)).httpAuth;
    const service = this.rootRef.uid ?? undefined;
    const environment = this.deploymentEnvironment ?? undefined;
    return async (input, init) => {
      const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
      const run = STATE_RUNS.getStore()?.findLast((entry) => entry.state === this);
      const context: Partial<Record<(typeof CONTEXT_HEADERS)[number], string>> = {
        "x-wyrd-workload-uid": service,
        "wyrd-card-uid": run?.cardUid ?? service,
        "wyrd-run-id": run?.runId,
      };
      for (const name of CONTEXT_HEADERS) {
        const explicit = headers.get(name);
        if (explicit !== null && explicit !== context[name]) {
          throw new WyrdError(
            "WYRD_CLIENT_400_CONFIG_INVALID",
            400,
            "Client configuration is invalid",
            `${name} ${JSON.stringify(explicit)} conflicts with the state-bound context; select the intended state or Run instead`,
            "Correct the client configuration field named in details and rebuild the client.",
            { header: name },
          );
        }
        const value = context[name];
        if (value !== undefined) headers.set(name, value);
      }
      if (environment !== undefined) headers.set("x-wyrd-deployment-environment", environment);
      const trace = headers.has("traceparent") ? null : activeTraceContext();
      if (trace !== null) {
        headers.set("traceparent", trace.traceparent);
        if (trace.tracestate !== undefined) headers.set("tracestate", trace.tracestate);
      }
      return auth(input, { ...init, headers });
    };
  }

  /**
   * Export completed spans from the provider {@link WyrdState.startTelemetry}
   * installed, then drain every producer of this state's writer without
   * closing it.
   *
   * The explicit durability barrier for a test or a finite job. The provider
   * flush leaves active spans active, and the native drain is attempted even
   * when it fails. An application's own provider is never touched.
   *
   * @throws a {@link WyrdError}: `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` or
   * `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` when the managed provider did not
   * flush, with any drain failure's code as `details.native_code`; otherwise
   * before startup, after shutdown, or for the first producer or sink failure.
   */
  async flush(): Promise<void> {
    const telemetry = await this.#flushTelemetry();
    WyrdState.#settle(telemetry, await this.#native.flush());
  }

  /**
   * Drain every producer of this state's writer and close it to writes.
   *
   * Call this once at graceful shutdown, not after each observation: queue
   * admission is not a durable acknowledgement, so an abrupt exit before it
   * resolves can lose pending rows. After an ambiguous failure, retry
   * `shutdown()` on the same state rather than replacing the writer; a
   * successfully closed state stays closed. A tracer provider
   * {@link WyrdState.startTelemetry} installed is flushed first but stays
   * installed and usable; the drain is attempted even when that flush fails.
   *
   * @throws a {@link WyrdError}: `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` or
   * `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` when the managed provider did not
   * flush, with any drain failure's code as `details.native_code`; otherwise
   * the first producer or sink failure.
   */
  async shutdown(): Promise<void> {
    const telemetry = await this.#flushTelemetry();
    WyrdState.#settle(telemetry, await this.#native.shutdown());
  }
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
   * Connect an administration client, completing any credential exchange the client
   * requires before resolving. A presented-as-is bearer or fresh cached token
   * is reused without a request, so a later call may still be refused.
   *
   * @param options - `client` to call the server as; omitted, the ambient
   * client is resolved.
   * @returns The administration client.
   * @throws {@link WyrdError} when the ambient client cannot be resolved or a
   * required credential exchange is refused or unreachable.
   */
  static async connect(options: ClientOptions = {}): Promise<Gateway> {
    const connection = await wyrdClientNative(clientOf(options)).gateway();
    return new Gateway(nativeHandle(connection.handle, connection.error));
  }

  /**
   * Read one redacted provider credential.
   *
   * @param name - The credential's name.
   * @returns The redacted view.
   * @throws {@link WyrdError} for an invalid name, an unknown credential, or a
   * transport or authorization failure.
   */
  async credential(name: string): Promise<ProviderCredentialView> {
    return lifecycleValue(await this.#native.credential(name));
  }

  /**
   * List redacted provider credentials ordered by name.
   *
   * @returns Every redacted view.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async credentials(): Promise<ProviderCredentialView[]> {
    return lifecycleValue(await this.#native.credentials());
  }

  /**
   * Create or replace a provider deployment.
   *
   * @param deployment - The deployment to store under its `name`.
   * @returns The stored deployment.
   * @throws {@link WyrdError} for a malformed body (the body is never quoted),
   * an unknown credential, or a transport or authorization failure.
   */
  async putDeployment(deployment: ProviderDeployment): Promise<ProviderDeployment> {
    return lifecycleValue(await this.#native.putDeployment(JSON.stringify(deployment)));
  }

  /**
   * Read one provider deployment.
   *
   * @param name - The deployment's name.
   * @returns The deployment.
   * @throws {@link WyrdError} for an invalid name, an unknown deployment, or a
   * transport or authorization failure.
   */
  async deployment(name: string): Promise<ProviderDeployment> {
    return lifecycleValue(await this.#native.deployment(name));
  }

  /**
   * List provider deployments ordered by name.
   *
   * @returns Every deployment.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async deployments(): Promise<ProviderDeployment[]> {
    return lifecycleValue(await this.#native.deployments());
  }

  /**
   * Delete a provider deployment; an absent name succeeds.
   *
   * @param name - The deployment's name.
   * @throws {@link WyrdError} for an invalid name or a transport or
   * authorization failure.
   */
  async deleteDeployment(name: string): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteDeployment(name));
  }

  /**
   * Replace the tenant fallback policy.
   *
   * @param policy - The complete new policy.
   * @returns The stored policy.
   * @throws {@link WyrdError} for a malformed policy or a transport or
   * authorization failure.
   */
  async putFallbackPolicy(policy: GatewayFallbackPolicy): Promise<GatewayFallbackPolicy> {
    return lifecycleValue(await this.#native.putFallbackPolicy(JSON.stringify(policy)));
  }

  /**
   * Read the tenant fallback policy, or the default.
   *
   * @returns The stored policy, or the default with no rules.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async fallbackPolicy(): Promise<GatewayFallbackPolicy> {
    return lifecycleValue(await this.#native.fallbackPolicy());
  }

  /**
   * Restore the default fallback policy.
   *
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async deleteFallbackPolicy(): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteFallbackPolicy());
  }

  /**
   * Replace the tenant governance policy.
   *
   * @param policy - The complete new policy.
   * @returns The stored policy.
   * @throws {@link WyrdError} for a malformed policy or a transport or
   * authorization failure.
   */
  async putGovernancePolicy(policy: GatewayGovernancePolicy): Promise<GatewayGovernancePolicy> {
    return lifecycleValue(await this.#native.putGovernancePolicy(JSON.stringify(policy)));
  }

  /**
   * Read the tenant governance policy, or the default.
   *
   * @returns The stored policy, or the default.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async governancePolicy(): Promise<GatewayGovernancePolicy> {
    return lifecycleValue(await this.#native.governancePolicy());
  }

  /**
   * Restore the default governance policy.
   *
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async deleteGovernancePolicy(): Promise<void> {
    lifecycleValue<null>(await this.#native.deleteGovernancePolicy());
  }

  /**
   * Replace the tenant capture policy.
   *
   * @param policy - The capture mode and, in payload mode, its fields.
   * @returns The stored policy with its version.
   * @throws {@link WyrdError} for a malformed policy or a transport or
   * authorization failure.
   */
  async putCapturePolicy(policy: GatewayCapturePolicyWrite): Promise<GatewayCapturePolicy> {
    return lifecycleValue(await this.#native.putCapturePolicy(JSON.stringify(policy)));
  }

  /**
   * Read the tenant capture policy, or the disabled default.
   *
   * @returns The stored policy, or the disabled default.
   * @throws {@link WyrdError} for a transport or authorization failure.
   */
  async capturePolicy(): Promise<GatewayCapturePolicy> {
    return lifecycleValue(await this.#native.capturePolicy());
  }
}
