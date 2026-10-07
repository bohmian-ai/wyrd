// Declare every Iceberg column type from an Arrow schema, write it, and read it back.
//
// `TableConfig.fromArrow` declares the types JSON Schema cannot, such as
// `Decimal`, `Int32`, maps, and nanosecond timestamps. Types with no Iceberg
// column are refused before any table exists. Every SDK reads the same
// `fixtures/bifrost` batches, written by `fixtures/bifrost/every_iceberg_type.py`.
import { readFileSync } from "node:fs";

import {
  DateMillisecond,
  DenseUnion,
  Duration,
  Field,
  Float16,
  Int32,
  Interval,
  IntervalUnit,
  Map_,
  Schema,
  Struct,
  TimeUnit,
  Uint64,
  Utf8,
  tableFromIPC,
  type DataType,
  type Table,
} from "apache-arrow";
import { beforeAll, describe, expect, it } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { columnValues, useTestServer } from "../support/test-server.js";

const FIXTURES = new URL("../../../../../fixtures/bifrost/", import.meta.url);
const EVERY_TYPE = tableFromIPC(readFileSync(new URL("every_iceberg_type.arrow", FIXTURES)));
const NARROW = tableFromIPC(readFileSync(new URL("narrow_types.arrow", FIXTURES)));
const UNSUPPORTED_TYPE = "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE";

/** A one-column schema holding `type`. */
const oneColumn = (type: DataType) => new Schema([new Field("c", type, true)]);

/** A map of string keys to `value` entries. */
const mapOf = (value: DataType) =>
  new Map_<Utf8, DataType>(
    new Field(
      "entries",
      new Struct<{ key: Utf8; value: DataType }>([
        new Field("key", new Utf8(), false),
        new Field("value", value, true),
      ]),
      false,
    ),
  );

describe("every iceberg column type", () => {
  const server = useTestServer();
  let all: Bifrost;
  let allTypes: Table;
  let narrowTypes: Table;

  beforeAll(async () => {
    all = await Bifrost.connect({
      table: TableConfig.fromArrow("vala.datasets.all_types", EVERY_TYPE.schema),
    });
    await all.register();
    await all.writeBatch("vala.datasets.all_types", EVERY_TYPE.batches[0]!);
    const narrow = await Bifrost.connect({
      table: TableConfig.fromArrow("vala.datasets.narrow_types", NARROW.schema),
    });
    await narrow.register();
    await narrow.writeBatch("vala.datasets.narrow_types", NARROW.batches[0]!);
    server().flushBifrost();
    allTypes = (await all.sql("SELECT * FROM vala.datasets.all_types ORDER BY id")).toArrow();
    narrowTypes = (
      await narrow.sql("SELECT * FROM vala.datasets.narrow_types ORDER BY int8")
    ).toArrow();
  }, 60_000);

  it.each(EVERY_TYPE.schema.names)(
    "every iceberg type round trips with nested nulls: %s",
    (column) => {
      const written = EVERY_TYPE.getChild(column);

      expect(String(allTypes.getChild(column)?.type)).toBe(String(written?.type));
      expect(columnValues(allTypes.getChild(column))).toEqual(columnValues(written));
    },
  );

  it("a map column reads back as a plain object", async () => {
    const rows = await all.sql(
      "SELECT counts FROM vala.datasets.all_types ORDER BY id",
      z.object({ counts: z.record(z.string(), z.number().nullable()).nullable() }),
    );

    expect(rows).toEqual([
      { counts: { a: 1, b: 2 } },
      { counts: null },
      { counts: { k: null } },
      { counts: {} },
    ]);
  });

  it("a 64-bit integer reads back as a number when it is safe", async () => {
    const rows = await all.sql(
      "SELECT int64 FROM vala.datasets.all_types ORDER BY id",
      z.object({ int64: z.union([z.number(), z.bigint()]).nullable() }),
    );

    expect(rows).toEqual([
      { int64: 7 },
      { int64: null },
      { int64: 0 },
      { int64: 9223372036854775807n },
    ]);
  });

  it.each(NARROW.schema.names)("a narrower type reads back the same values: %s", (column) => {
    expect(columnValues(narrowTypes.getChild(column))).toEqual(
      columnValues(NARROW.getChild(column)),
    );
  });

  it("a narrower declaration is the table it describes", async () => {
    const described = await Bifrost.connect({
      table: await TableConfig.describe("vala.datasets.narrow_types"),
    });

    await expect(described.register()).resolves.toBe("already_exists");
  });

  it.each([
    new DenseUnion([0], [new Field("i", new Int32(), true)]),
    new Duration(TimeUnit.SECOND),
    new Interval(IntervalUnit.MONTH_DAY_NANO),
    new Float16(),
    mapOf(new Float16()),
  ])("a type with no wire form is refused when declared: %s", (type) => {
    expect(() => TableConfig.fromArrow("vala.datasets.no_wire_form", oneColumn(type))).toThrow(
      expect.objectContaining({ code: UNSUPPORTED_TYPE }),
    );
  });

  it.each([new Uint64(), new DateMillisecond(), mapOf(new Uint64())])(
    "a type with no iceberg column is refused at registration: %s",
    async (type) => {
      const refused = await Bifrost.connect({
        table: TableConfig.fromArrow("vala.datasets.no_column", oneColumn(type)),
      });

      await expect(refused.register()).rejects.toMatchObject({ code: UNSUPPORTED_TYPE });
    },
  );

  it("a refused registration creates no table", async () => {
    const refused = await Bifrost.connect({
      table: TableConfig.fromArrow("vala.datasets.never", oneColumn(new Uint64())),
    });
    await expect(refused.register()).rejects.toMatchObject({ code: UNSUPPORTED_TYPE });

    await expect(TableConfig.describe("vala.datasets.never")).rejects.toMatchObject({
      code: "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
    });
  });
});
