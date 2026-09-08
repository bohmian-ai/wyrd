import { tableFromIPC, type RecordBatch, type Schema } from "apache-arrow";
import { createRequire } from "node:module";

import type {
  NativeBifrostQueryStream,
  NativeGenAiRequest,
  NativeInsertResult,
  NativeLifecycleResult,
  NativeQueryRequest,
  NativeQueryStep,
} from "../index.cjs";

const require = createRequire(import.meta.url);
const nativeBinding = require("../index.cjs") as typeof import("../index.cjs");
const { NativeBifrostQueryClient } = nativeBinding;
type NativeBifrostQueryClient = import("../index.cjs").NativeBifrostQueryClient;

export type VisibilityMode = "published_only" | "fused";
export type FreshnessPolicy = "strict" | "allow_degraded";

export interface BifrostQueryRequest {
  sql: string;
  visibility?: VisibilityMode;
  freshness?: FreshnessPolicy;
  deadlineMs?: number;
}

export interface BifrostInsertAck {
  readonly batchId: Uint8Array;
}

/**
 * One column declaration in a table description, exactly as stored.
 *
 * `metadata` carries the Arrow field metadata the physical schema holds,
 * including the stable `PARQUET:field_id`.
 */
export interface FieldDescription {
  readonly name: string;
  readonly data_type: unknown;
  readonly nullable: boolean;
  readonly metadata: Readonly<Record<string, string>>;
}

/**
 * The server's projection of one registered table's stored physical schema.
 *
 * A writer declares `user_fields`, supplies `correlation_fields`, and may
 * supply `managed_candidates`. `canonical_physical_fingerprint` is present only
 * for a canonical signal table.
 */
export interface TableDescription {
  readonly entry: Readonly<Record<string, unknown>>;
  readonly user_fields: readonly FieldDescription[];
  readonly correlation_fields: readonly FieldDescription[];
  readonly managed_candidates: readonly FieldDescription[];
  readonly canonical_physical_fingerprint?: string;
  readonly physical_layout: Readonly<Record<string, unknown>>;
}

/** One complete authorized cut of a trace; children nest on their own span. */
export interface TraceDetail {
  readonly trace: {
    readonly trace_id: string;
    readonly spans: readonly Readonly<Record<string, unknown>>[];
  };
}

/** One page of GenAI generation records. */
export interface GenAiPage {
  readonly rows: readonly Readonly<Record<string, unknown>>[];
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

export class BifrostClient {
  readonly #native: NativeBifrostQueryClient;

  constructor(native: NativeBifrostQueryClient) {
    this.#native = native;
  }

  async query(request: BifrostQueryRequest): Promise<BifrostQueryStream> {
    const nativeRequest: NativeQueryRequest = {
      sql: request.sql,
      visibility: request.visibility ?? "published_only",
      freshness: request.freshness ?? "strict",
      deadlineMs: request.deadlineMs,
    };
    const start = await this.#native.query(nativeRequest);
    const error = projectedError(start);
    if (error !== undefined) {
      throw error;
    }
    const stream = start.takeStream();
    if (stream === null || stream === undefined) {
      throw new IncompleteQueryStreamError(
        502,
        "Query stream incomplete",
        "native query startup returned neither a stream nor structured error",
      );
    }
    return new BifrostQueryStream(stream);
  }

  async running(): Promise<RunningQuery[]> {
    return lifecycleValue<RunningQueryWire[]>(await this.#native.running()).map(
      runningQuery,
    );
  }

  async status(requestId: string): Promise<RunningQuery> {
    return runningQuery(
      lifecycleValue<RunningQueryWire>(await this.#native.status(requestId)),
    );
  }

  async cancel(requestId: string): Promise<CancelRunningQueryResult> {
    const wire = lifecycleValue<CancelRunningQueryWire>(
      await this.#native.cancel(requestId),
    );
    return {
      requestId: wire.request_id,
      cancellationStarted: wire.cancellation_started,
    };
  }

  /**
   * Describe one registered table's stored physical schema.
   */
  async describeTable(
    namespace: string,
    name: string,
  ): Promise<TableDescription> {
    return lifecycleValue<TableDescription>(
      await this.#native.describeTable(namespace, name),
    );
  }

  /**
   * Return the exact Arrow schema a writer builds batches on.
   *
   * The schema is decoded from schema-only Arrow IPC produced by Rust from the
   * same description, so JavaScript never reimplements the field/type
   * conversion the physical contract depends on.
   */
  async writableSchema(
    description: TableDescription,
    includeEventTime = false,
  ): Promise<Schema> {
    const ipc = await this.#native.writableSchemaIpc(
      JSON.stringify(description),
      includeEventTime,
    );
    return tableFromIPC(new Uint8Array(ipc)).schema;
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

  /**
   * Read one page of GenAI generation records.
   */
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

  async insertBatch(
    table: string,
    batchId: Uint8Array,
    ipc: Uint8Array,
  ): Promise<BifrostInsertAck> {
    const result: NativeInsertResult = await this.#native.insertBatch(
      table,
      Buffer.from(batchId),
      Buffer.from(ipc),
    );
    const error = projectedError(result);
    if (error !== undefined) {
      throw error;
    }
    if (result.batchId === null || result.batchId === undefined) {
      throw new WyrdError(
        "WYRD_VALA_502_INGEST_ACK_INCOMPLETE",
        502,
        "Bifrost ingest acknowledgement incomplete",
        "ingest returned without a durable batch acknowledgement",
      );
    }
    return { batchId: new Uint8Array(result.batchId) };
  }
}

export class BifrostQueryClient extends BifrostClient {
  constructor(serverUrl: string, token: string, grpcUrl?: string) {
    super(new NativeBifrostQueryClient(serverUrl, token, grpcUrl));
  }
}

export class WyrdClient {
  readonly bifrost: BifrostClient;

  constructor(serverUrl: string, token: string, grpcUrl?: string) {
    this.bifrost = new BifrostClient(
      new NativeBifrostQueryClient(serverUrl, token, grpcUrl),
    );
  }
}
