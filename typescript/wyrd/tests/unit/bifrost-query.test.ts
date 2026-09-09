import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
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
