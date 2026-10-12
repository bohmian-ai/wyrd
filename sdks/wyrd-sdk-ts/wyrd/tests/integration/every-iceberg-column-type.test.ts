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
  TimeMillisecond,
  TimeNanosecond,
  TimeSecond,
  TimeUnit,
  TimestampMillisecond,
  TimestampSecond,
  Uint64,
  Utf8,
  tableFromIPC,
  type DataType,
  type Table,
} from "apache-arrow";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { columnValues, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

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

/** Both tables, registered, written, and published once, and what each reads back. */
interface Written {
  readonly all: Bifrost;
  readonly allTypes: Table;
  readonly narrowTypes: Table;
}

/**
 * Every test starts after {@link Written}, so the narrower-declaration story
 * finds `narrow_types` registered without reading the fixture.
 */
const test = serverTest().extend<{ written: Written }>({
  written: [
    async ({ server }, use) => {
      const all = await Bifrost.connect({
        table: TableConfig.fromArrow("vala.datasets.all_types", EVERY_TYPE.schema),
      });
      await all.register();
      await all.writeBatch("vala.datasets.all_types", EVERY_TYPE.batches[0]);
      const narrow = await Bifrost.connect({
        table: TableConfig.fromArrow("vala.datasets.narrow_types", NARROW.schema),
      });
      await narrow.register();
      await narrow.writeBatch("vala.datasets.narrow_types", NARROW.batches[0]);
      server.flushBifrost();
      const allTypes = (await all.sql("SELECT * FROM vala.datasets.all_types ORDER BY id")).toArrow();
      const narrowTypes = (await narrow.sql("SELECT * FROM vala.datasets.narrow_types ORDER BY int8")).toArrow();
      await use({ all, allTypes, narrowTypes });
    },
    { scope: "file", auto: true },
  ],
});

test.for(EVERY_TYPE.schema.names)(
  "every iceberg type round trips with nested nulls: %s",
  (column, { written: { allTypes } }) => {
    const written = EVERY_TYPE.getChild(column);

    expect(String(allTypes.getChild(column)?.type)).toBe(String(written?.type));
    expect(columnValues(allTypes.getChild(column))).toEqual(columnValues(written));
  },
);

test("a map column reads back as a plain object", async ({ written: { all } }) => {
  const rows = await all.sql(
    "SELECT counts FROM vala.datasets.all_types ORDER BY id",
    [],
    z.object({ counts: z.record(z.string(), z.bigint().nullable()).nullable() }),
  );

  expect(rows).toEqual([{ counts: { a: 1n, b: 2n } }, { counts: null }, { counts: { k: null } }, { counts: {} }]);
});

test("a 64-bit integer reads back as a bigint", async ({ written: { all } }) => {
  const rows = await all.sql(
    "SELECT int64 FROM vala.datasets.all_types ORDER BY id",
    [],
    z.object({ int64: z.bigint().nullable() }),
  );

  expect(rows).toEqual([{ int64: 7n }, { int64: null }, { int64: 0n }, { int64: 9223372036854775807n }]);
});

test.for(NARROW.schema.names)(
  "a narrower type reads back the same values: %s",
  (column, { written: { narrowTypes } }) => {
    expect(columnValues(narrowTypes.getChild(column))).toEqual(columnValues(NARROW.getChild(column)));
  },
);

test("a narrower declaration is the table it describes", async () => {
  const described = await Bifrost.connect({
    table: await TableConfig.describe("vala.datasets.narrow_types"),
  });

  await expect(described.register()).resolves.toBe("already_exists");
});

test.each([
  new DenseUnion([0], [new Field("i", new Int32(), true)]),
  new Duration(TimeUnit.SECOND),
  new Interval(IntervalUnit.MONTH_DAY_NANO),
  new Float16(),
  mapOf(new Float16()),
  new Uint64(),
  new DateMillisecond(),
  new TimeSecond(),
  new TimeMillisecond(),
  new TimeNanosecond(),
  new TimestampSecond(),
  new TimestampMillisecond(),
  mapOf(new Uint64()),
])("a type Bifrost cannot store is refused when declared: %s", (type) => {
  expect(() => TableConfig.fromArrow("vala.datasets.no_wire_form", oneColumn(type))).toThrow(
    expect.objectContaining({ code: UNSUPPORTED_TYPE }),
  );
});
