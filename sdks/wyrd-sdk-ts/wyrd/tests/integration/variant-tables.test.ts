/**
 * Tables with free-form, union, and nested fields.
 *
 * A model field typed `z.any()` or as a union is stored as a Variant column, a
 * nested object as a Struct column, and a typed array as a List column. Rows
 * and Arrow batches go in; native values come back out, with 64-bit integers
 * as `bigint`.
 */
import type { Field } from "apache-arrow";
import { z } from "zod";
import type { NativeWyrdTestServer } from "@wyrd/testing";
import { expect, vi } from "vitest";

import { Bifrost, TableConfig, WyrdError } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const Point = z.object({ x: z.int(), label: z.string().optional() });

const Event = z.object({
  id: z.int(),
  payload: z.any().optional(), // free-form JSON -> Variant
  mixed: z.union([z.int(), z.string()]).optional(), // union -> Variant
  point: Point.optional(), // nested object -> Struct
  tags: z.array(z.string()).optional(), // typed array -> List
});

/** Rows exactly as the query returns them. */
const asReturned = { parse: (row: unknown) => row as Record<string, unknown> };

/**
 * Each test's own `events` table, uniquely named so tests stay independent on
 * the file's shared server, and a writer registered on it.
 */
const test = serverTest().extend<{ table: string; bifrost: Bifrost }>({
  table: async ({}, use) => {
    await use(`vala.datasets.events_${crypto.randomUUID().replaceAll("-", "")}`);
  },
  bifrost: async ({ table }, use) => {
    const bifrost = await Bifrost.connect({ table: TableConfig.fromJsonSchema(table, Event) });
    await bifrost.register();
    await use(bifrost);
    await bifrost.shutdown();
  },
});

function isVariant(field: Field | undefined): boolean {
  return field?.metadata.get("ARROW:extension:name") === "arrow.parquet.variant";
}

/** Insert rows through `bifrost`, flush, and publish them on `server`. */
async function insert(
  server: NativeWyrdTestServer,
  bifrost: Bifrost,
  ...events: z.infer<typeof Event>[]
): Promise<void> {
  for (const event of events) {
    bifrost.insert(event);
  }
  await bifrost.flush();
  server.flushBifrost();
}

test("maps model fields to Variant, Struct, and List columns", ({ bifrost }) => {
  const schema = bifrost.table?.arrowSchema;
  const field = (name: string) => schema?.fields.find((f) => f.name === name);

  expect(isVariant(field("payload"))).toBe(true);
  expect(isVariant(field("mixed"))).toBe(true);
  expect(String(field("point")?.type)).toMatch(/^Struct/);
  expect(String(field("tags")?.type)).toMatch(/^List/);
  expect(field("tags")?.type.children[0]?.nullable).toBe(false); // string[] items are never null
});

test("returns inserted rows as native values", async ({ server, bifrost, table }) => {
  await insert(
    server,
    bifrost,
    { id: 1, payload: { scores: [1, { z: null }] }, mixed: 5, point: { x: 1, label: "a" }, tags: ["a", "b"] },
    { id: 2, payload: '{"stays": "a string"}', mixed: "five", tags: [] },
  );

  const rows = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${table} ORDER BY id`, [], asReturned);

  expect(rows).toEqual([
    { id: 1n, payload: { scores: [1, { z: null }] }, mixed: 5, point: { x: 1n, label: "a" }, tags: ["a", "b"] },
    { id: 2n, payload: '{"stays": "a string"}', mixed: "five", point: null, tags: [] },
  ]);
});

test("copies query results into another table", async ({ server, bifrost, table }) => {
  const archive = `vala.datasets.archive_${crypto.randomUUID().replaceAll("-", "")}`;
  const archiver = await Bifrost.connect({
    table: TableConfig.fromJsonSchema(archive, Event),
  });
  await archiver.register();
  await insert(server, bifrost, { id: 1, payload: { source: "row" }, mixed: 8, point: { x: 1 }, tags: ["a"] });

  // Not the declared (alphabetical) order: the server matches columns by name.
  const copied = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${table}`);
  await bifrost.writeBatch(archive, copied.batches[0]);
  server.flushBifrost();

  const rows = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${archive}`, [], asReturned);
  expect(rows).toEqual([{ id: 1n, payload: { source: "row" }, mixed: 8, point: { x: 1n, label: null }, tags: ["a"] }]);
});

test("keeps Struct fields typed and Variant paths Variant", async ({ server, bifrost, table }) => {
  await insert(server, bifrost, { id: 1, payload: { scores: [1, 2] }, point: { x: 7 } });

  const result = await bifrost.sql(`SELECT point['x'] AS x, payload -> 'scores' AS scores FROM ${table}`);
  const [x, scores] = result.schema.fields;

  expect(String(x?.type)).toBe("Int64");
  expect(isVariant(scores)).toBe(true);
  expect(Array.from(result.batches[0].getChild("x")?.toArray() ?? [])).toEqual([7n]);
});

test("refuses an undeclared field and writes nothing", async ({ server, bifrost, table }) => {
  expect(() => bifrost.insert({ id: 1, undeclared: true })).toThrow(
    expect.objectContaining({ code: "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD" }),
  );
  await bifrost.flush();
  server.flushBifrost();

  expect((await bifrost.sql(`SELECT id FROM ${table}`)).numRows).toBe(0);
});

test("refuses a model allowing extra keys", () => {
  const Loose = z.looseObject({ id: z.int() }); // extra keys could be silently lost

  let error: unknown;
  try {
    TableConfig.fromJsonSchema("vala.datasets.loose", Loose);
  } catch (thrown) {
    error = thrown;
  }

  expect(error).toBeInstanceOf(WyrdError);
  expect(error).toMatchObject({ code: "WYRD_VALA_400_SCHEMA_PARSE", status: 400 });
  expect((error as WyrdError).message).toContain("declare a Variant field for open data");
  expect((error as WyrdError).details).toMatchObject({
    data: {
      detail: "an object allows undeclared keys beside its declared properties; declare a Variant field for open data",
    },
  });
  expect((error as WyrdError).remediation).toBe(
    "Correct the column type or the row value so it satisfies the table's declared DataTypeSpec; declare a Variant field for open data.",
  );
});
