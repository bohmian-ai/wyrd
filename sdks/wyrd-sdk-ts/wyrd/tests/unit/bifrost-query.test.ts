import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, test } from "vitest";

import { BifrostQueryStream, IncompleteQueryStreamError, TableConfig, type CompactionType } from "@wyrd/sdk";

const REQUEST_ID = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff";

type NativeStream = Parameters<typeof BifrostQueryStream.fromNative>[0];
type NativeStep = Awaited<ReturnType<NativeStream["next"]>>;

/**
 * A fake native query stream that answers `steps` in order, then repeats the
 * last one, and records whether it was closed. These tests cover the wrapper's
 * fault paths. The `query-bifrost` journey covers the happy path.
 */
function fakeStream(steps: Partial<NativeStep>[], terminalJson: string | null = null) {
  const state = { closed: false, closeError: undefined as Error | undefined };
  let index = 0;
  const native: NativeStream = {
    requestId: REQUEST_ID,
    async next() {
      const step = steps[Math.min(index++, steps.length - 1)];
      return { ipc: undefined, terminalJson: undefined, ...step } as NativeStep;
    },
    async close() {
      state.closed = true;
      if (state.closeError) {
        throw state.closeError;
      }
    },
    schemaIpc: null,
    terminalJson,
  };
  return { stream: BifrostQueryStream.fromNative(native), state };
}

const DONE = { done: true, value: undefined };

// Internal: these drive the TypeScript wrapper over a fake native stream,
// which no user constructs; the `query-bifrost` journey proves the public path.
describe("internal native query stream wrapper", () => {
  test("arrow batches are yielded and the validated terminal is retained", async () => {
    const terminal = { outcome: "success", row_count: 2, warnings: [], source_completion: [], error: null };
    const { stream } = fakeStream([
      { ipc: Buffer.from(tableToIPC(tableFromArrays({ value: [1, 2] }), "stream")) },
      { terminalJson: JSON.stringify(terminal) },
    ]);

    const rows = [];
    for await (const batch of stream) {
      rows.push(batch.numRows);
    }

    expect([stream.requestId, rows, stream.terminal]).toEqual([REQUEST_ID, [2], terminal]);
  });

  test("stopping iteration early closes the native response", async () => {
    const { stream, state } = fakeStream([{}]);

    await stream.return();

    expect(state.closed).toBe(true);
  });

  test("stream that ends before its terminal raises the incomplete stream error once", async () => {
    const { stream } = fakeStream([
      {
        errorCode: "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
        errorStatus: 502,
        errorTitle: "Query stream incomplete",
        errorDetail: "response ended before terminal",
      },
    ]);

    await expect(stream.next()).rejects.toBeInstanceOf(IncompleteQueryStreamError);
    await expect(stream.next()).resolves.toEqual(DONE);
  });

  test("failed terminal keeps its step details and status", async () => {
    const terminal = { outcome: "failed", error: { code: "query_execution_failed", detail: "source unavailable" } };
    const { stream } = fakeStream(
      [
        {
          errorCode: "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
          errorStatus: 500,
          errorTitle: "Query execution failed",
          errorDetail: "source unavailable",
          errorRemediation: "Inspect the retained terminal error.",
          errorDetailsJson: JSON.stringify(terminal),
        },
      ],
      JSON.stringify(terminal),
    );

    await expect(stream.next()).rejects.toMatchObject({
      code: "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
      status: 500,
      detail: "source unavailable",
      details: terminal,
    });
  });

  test("arrow decode failure closes the native response before it propagates", async () => {
    const { stream, state } = fakeStream([{ ipc: Buffer.from("not-arrow") }]);
    state.closeError = new Error("cleanup failed");

    const failure = await stream.next().catch((reason: unknown) => reason);

    expect(failure).toBeInstanceOf(Error);
    expect(failure).not.toBe(state.closeError);
    expect(state.closed).toBe(true);
    await expect(stream.next()).resolves.toEqual(DONE);
  });
});

const SCHEMA = { type: "object", properties: { id: { type: "integer" } }, required: ["id"] };

test("compaction target defaults to the deployment and carries an explicit one", () => {
  const target = 256 * 1024 * 1024;

  expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA).compactionTargetFileSizeBytes).toBeUndefined();
  expect(
    TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionTargetFileSizeBytes: target })
      .compactionTargetFileSizeBytes,
  ).toBe(target);
});

test.for([1.5, -1])("compaction target %s is refused", (bytes) => {
  expect(() => TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionTargetFileSizeBytes: bytes })).toThrow(
    expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }),
  );
});

test("compaction type defaults to the deployment", () => {
  expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA).compactionType).toBeUndefined();
});

test.for(["auto", "full", "small-files", "files-with-delete"] as const)("compaction type %s is carried", (kind) => {
  expect(TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionType: kind }).compactionType).toBe(kind);
});

test.for(["small_files", "files_with_delete"])("compaction type spelled %s is refused", (refused) => {
  expect(() => TableConfig.fromJsonSchema("unit.rows", SCHEMA, { compactionType: refused as CompactionType })).toThrow(
    expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }),
  );
});
