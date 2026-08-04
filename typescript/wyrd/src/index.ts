import { tableFromIPC, type RecordBatch } from "apache-arrow";
import { createRequire } from "node:module";

import type {
  NativeBifrostQueryStream,
  NativeInsertResult,
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

export interface QueryTerminal {
  outcome: "success" | "degraded" | "failed";
  freshness: string;
  row_count: number;
  warnings: unknown[];
  source_completion: unknown[];
  error?: unknown;
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
