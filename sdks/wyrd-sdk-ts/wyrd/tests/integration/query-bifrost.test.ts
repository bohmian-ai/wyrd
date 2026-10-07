import { tableFromIPC } from "apache-arrow";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The `vala.datasets` table the query stories read, holding rows 1 to 3. */
const QUERY_ROWS = "vala.datasets.query_rows";

/** The user columns of {@link QUERY_ROWS}, as a table declaration. */
const QueryRowColumns = z.object({ id: z.int(), value: z.string() });

/** One {@link QUERY_ROWS} row as a query returns it: `id` is an Arrow int64. */
const QueryRow = z.object({ id: z.bigint(), value: z.string() });

/** The query table, written and published once through the SDK, and a reader. */
const test = serverTest().extend<{ bifrost: Bifrost }>({
  bifrost: [
    async ({ server }, use) => {
      const writer = await Bifrost.connect({ table: TableConfig.fromJsonSchema(QUERY_ROWS, QueryRowColumns) });
      await writer.register();
      for (const [id, value] of [
        [1, "one"],
        [2, "two"],
        [3, "three"],
      ] as const) {
        writer.insert({ id, value });
      }
      await writer.flush();
      await writer.shutdown();
      server.flushBifrost();
      const bifrost = await Bifrost.connect();
      await use(bifrost);
      await bifrost.shutdown();
    },
    { scope: "file" },
  ],
});

test("parameterized sql returns the callers rows", async ({ bifrost }) => {
  const rows = await bifrost.sql(`SELECT id, value FROM ${QUERY_ROWS} WHERE id >= $1 ORDER BY id`, [2], QueryRow);

  expect(rows).toEqual([
    { id: 2n, value: "two" },
    { id: 3n, value: "three" },
  ]);
});

test("bound sql text is treated as data", async ({ bifrost }) => {
  const rows = await bifrost.sql(`SELECT id, value FROM ${QUERY_ROWS} WHERE value = $1`, ["one' OR '1'='1"], QueryRow);

  expect(rows).toEqual([]);
});

test("unwritten builtin table reads as empty", async ({ bifrost }) => {
  const rows = await bifrost.sql("SELECT COUNT(*) AS n FROM vala.eval.result_items", [], z.object({ n: z.bigint() }));

  expect(rows).toEqual([{ n: 0n }]);
});

test("stream yields arrow batches and a terminal", async ({ bifrost }) => {
  const stream = await bifrost.stream(`SELECT id FROM ${QUERY_ROWS} ORDER BY id`);

  const ids = [];
  for await (const batch of stream) {
    ids.push(...(batch.getChild("id")?.toArray() ?? []));
  }

  expect(ids).toEqual([1n, 2n, 3n]);
  expect(stream.terminal).toMatchObject({ outcome: "success", row_count: 3, warnings: [] });
});

test("collected result converts to arrow and ipc bytes", async ({ bifrost }) => {
  const result = await bifrost.sql(`SELECT id FROM ${QUERY_ROWS} ORDER BY id`);

  for (const table of [result.toArrow(), tableFromIPC(result.toBytes())]) {
    expect(table.numRows).toBe(result.numRows);
    expect(Array.from(table.getChild("id")?.toArray() ?? [])).toEqual([1n, 2n, 3n]);
  }
});

test("caller without bifrost read is refused", async ({ server }) => {
  const denied = await Bifrost.connect({ credential: server.scopedApiKey("query_denied", ["cards:read"]) });

  await expect(denied.sql(`SELECT id FROM ${QUERY_ROWS}`)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

// The deadline is range-checked with the rest of the query request.
test.for([0, 2 ** 32, 1.5, Number.NaN])("out of range deadline is refused: %s", async (deadlineMs, { bifrost }) => {
  await expect(bifrost.stream({ sql: "SELECT 1", deadlineMs })).rejects.toMatchObject({
    code: "WYRD_VALA_400_QUERY_INVALID_DEADLINE",
  });
});
