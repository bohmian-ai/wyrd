/**
 * Tables with free-form, union, and nested fields.
 *
 * A model field typed `z.any()` or as a union is stored as a Variant column, a
 * nested object as a Struct column, and a typed array as a List column. Rows
 * and Arrow batches go in; native values come back out, with 64-bit integers
 * as `bigint`.
 */
import { Int64, Table, Utf8, vectorFromArray, type Field } from "apache-arrow";
import { z } from "zod";
import { startTestServer } from "@wyrd/testing";
import { afterEach, beforeEach, expect, it } from "vitest";

import { Bifrost, TableConfig, WyrdError } from "@wyrd/sdk";

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

let server: ReturnType<typeof startTestServer>;
let bifrost: Bifrost;
let table: string;

beforeEach(async () => {
  server = startTestServer();
  table = `vala.datasets.events_${crypto.randomUUID().replaceAll("-", "")}`;
  bifrost = await Bifrost.connect({
    table: TableConfig.fromJsonSchema(table, Event),
    serverUrl: server.baseUrl,
    credential: server.apiKey,
    grpcUrl: server.grpcUrl,
  });
  await bifrost.register();
});

afterEach(() => {
  server.shutdown();
});

function isVariant(field: Field | undefined): boolean {
  return field?.metadata.get("ARROW:extension:name") === "arrow.parquet.variant";
}

/** Insert rows, flush, and publish them. */
async function insert(...events: z.infer<typeof Event>[]): Promise<void> {
  for (const event of events) {
    bifrost.insert(event);
  }
  await bifrost.flush();
  server.flushBifrost();
}

/** Capture an asynchronous structured error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

it("maps model fields to Variant, Struct, and List columns", () => {
  const schema = bifrost.table!.arrowSchema;
  const field = (name: string) => schema.fields.find((f) => f.name === name);

  expect(isVariant(field("payload"))).toBe(true);
  expect(isVariant(field("mixed"))).toBe(true);
  expect(String(field("point")?.type)).toMatch(/^Struct/);
  expect(String(field("tags")?.type)).toMatch(/^List/);
  expect(field("tags")?.type.children[0]?.nullable).toBe(false); // string[] items are never null
});

it("returns inserted rows as native values", async () => {
  await insert(
    { id: 1, payload: { scores: [1, { z: null }] }, mixed: 5, point: { x: 1, label: "a" }, tags: ["a", "b"] },
    { id: 2, payload: '{"stays": "a string"}', mixed: "five", tags: [] },
  );

  const rows = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${table} ORDER BY id`, asReturned);

  expect(rows).toEqual([
    { id: 1n, payload: { scores: [1, { z: null }] }, mixed: 5, point: { x: 1n, label: "a" }, tags: ["a", "b"] },
    { id: 2n, payload: '{"stays": "a string"}', mixed: "five", point: null, tags: [] },
  ]);
});

it("stores Arrow JSON text as Variant", async () => {
  // Variant columns take JSON text; omitted nullable columns are null.
  const arrow = new Table({
    id: vectorFromArray([1n], new Int64()),
    payload: vectorFromArray(['{"n": 9007199254740993}'], new Utf8()),
    mixed: vectorFromArray(['"seven"'], new Utf8()),
  });
  await bifrost.writeBatch(table, arrow.batches[0]!);
  server.flushBifrost();

  const rows = await bifrost.sql(`SELECT id, payload, mixed FROM ${table}`, asReturned);

  expect(rows).toEqual([{ id: 1n, payload: { n: 9007199254740993n }, mixed: "seven" }]);
});

it("copies query results into another table", async () => {
  const archive = `vala.datasets.archive_${crypto.randomUUID().replaceAll("-", "")}`;
  const archiver = await Bifrost.connect({
    table: TableConfig.fromJsonSchema(archive, Event),
    serverUrl: server.baseUrl,
    credential: server.apiKey,
    grpcUrl: server.grpcUrl,
  });
  await archiver.register();
  await insert({ id: 1, payload: { source: "row" }, mixed: 8, point: { x: 1 }, tags: ["a"] });

  const copied = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${table}`);
  await bifrost.writeBatch(archive, copied.batches[0]!);
  server.flushBifrost();

  const rows = await bifrost.sql(`SELECT id, payload, mixed, point, tags FROM ${archive}`, asReturned);
  expect(rows).toEqual([{ id: 1n, payload: { source: "row" }, mixed: 8, point: { x: 1n, label: null }, tags: ["a"] }]);
});

it("keeps Struct fields typed and Variant paths Variant", async () => {
  await insert({ id: 1, payload: { scores: [1, 2] }, point: { x: 7 } });

  const result = await bifrost.sql(`SELECT point['x'] AS x, payload -> 'scores' AS scores FROM ${table}`);
  const [x, scores] = result.schema.fields;

  expect(String(x?.type)).toBe("Int64");
  expect(isVariant(scores)).toBe(true);
  expect(Array.from(result.batches[0]!.getChild("x")!.toArray())).toEqual([7n]);
});

it("refuses an undeclared field and writes nothing", async () => {
  expect(() => bifrost.insert({ id: 1, undeclared: true })).toThrow(
    expect.objectContaining({ code: "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD" }),
  );
  await bifrost.flush();
  server.flushBifrost();

  expect((await bifrost.sql(`SELECT id FROM ${table}`)).numRows).toBe(0);
});

it("refuses invalid JSON text and writes nothing", async () => {
  const arrow = new Table({
    id: vectorFromArray([1n], new Int64()),
    payload: vectorFromArray(["{not json"], new Utf8()),
  });

  const error = await rejection(bifrost.writeBatch(table, arrow.batches[0]!));
  server.flushBifrost();

  expect(error.code).toBe("WYRD_VALA_400_VARIANT_INVALID_JSON");
  expect((await bifrost.sql(`SELECT id FROM ${table}`)).numRows).toBe(0);
});

it("refuses a model allowing extra keys", () => {
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
});

it("refuses a Variant column sent as neither Variant nor JSON text", async () => {
  // JSON Schema cannot declare the types Iceberg cannot store, so this is
  // where a TypeScript caller meets the unsupported-type refusal.
  const arrow = new Table({
    id: vectorFromArray([1n], new Int64()),
    payload: vectorFromArray([7n], new Int64()),
  });

  const error = await rejection(bifrost.writeBatch(table, arrow.batches[0]!));
  server.flushBifrost();

  expect(error).toMatchObject({ code: "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE", status: 400 });
  expect(error.message).toContain("for field payload");
  expect((await bifrost.sql(`SELECT id FROM ${table}`)).numRows).toBe(0);
});
