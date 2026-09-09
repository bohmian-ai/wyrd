import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
  BifrostClient,
  BifrostQueryStream,
  IncompleteQueryStreamError,
  WyrdError,
  type GenAiPage,
  type TableDescription,
  type TraceDetail,
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

describe("BifrostQueryClient typed reads", () => {
  it("typed trace and GenAI client contracts", async () => {
    const described = {
      entry: { namespace: "vala.traces", name: "spans" },
      user_fields: [
        {
          name: "trace_id",
          data_type: "Binary",
          nullable: false,
          metadata: { "PARQUET:field_id": "1" },
        },
      ],
      correlation_fields: [
        {
          name: "card_ref",
          data_type: "Utf8",
          nullable: false,
          metadata: { "wyrd:input_class": "gate_correlation" },
        },
      ],
      managed_candidates: [],
      canonical_physical_fingerprint: "a".repeat(64),
      physical_layout: { partition_granularity: "hour" },
    };
    const schemaOnlyIpc = tableToIPC(
      tableFromArrays({ trace_id: Int32Array.from([]) }),
      "stream",
    );
    const calls: unknown[][] = [];
    const native = {
      async describeTable(namespace: string, name: string) {
        calls.push(["describeTable", namespace, name]);
        return { valueJson: JSON.stringify(described) };
      },
      async writableSchemaIpc(
        descriptionJson: string,
        includeEventTime: boolean,
      ) {
        calls.push(["writableSchemaIpc", descriptionJson, includeEventTime]);
        return Buffer.from(schemaOnlyIpc);
      },
      async getTrace(traceId: string, since?: string, until?: string) {
        calls.push(["getTrace", traceId, since, until]);
        return {
          valueJson: JSON.stringify({
            trace: {
              trace_id: traceId,
              spans: [
                {
                  span_id: "0102030405060708",
                  name: "chat",
                  start_time_unix_nano: 7,
                  events: [{ time_unix_nano: 8, name: "chunk" }],
                  links: [],
                },
              ],
            },
          }),
        };
      },
      async queryGenai(request: unknown) {
        calls.push(["queryGenai", request]);
        return {
          valueJson: JSON.stringify({
            rows: [
              {
                model: "gpt-4o",
                start_time_unix_nano: 11,
                input_messages: [{ role: "user", parts: [1, null] }],
              },
            ],
            next_page_token: "next",
          }),
        };
      },
    };
    const client = new BifrostClient(native as never);

    const description = await client.describeTable("vala.traces", "spans");
    expect(description.canonical_physical_fingerprint).toBe("a".repeat(64));
    expect(description.user_fields[0].metadata?.["PARQUET:field_id"]).toBe("1");

    const schema = await client.writableSchema(description);
    expect(schema.fields.map((field) => field.name)).toEqual(["trace_id"]);
    expect(calls[1][2]).toBe(false);

    const trace = await client.getTrace("0102030405060708090a0b0c0d0e0f10", {
      since: "2026-07-01T00:00:00Z",
    });
    expect(trace.trace.spans).toHaveLength(1);
    expect(trace.trace.spans[0].events).toHaveLength(1);
    expect(trace.trace).not.toHaveProperty("events");
    expect(calls[2]).toEqual([
      "getTrace",
      "0102030405060708090a0b0c0d0e0f10",
      "2026-07-01T00:00:00Z",
      undefined,
    ]);

    const generations = await client.queryGenAi({ model: "gpt-4o", limit: 10 });
    expect(generations.next_page_token).toBe("next");
    expect(generations.rows[0].input_messages).toEqual([
      { role: "user", parts: [1, null] },
    ]);
    expect(generations.rows[0]).not.toHaveProperty("output_messages");
    expect(generations.rows[0]).not.toHaveProperty("cost_usd");
    expect(calls[3][1]).toMatchObject({ model: "gpt-4o", limit: 10 });
  });
});

describe("bifrost public typing", () => {
  it("reaches recursive fields, nested events and links, and page tokens without casts", () => {
    // Compile-only fixture: `tsc --noEmit` fails here if any position below
    // collapses back to `unknown` or a `Record<string, unknown>`.
    const description: TableDescription = {
      entry: {
        namespace: "vala.traces",
        name: "spans",
        table_uid: "01J0",
        status: "Active",
        fingerprint: "fp",
        registered_at: "2026-07-01T00:00:00Z",
        updated_at: "2026-07-01T00:00:00Z",
      },
      user_fields: [
        {
          name: "attributes",
          data_type: {
            List: {
              name: "item",
              data_type: { Struct: [{ name: "inner", data_type: "Utf8", nullable: true }] },
              nullable: false,
            },
          },
          nullable: true,
          metadata: { "PARQUET:field_id": "7" },
        },
      ],
      correlation_fields: [],
      managed_candidates: [],
      canonical_physical_fingerprint: "canonical-fp",
      physical_layout: {
        partition_granularity: "Hour",
        sort_keys: [{ column: "trace_id", direction: "Ascending", null_order: "Last" }],
        bloom_columns: ["trace_id"],
      },
    };
    const nested = description.user_fields[0].data_type;
    const item = typeof nested === "string" || !("List" in nested) ? undefined : nested.List;
    const struct = item === undefined || typeof item.data_type === "string" ? undefined : item.data_type;

    const detail: TraceDetail = {
      trace: {
        trace_id: "0102",
        spans: [
          {
            span_id: "aabb",
            trace_state: "",
            flags: 1,
            name: "chat",
            kind: 3,
            start_time_unix_nano: 1,
            end_time_unix_nano: 2,
            duration_nano: 1,
            dropped_attributes_count: 0,
            events: [
              { time_unix_nano: 1, name: "retry", attributes: { attempt: 1 }, dropped_attributes_count: 0 },
            ],
            dropped_events_count: 0,
            links: [
              {
                linked_trace_id: "0304",
                linked_span_id: "ccdd",
                trace_state: "",
                flags: 0,
                dropped_attributes_count: 0,
              },
            ],
            dropped_links_count: 0,
            resource_dropped_attributes_count: 0,
            resource_schema_url: "",
            scope_name: "wyrd",
            scope_version: "1",
            scope_dropped_attributes_count: 0,
            scope_schema_url: "",
          },
        ],
      },
    };
    const page: GenAiPage = {
      rows: [{ start_time_unix_nano: 1, model: "claude", input_messages: [{ role: "user" }] }],
      next_page_token: "cursor",
    };

    expect(struct !== undefined && "Struct" in struct ? struct.Struct[0].name : undefined).toBe("inner");
    expect(description.physical_layout.sort_keys[0].column).toBe("trace_id");
    expect(description.canonical_physical_fingerprint).toBe("canonical-fp");
    expect(detail.trace.spans[0].events?.[0].name).toBe("retry");
    expect(detail.trace.spans[0].links?.[0].linked_span_id).toBe("ccdd");
    expect(page.rows[0].model).toBe("claude");
    expect(page.next_page_token).toBe("cursor");
  });
});
