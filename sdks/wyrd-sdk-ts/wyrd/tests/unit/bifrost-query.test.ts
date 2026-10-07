import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
  BifrostQueryStream,
  IncompleteQueryStreamError,
  TableConfig,
  WyrdError,
  type CompactionType,
  type TableDescription,
} from "@wyrd/sdk";

const REQUEST_ID = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff";

describe("BifrostQueryStream", () => {
  it("yields Apache Arrow batches and retains the validated terminal", async () => {
    const ipc = tableToIPC(tableFromArrays({ value: [1, 2] }), "stream");
    const terminal = {
      outcome: "success",
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
      schemaIpc: null,
      terminalJson: null,
    };
    const stream = BifrostQueryStream.fromNative(native);
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
      schemaIpc: null,
      terminalJson: null,
    };
    const stream = BifrostQueryStream.fromNative(native);
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
      schemaIpc: null,
      terminalJson: null,
    };
    const stream = BifrostQueryStream.fromNative(native);
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
      schemaIpc: null,
      terminalJson: JSON.stringify(terminal),
    };
    const stream = BifrostQueryStream.fromNative(native);
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
      schemaIpc: null,
      terminalJson: null,
    };
    const stream = BifrostQueryStream.fromNative(native);
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
      compaction_target_file_size_bytes: 1_073_741_824,
      compaction_type: "small-files",
    };
    const nested = description.user_fields[0].data_type;
    const item = typeof nested === "string" || !("List" in nested) ? undefined : nested.List;
    const struct = item === undefined || typeof item.data_type === "string" ? undefined : item.data_type;

    expect(struct !== undefined && "Struct" in struct ? struct.Struct[0].name : undefined).toBe("inner");
    expect(description.physical_layout.sort_keys[0].column).toBe("trace_id");
    expect(description.canonical_physical_fingerprint).toBe("canonical-fp");
    expect(description.compaction_target_file_size_bytes).toBe(1_073_741_824);
    expect(description.compaction_type).toBe("small-files");
  });
});

describe("TableConfig compaction target", () => {
  const SCHEMA = {
    type: "object",
    properties: { id: { type: "integer" } },
    required: ["id"],
  };

  it("defaults to the deployment target and carries an explicit one", () => {
    expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA).compactionTargetFileSizeBytes).toBeUndefined();
    const target = 256 * 1024 * 1024;
    expect(
      TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionTargetFileSizeBytes: target })
        .compactionTargetFileSizeBytes,
    ).toBe(target);
  });

  it("refuses a target that is not a non-negative integer", () => {
    for (const bytes of [1.5, -1]) {
      expect(() =>
        TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionTargetFileSizeBytes: bytes }),
      ).toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }));
    }
  });
});

describe("TableConfig compaction type", () => {
  const SCHEMA = {
    type: "object",
    properties: { id: { type: "integer" } },
    required: ["id"],
  };

  it("defaults to small-files by declaring nothing and carries an explicit type", () => {
    expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA).compactionType).toBeUndefined();
    expect(
      TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionType: "small-files" }).compactionType,
    ).toBe("small-files");
  });

  it("carries every hyphenated wire compaction type", () => {
    for (const kind of ["auto", "full", "small-files", "files-with-delete"] as const) {
      expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionType: kind }).compactionType).toBe(kind);
    }
  });

  it("refuses a spelling that is not a wire compaction type", () => {
    for (const refused of ["small_files", "files_with_delete"]) {
      expect(() =>
        TableConfig.fromJsonSchema(
          "unit.rows",
          SCHEMA,
          { compactionType: refused as unknown as CompactionType },
        ),
      ).toThrow(/compaction type/);
    }
  });
});
