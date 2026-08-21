import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
  BifrostClient,
  BifrostQueryStream,
  IncompleteQueryStreamError,
  WyrdError,
} from "@wyrd/sdk";

const REQUEST_ID = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff";

describe("BifrostQueryStream", () => {
  it("yields Apache Arrow batches and retains the validated terminal", async () => {
    const ipc = tableToIPC(tableFromArrays({ value: [1, 2] }), "stream");
    const terminal = {
      outcome: "success",
      freshness: "fresh",
      row_count: 2,
      warnings: [],
      source_completion: [],
      error: null,
    };
    let index = 0;
    const native = {
      requestId: REQUEST_ID,
      async next() {
        index += 1;
        return index === 1
          ? { ipc: Buffer.from(ipc), terminalJson: undefined }
          : { ipc: undefined, terminalJson: JSON.stringify(terminal) };
      },
      async close() {},
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    expect(stream.requestId).toBe(REQUEST_ID);
    const batches = [];
    for await (const batch of stream) {
      batches.push(batch);
    }
    expect(batches).toHaveLength(1);
    expect(batches[0]?.numRows).toBe(2);
    expect(stream.terminal).toEqual(terminal);
  });

  it("closes the native response when iteration stops early", async () => {
    let closed = false;
    const native = {
      requestId: REQUEST_ID,
      async next() {
        return { ipc: undefined, terminalJson: undefined };
      },
      async close() {
        closed = true;
      },
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    await stream.return();
    expect(closed).toBe(true);
  });

  it("raises a structured incomplete-stream error without parsing messages", async () => {
    let polls = 0;
    const native = {
      requestId: REQUEST_ID,
      async next() {
        polls += 1;
        return {
          ipc: undefined,
          terminalJson: undefined,
          errorCode: "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
          errorStatus: 502,
          errorTitle: "Query stream incomplete",
          errorDetail: "response ended before terminal",
        };
      },
      async close() {},
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    await expect(stream.next()).rejects.toBeInstanceOf(
      IncompleteQueryStreamError,
    );
    await expect(stream.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
    expect(polls).toBe(1);
  });

  it("preserves failed-terminal step details and status", async () => {
    const terminal = {
      outcome: "failed",
      error: { code: "query_execution_failed", detail: "source unavailable" },
    };
    const native = {
      requestId: REQUEST_ID,
      async next() {
        return {
          ipc: undefined,
          terminalJson: undefined,
          errorCode: "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
          errorStatus: 500,
          errorTitle: "Query execution failed",
          errorDetail: "source unavailable",
          errorRemediation: "Inspect the retained terminal error.",
          errorDetailsJson: JSON.stringify(terminal),
        };
      },
      async close() {},
      terminalJson: JSON.stringify(terminal),
    };
    const stream = new BifrostQueryStream(native);
    await expect(stream.next()).rejects.toMatchObject({
      code: "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
      status: 500,
      detail: "source unavailable",
      details: terminal,
    } satisfies Partial<WyrdError>);
  });

  it("closes native ownership before propagating Arrow decode failures", async () => {
    let closed = false;
    const native = {
      requestId: REQUEST_ID,
      async next() {
        return { ipc: Buffer.from("not-arrow"), terminalJson: undefined };
      },
      async close() {
        closed = true;
        throw new Error("cleanup failed");
      },
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    await expect(stream.next()).rejects.toThrow();
    expect(closed).toBe(true);
    await expect(stream.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
  });
});

describe("BifrostClient", () => {
  it("projects request identity and lifecycle controls without conflating stream close", async () => {
    const requestId = REQUEST_ID;
    let cancelCalls = 0;
    let closeCalls = 0;
    const summary = {
      request_id: requestId,
      query_class: "interactive",
      started_at: "2026-08-21T00:00:00Z",
      deadline: "2026-08-21T00:01:00Z",
      state: "running",
      progress: { completed_participants: 0, total_participants: 1 },
      cancellation_requested: false,
    };
    const native = {
      async running() {
        return { valueJson: JSON.stringify([summary]) };
      },
      async status(id: string) {
        expect(id).toBe(requestId);
        return { valueJson: JSON.stringify(summary) };
      },
      async cancel(id: string) {
        cancelCalls += 1;
        expect(id).toBe(requestId);
        return {
          valueJson: JSON.stringify({
            request_id: id,
            cancellation_started: true,
          }),
        };
      },
    };
    const client = new BifrostClient(native as never);
    const projectedSummary = {
      requestId,
      queryClass: "interactive",
      startedAt: "2026-08-21T00:00:00Z",
      deadline: "2026-08-21T00:01:00Z",
      state: "running",
      progress: { completedParticipants: 0, totalParticipants: 1 },
      cancellationRequested: false,
    };
    const stream = new BifrostQueryStream({
      requestId,
      async next() {
        return { ipc: undefined, terminalJson: undefined };
      },
      async close() {
        closeCalls += 1;
      },
      terminalJson: null,
    });
    await stream.return();
    expect(closeCalls).toBe(1);
    expect(cancelCalls).toBe(0);
    expect(await client.running()).toEqual([projectedSummary]);
    expect(await client.status(requestId)).toEqual(projectedSummary);
    expect(await client.cancel(requestId)).toEqual({
      requestId,
      cancellationStarted: true,
    });
    expect(cancelCalls).toBe(1);
    expect(closeCalls).toBe(1);
  });

  it("projects a durable ingest acknowledgement without rewriting the wire", async () => {
    const requests: unknown[] = [];
    const native = {
      async insertBatch(...request: unknown[]) {
        requests.push(request);
        return { batchId: Buffer.from([1, 2, 3]) };
      },
    };
    const client = new BifrostClient(native as never);
    const batchId = new Uint8Array([1, 2, 3]);
    const ipc = new Uint8Array([4, 5]);
    const ack = await client.insertBatch("vala.bifrost.events", batchId, ipc);
    expect(Array.from(ack.batchId)).toEqual([1, 2, 3]);
    expect(requests).toHaveLength(1);
    expect((requests[0] as unknown[])[0]).toBe("vala.bifrost.events");
  });

  it("projects stable ingest errors from the native owner", async () => {
    const native = {
      async insertBatch() {
        return {
          batchId: undefined,
          errorCode: "WYRD_PERMISSION_403_DENIED_RBAC",
          errorStatus: 403,
          errorTitle: "Permission denied (RBAC)",
          errorDetail: "principal lacks bifrost_record:write",
          errorRemediation: "Request the required role.",
          errorDetailsJson: JSON.stringify({ required_scope: "bifrost_record:write" }),
        };
      },
    };
    const client = new BifrostClient(native as never);
    await expect(
      client.insertBatch("vala.bifrost.events", new Uint8Array(16), new Uint8Array()),
    ).rejects.toMatchObject({
      code: "WYRD_PERMISSION_403_DENIED_RBAC",
      status: 403,
      details: { required_scope: "bifrost_record:write" },
    });
  });

  it("serializes published-only and strict defaults while preserving opt-ins", async () => {
    const requests: unknown[] = [];
    const stream = {
      async next() {
        return { ipc: undefined, terminalJson: undefined };
      },
      async close() {},
      terminalJson: null,
    };
    const native = {
      async query(request: unknown) {
        requests.push(request);
        return { takeStream: () => stream };
      },
    };
    const client = new BifrostClient(native as never);
    await client.query({ sql: "SELECT 1" });
    await client.query({
      sql: "SELECT 1",
      visibility: "fused",
      freshness: "allow_degraded",
    });
    expect(requests).toEqual([
      {
        sql: "SELECT 1",
        visibility: "published_only",
        freshness: "strict",
        deadlineMs: undefined,
      },
      {
        sql: "SELECT 1",
        visibility: "fused",
        freshness: "allow_degraded",
        deadlineMs: undefined,
      },
    ]);
  });

  it.each([
    {
      boundary: "Gate",
      code: "WYRD_PERMISSION_403_DENIED_RBAC",
      status: 403,
      title: "Permission denied (RBAC)",
      detail: "principal lacks bifrost_query:read",
      remediation: "Request the required role from a workspace admin.",
      details: { required_scope: "bifrost_query:read" },
    },
    {
      boundary: "authentication",
      code: "WYRD_PERMISSION_401_UNAUTHENTICATED",
      status: 401,
      title: "Authentication required",
      detail: "access token is invalid",
      remediation:
        "Send a valid `Authorization: Bearer <token>` header before invoking permission-protected routes.",
      details: { reason: "invalid_token" },
    },
    {
      boundary: "transport",
      code: "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
      status: 503,
      title: "Service unavailable",
      detail: "query transport is unavailable",
      remediation:
        "Retry with exponential backoff; the server is shedding load to protect inflight requests.",
      details: { retryable: true },
    },
    {
      boundary: "request",
      code: "WYRD_VALA_400_QUERY_INVALID_SQL",
      status: 400,
      title: "Invalid or unsupported query SQL",
      detail: "only SELECT statements are accepted",
      remediation:
        "Submit a single SELECT statement; DDL/DML and unsupported constructs are rejected.",
      details: null,
    },
  ])(
    "preserves structured $boundary startup errors",
    async ({ code, status, title, detail, remediation, details }) => {
      const native = {
        async query() {
          return {
            takeStream() {
              return undefined;
            },
            errorCode: code,
            errorStatus: status,
            errorTitle: title,
            errorDetail: detail,
            errorRemediation: remediation,
            errorDetailsJson: JSON.stringify(details),
          };
        },
      };
      const client = new BifrostClient(native as never);
      let error: unknown;
      try {
        await client.query({ sql: "SELECT 1" });
      } catch (reason) {
        error = reason;
      }

      expect(error).toBeInstanceOf(WyrdError);
      expect(error).toMatchObject({
        code,
        status,
        title,
        detail,
        remediation,
        details,
      });
    },
  );
});
