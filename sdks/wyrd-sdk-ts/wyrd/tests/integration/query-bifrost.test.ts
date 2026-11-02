import { randomUUID } from "node:crypto";

import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig, WyrdClient } from "wyrd-ai";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The `bifrost.datasets` table the query stories read, holding rows 1 to 3. */
const QUERY_ROWS = "bifrost.datasets.query_rows";

/** The user columns of {@link QUERY_ROWS}, as a table declaration. */
const QueryRowColumns = z.object({ id: z.int(), value: z.string() });

/** One {@link QUERY_ROWS} row as a query returns it: `id` is an Arrow int64. */
const QueryRow = z.object({ id: z.bigint(), value: z.string() });

/** A table holding the same rows that Scribe acknowledged and nothing published. */
const LIVE_ROWS = "bifrost.datasets.live_query_rows";

/**
 * Writes rows 1 to 3 of `table` through the SDK and waits for Scribe's
 * acknowledgement; rows 1 and 2 share one managed `run_id`, row 3 another.
 */
async function writeRows(table: string): Promise<void> {
  const writer = await Bifrost.connect({ table: TableConfig.fromJsonSchema(table, QueryRowColumns) });
  await writer.register();
  const [shared, other] = [randomUUID(), randomUUID()];
  for (const [id, value, runId] of [
    [1, "one", shared],
    [2, "two", shared],
    [3, "three", other],
  ] as const) {
    writer.insert({ id, value }, { runId });
  }
  await writer.flush();
  await writer.shutdown();
}

/**
 * The query table, written and published once through the SDK, and a reader;
 * and the live table, written after that publication so only Scribe's live
 * tail serves it.
 */
const test = serverTest().extend<{ bifrost: Bifrost; live: string }>({
  live: [
    async ({ bifrost }, use) => {
      void bifrost;
      await writeRows(LIVE_ROWS);
      await use(LIVE_ROWS);
    },
    { scope: "file" },
  ],
  bifrost: [
    async ({ server }, use) => {
      await writeRows(QUERY_ROWS);
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
  const rows = await bifrost.sql(
    "SELECT COUNT(*) AS n FROM bifrost.verification.execution_outputs",
    [],
    z.object({ n: z.bigint() }),
  );

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

test("caller without bifrost read is refused", async ({ server }) => {
  const denied = await Bifrost.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("query_denied", ["cards:read"]) }),
  });

  await expect(denied.sql(`SELECT id FROM ${QUERY_ROWS}`)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

/** One `id` result: an Arrow int64. */
const Id = z.object({ id: z.bigint() });

/** The `id` of every row `sql` returns, in order. */
async function ids(bifrost: Bifrost, sql: string): Promise<bigint[]> {
  return (await bifrost.sql(sql, [], Id)).map((row) => row.id);
}

test.for([
  {
    shape: "scalar",
    subquery: `SELECT id FROM ${LIVE_ROWS} WHERE id = (SELECT id FROM ${LIVE_ROWS} ORDER BY id DESC LIMIT 1)`,
    join: `SELECT a.id FROM ${LIVE_ROWS} a JOIN (SELECT MAX(id) AS id FROM ${LIVE_ROWS}) b ON a.id = b.id`,
    expected: [3n],
  },
  {
    shape: "in",
    subquery: `SELECT id FROM ${LIVE_ROWS} WHERE id IN (SELECT id FROM ${LIVE_ROWS} WHERE value <> 'two') ORDER BY id`,
    join: `SELECT a.id FROM ${LIVE_ROWS} a JOIN ${LIVE_ROWS} b ON a.id = b.id WHERE b.value <> 'two' ORDER BY a.id`,
    expected: [1n, 3n],
  },
  {
    shape: "exists",
    subquery: `SELECT a.id FROM ${LIVE_ROWS} a WHERE EXISTS (SELECT 1 FROM ${LIVE_ROWS} b WHERE b.id = a.id + 1) ORDER BY a.id`,
    join: `SELECT a.id FROM ${LIVE_ROWS} a JOIN ${LIVE_ROWS} b ON b.id = a.id + 1 ORDER BY a.id`,
    expected: [1n, 2n],
  },
  {
    shape: "managed run_id",
    subquery: `SELECT a.id FROM ${LIVE_ROWS} a WHERE EXISTS (SELECT 1 FROM ${LIVE_ROWS} b WHERE b.run_id = a.run_id AND b.id <> a.id) ORDER BY a.id`,
    join: `SELECT DISTINCT a.id FROM ${LIVE_ROWS} a JOIN ${LIVE_ROWS} b ON b.run_id = a.run_id AND b.id <> a.id ORDER BY a.id`,
    expected: [1n, 2n],
  },
])("live table $shape subquery matches its join", async ({ subquery, join, expected }, { bifrost, live }) => {
  void live;
  expect(await ids(bifrost, subquery)).toEqual(expected);
  expect(await ids(bifrost, join)).toEqual(expected);
});

test.for([
  { shape: "single", sql: `SELECT r.valeu FROM ${LIVE_ROWS} r` },
  {
    shape: "join",
    sql: `SELECT r.valeu, r.valeu AS again FROM ${LIVE_ROWS} r JOIN ${LIVE_ROWS} s ON r.id = s.id JOIN bifrost.verification.results v ON v.result_id = s.value`,
  },
  { shape: "subquery", sql: `SELECT id FROM ${LIVE_ROWS} WHERE id IN (SELECT r.valeu FROM ${LIVE_ROWS} r)` },
])("unknown column in a $shape query carries the planners suggestion", async ({ sql }, { bifrost, live }) => {
  void live;
  await expect(bifrost.sql(sql)).rejects.toMatchObject({
    code: "WYRD_VALA_400_QUERY_INVALID_SQL",
    detail: expect.stringContaining("No field named r.valeu. Did you mean 'r.value'?"),
  });
});

test.for([
  {
    fault: "type mismatch",
    sql: `SELECT value + 1 AS n FROM ${LIVE_ROWS}`,
    detail: "Cannot coerce arithmetic expression Utf8 + Int64",
  },
  {
    fault: "ambiguous reference",
    sql: `SELECT id FROM ${LIVE_ROWS} a JOIN ${LIVE_ROWS} b ON a.id = b.id`,
    detail: "Ambiguous reference to unqualified field id",
  },
])("$fault carries the planners detail", async ({ sql, detail }, { bifrost, live }) => {
  void live;
  await expect(bifrost.sql(sql)).rejects.toMatchObject({
    code: "WYRD_VALA_400_QUERY_INVALID_SQL",
    detail: expect.stringContaining(detail),
  });
});

test("unauthorized table refusal reveals no columns", async ({ server, live }) => {
  const datasetsOnly = {
    resource: "bifrost_query",
    action: "read",
    scope: { bifrost: { schema: { catalog: "bifrost", schema: "datasets" } } },
  };
  const reader = await Bifrost.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("datasets_reader", [datasetsOnly]) }),
  });

  expect(await ids(reader, `SELECT id FROM ${live} ORDER BY id`)).toEqual([1n, 2n, 3n]);
  const refused = await reader
    .sql(`SELECT q.id, v.verdcit FROM ${live} q JOIN bifrost.verification.results v ON v.result_id = q.value`)
    .then(
      () => undefined,
      (error: unknown) => error as { code: string; detail: string },
    );
  expect(refused?.code).toBe("WYRD_VALA_403_QUERY_FORBIDDEN");
  expect(refused?.detail).not.toMatch(/verdcit|verdict/);
  await reader.shutdown();
});

test("predicates are typed bounded and tenant scoped", async ({ server, bifrost, live }) => {
  const [namespace, name] = [live.slice(0, live.lastIndexOf(".")), live.slice(live.lastIndexOf(".") + 1)];
  expect(await bifrost.validatePredicate(namespace, name, "id >= 2 and value like 't%'")).toBe(
    "id >= 2 AND value LIKE 't%'",
  );
  expect(await bifrost.validatePredicate(namespace, name, "run_id is not null")).toBe("run_id IS NOT NULL");
  expect(await bifrost.validatePredicate(namespace, name, " ")).toBe("");
  for (const expression of [
    "id = 1; DROP TABLE live_query_rows",
    `id IN (SELECT id FROM ${live})`,
    "id = $1",
    "id = 1 -- comment",
    "verdict = 'passed'",
    `value = '${"x".repeat(8192)}'`,
  ]) {
    await expect(bifrost.validatePredicate(namespace, name, expression), expression).rejects.toMatchObject({
      code: "WYRD_VALA_400_QUERY_INVALID_SQL",
    });
  }

  const datasetsOnly = {
    resource: "bifrost_query",
    action: "read",
    scope: { bifrost: { schema: { catalog: "bifrost", schema: "datasets" } } },
  };
  const reader = await Bifrost.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("predicate_reader", [datasetsOnly]) }),
  });
  expect(await reader.validatePredicate(namespace, name, "id = 1")).toBe("id = 1");
  const forbidden = await reader.validatePredicate("bifrost.verification", "results", "verdcit = 'passed'").then(
    () => undefined,
    (error: unknown) => error as { code: string; detail: string },
  );
  expect(forbidden?.code).toBe("WYRD_VALA_403_QUERY_FORBIDDEN");
  expect(forbidden?.detail).not.toMatch(/verdcit|verdict/);
  await reader.shutdown();

  const other = await Bifrost.connect({
    client: WyrdClient.connect({
      credential: server.bootstrapServiceInTenant(server.seedTenant("predicate-other"), ["admin"], "predicate-other"),
    }),
  });
  await expect(other.validatePredicate(namespace, name, "id = 1")).rejects.toMatchObject({
    code: "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
  });
  await other.shutdown();
});
