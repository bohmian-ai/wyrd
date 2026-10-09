import { Field, Int32, RecordBatch, Schema, Struct, makeBuilder, makeData, tableFromIPC } from "apache-arrow";
import { expect, vi } from "vitest";
import { ZodError, z } from "zod";

import { Bifrost, TableConfig, type TableConfigOptions, WyrdClient, record } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** One model call, declared the way a TypeScript user already models data. */
const Inference = z.object({ call_id: z.int(), model: z.string(), tokens: z.int(), latency_ms: z.number() });

const INFERENCES: z.infer<typeof Inference>[] = [
  { call_id: 1, model: "opus", tokens: 100, latency_ms: 120.5 },
  { call_id: 2, model: "opus", tokens: 300, latency_ms: 240.0 },
  { call_id: 3, model: "haiku", tokens: 50, latency_ms: 30.0 },
];

/** The table every read story queries, holding {@link INFERENCES}. */
const INFERENCE_TABLE = "vala.datasets.inferences";

/** A one-column table declaration for the write stories. */
const Value = z.object({ value: z.int() });

/** One single-column batch holding `value`, shaped by the described schema. */
function valueBatch(schema: Schema, value: bigint): RecordBatch {
  const builder = makeBuilder({ type: schema.fields[0].type });
  builder.append(value);
  builder.finish();
  return new RecordBatch(schema, makeData({ type: new Struct(schema.fields), length: 1, children: [builder.flush()] }));
}

/** Register `fqn` as a {@link Value} table with `options`, then release the writer. */
async function registerValueTable(fqn: string, options: TableConfigOptions = {}): Promise<string> {
  const bifrost = await Bifrost.connect({ table: TableConfig.fromJsonSchema(fqn, Value, options) });
  try {
    return await bifrost.register();
  } finally {
    await bifrost.shutdown();
  }
}

/**
 * The published inference table and a reader for it, and the keys of
 * service A, which may only query, and service B, which may also write.
 */
const test = serverTest().extend<{ reader: Bifrost; a: string; b: string }>({
  a: [async ({ server }, use) => use(server.scopedApiKey("delegation_a", ["bifrost_query:read"])), { scope: "file" }],
  b: [
    async ({ server }, use) =>
      use(server.scopedApiKey("delegation_b", ["bifrost_query:read", "bifrost_record:write", "bifrost_table:read"])),
    { scope: "file" },
  ],
  reader: [
    async ({ server }, use) => {
      const writer = await Bifrost.connect({ table: TableConfig.fromJsonSchema(INFERENCE_TABLE, Inference) });
      await writer.register();
      for (const row of INFERENCES) {
        writer.insert(row);
      }
      await writer.flush();
      await writer.shutdown();
      server.flushBifrost();
      const reader = await Bifrost.connect();
      await use(reader);
      await reader.shutdown();
    },
    { scope: "file" },
  ],
});

test("registering the same columns again matches", async () => {
  expect(await registerValueTable("vala.datasets.registered")).toBe("created");
  expect(await registerValueTable("vala.datasets.registered")).toBe("already_exists");
});

test("swapping tables drains the rows written before the swap", async ({ server, reader }) => {
  await registerValueTable("vala.datasets.before_swap");
  await registerValueTable("vala.datasets.after_swap");
  const writer = await Bifrost.connect();

  await writer.useTableByName("vala.datasets.before_swap");
  writer.insert({ value: 71 });
  await writer.useTableByName("vala.datasets.after_swap");
  writer.insert({ value: 91 });
  await writer.shutdown();
  server.flushBifrost();

  const read = (table: string) => reader.sql(`SELECT value FROM ${table}`, [], Value.extend({ value: z.bigint() }));
  expect(await read("vala.datasets.before_swap")).toEqual([{ value: 71n }]);
  expect(await read("vala.datasets.after_swap")).toEqual([{ value: 91n }]);
});

/** One insert the client refuses before sending, and the catalog code it raises. */
type Refusal = readonly [string, (bifrost: Bifrost) => void, string];

test.for<Refusal>([
  ["no active table", (bifrost) => bifrost.insert({ value: 81 }), "WYRD_VALA_412_NO_ACTIVE_TABLE"],
  [
    "malformed Card UID",
    (bifrost) => {
      bifrost.useTable(TableConfig.fromJsonSchema("vala.datasets.refused", Value));
      bifrost.insert({ value: 82 }, { cardUid: "not-a-uid" });
    },
    "WYRD_SPEC_400_VALIDATION",
  ],
])("refused insert raises its catalog code: %s", async ([, insert, code], { server: _ }) => {
  const bifrost = await Bifrost.connect();

  expect(() => insert(bifrost)).toThrow(expect.objectContaining({ code }));

  await bifrost.shutdown();
});

test("compaction target is recorded and a different one is refused", async () => {
  const target = 256 * 1024 * 1024;

  expect(await registerValueTable("vala.datasets.target", { compactionTargetFileSizeBytes: target })).toBe("created");
  expect(await registerValueTable("vala.datasets.target")).toBe("already_exists");
  await expect(
    registerValueTable("vala.datasets.target", { compactionTargetFileSizeBytes: target * 2 }),
  ).rejects.toMatchObject({ code: "WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH" });
  expect((await TableConfig.describe("vala.datasets.target")).compactionTargetFileSizeBytes).toBe(target);
});

test("compaction type is recorded and a different one is refused", async () => {
  expect(await registerValueTable("vala.datasets.kind", { compactionType: "small-files" })).toBe("created");
  expect(await registerValueTable("vala.datasets.kind")).toBe("already_exists");
  await expect(registerValueTable("vala.datasets.kind", { compactionType: "full" })).rejects.toMatchObject({
    code: "WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH",
  });
  expect((await TableConfig.describe("vala.datasets.kind")).compactionType).toBe("small-files");
});

test("recorded telemetry row reads back", async ({ server, reader }) => {
  const writer = await Bifrost.connect({ table: TableConfig.fromJsonSchema("vala.datasets.recorded", Value) });
  expect(await writer.register()).toBe("created");

  record(writer, "vala.datasets.recorded", Value, { value: 7 });
  await writer.shutdown();
  server.flushBifrost();

  expect(writer.dropped).toBe(0);
  const rows = await reader.sql("SELECT value FROM vala.datasets.recorded", [], z.object({ value: z.bigint() }));
  expect(rows).toEqual([{ value: 7n }]);
});

test("recorded telemetry with an unmappable schema is refused", async ({ reader }) => {
  expect(() => record(reader, "vala.datasets.recorded", { type: "object" }, { value: 7 })).toThrow(
    expect.objectContaining({ code: "WYRD_VALA_400_SCHEMA_PARSE" }),
  );
});

// Arrow declares a column type JSON Schema cannot: a 32-bit integer.
test("arrow declared table keeps its column types", async ({ server, reader }) => {
  const schema = new Schema([new Field("small", new Int32(), false)]);
  const writer = await Bifrost.connect({ table: TableConfig.fromArrow("vala.datasets.arrow_declared", schema) });
  expect(await writer.register()).toBe("created");

  writer.insert({ small: 32 });
  await writer.shutdown();
  server.flushBifrost();

  const read = await reader.sql("SELECT small FROM vala.datasets.arrow_declared");
  expect(read.toArrow().schema.fields.map((field) => [field.name, String(field.type)])).toEqual([["small", "Int32"]]);
  expect(read.toArrow().getChild("small")?.toArray()).toEqual(Int32Array.from([32]));
});

test("described table accepts writes without restating its schema", async ({ server, reader }) => {
  await registerValueTable("vala.datasets.described");
  const writer = await Bifrost.connect({ table: await TableConfig.describe("vala.datasets.described") });

  writer.insert({ value: 91 });
  await writer.shutdown();
  server.flushBifrost();

  expect(writer.dropped).toBe(0);
  expect((await reader.sql("SELECT value FROM vala.datasets.described")).numRows).toBe(1);
});

test("aggregate sql reads the written rows", async ({ reader }) => {
  const rows = await reader.sql(
    `SELECT model, CAST(SUM(tokens) AS BIGINT) AS tokens FROM ${INFERENCE_TABLE} GROUP BY model ORDER BY model`,
    [],
    z.object({ model: z.string(), tokens: z.bigint() }),
  );

  expect(rows).toEqual([
    { model: "haiku", tokens: 50n },
    { model: "opus", tokens: 400n },
  ]);
});

test("typed rows parse each row", async ({ reader }) => {
  const rows = await reader.sql(
    `SELECT model, latency_ms FROM ${INFERENCE_TABLE} ORDER BY call_id`,
    [],
    Inference.pick({ model: true, latency_ms: true }),
  );

  expect(rows).toEqual(INFERENCES.map(({ model, latency_ms }) => ({ model, latency_ms })));
});

test("row that does not fit fails the read", async ({ reader }) => {
  await expect(
    reader.sql(`SELECT model FROM ${INFERENCE_TABLE}`, [], z.object({ model: z.number() })),
  ).rejects.toBeInstanceOf(ZodError);
});

test("empty result keeps the selected schema", async ({ reader }) => {
  const empty = await reader.sql(`SELECT model, latency_ms FROM ${INFERENCE_TABLE} WHERE call_id = 9999`);

  for (const table of [empty.toArrow(), tableFromIPC(empty.toBytes())]) {
    expect(table.numRows).toBe(0);
    expect(table.schema.fields.map((field) => [field.name, String(field.type)])).toEqual([
      ["model", "Utf8"],
      ["latency_ms", "Float64"],
    ]);
  }
});

/** Service B's client acting on behalf of A's inbound access token. */
async function onBehalfOfA(a: string, b: string): Promise<Bifrost> {
  const token = await WyrdClient.connect({ credential: a }).accessToken();
  const delegated = await WyrdClient.connect({ credential: b }).onBehalfOf(token, { audience: "bifrost" });
  return Bifrost.connect({ client: delegated });
}

test("delegated client reads with the callers authority", async ({ reader: _, a, b }) => {
  const asA = await onBehalfOfA(a, b);

  expect((await asA.sql(`SELECT call_id FROM ${INFERENCE_TABLE}`)).numRows).toBe(INFERENCES.length);
});

test("delegated client cannot write with the services authority", async ({ a, b }) => {
  await registerValueTable("vala.datasets.delegated");
  const { arrowSchema } = await TableConfig.describe("vala.datasets.delegated", {
    client: WyrdClient.connect({ credential: b }),
  });
  const asA = await onBehalfOfA(a, b);

  await expect(asA.writeBatch("vala.datasets.delegated", valueBatch(arrowSchema, 41n))).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
  await (await Bifrost.connect({ client: WyrdClient.connect({ credential: b }) })).writeBatch(
    "vala.datasets.delegated",
    valueBatch(arrowSchema, 42n),
  );
});
