// TypeScript's own query surface beyond the shared `query_bifrost` story:
// Arrow and IPC conversion of a collected result and the JavaScript number
// range of a deadline. Not mirrored.
import { tableFromIPC } from "apache-arrow";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The `vala.datasets` table the conversion reads, holding ids 1 to 3. */
const IDS = "vala.datasets.result_ids";

/** The ids table, written and published once through the SDK, and a reader. */
const test = serverTest().extend<{ bifrost: Bifrost }>({
  bifrost: [
    async ({ server }, use) => {
      const writer = await Bifrost.connect({ table: TableConfig.fromJsonSchema(IDS, z.object({ id: z.int() })) });
      await writer.register();
      for (const id of [1, 2, 3]) {
        writer.insert({ id });
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

test("collected result converts to arrow and ipc bytes", async ({ bifrost }) => {
  const result = await bifrost.sql(`SELECT id FROM ${IDS} ORDER BY id`);

  for (const table of [result.toArrow(), tableFromIPC(result.toBytes())]) {
    expect(table.numRows).toBe(result.numRows);
    expect(Array.from(table.getChild("id")?.toArray() ?? [])).toEqual([1n, 2n, 3n]);
  }
});

// The deadline is range-checked with the rest of the query request.
test.for([0, 2 ** 32, 1.5, Number.NaN])("out of range deadline is refused: %s", async (deadlineMs, { bifrost }) => {
  await expect(bifrost.stream("SELECT 1", [], { deadlineMs })).rejects.toMatchObject({
    code: "WYRD_VALA_400_QUERY_INVALID_DEADLINE",
  });
});
