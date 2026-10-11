// Register a user table on its own time column and read a time range back.
//
// A table declares `timeColumn`; the column partitions the table and bounds its
// files, so a time-range read returns exactly the rows inside the range.
// `describeTable` reports the declared column, a table declared without one
// reports `wyrd_ingested_at`, and a declaration naming a column that cannot be
// a time column is refused naming the field.
import { Field, Int64, RecordBatch, Schema, Struct, TimestampMicrosecond, makeData } from "apache-arrow";
import { z } from "zod";
import { expect } from "vitest";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

const NAMESPACE = "vala.datasets";
const READINGS = "ts_time_column_readings";
const DEFAULTS = "ts_time_column_defaults";
const REFUSED = "ts_time_column_refused";
/** One hour in microseconds. */
const HOUR = 3_600_000_000n;
/** 2026-10-06T00:00:00Z in microseconds, the first written hour. */
const FIRST_HOUR = 1_791_244_800_000_000n;
const SCHEMA = new Schema([
  new Field("event_time", new TimestampMicrosecond("UTC"), false),
  new Field("value", new Int64(), false),
]);
const Value = z.object({ value: z.number() });

/** One reading in each of four consecutive hours; the value is the hour index. */
function readings(): RecordBatch {
  const hours = [0n, 1n, 2n, 3n];
  const [eventTime, value] = SCHEMA.fields;
  return new RecordBatch(
    SCHEMA,
    makeData({
      type: new Struct(SCHEMA.fields),
      length: hours.length,
      nullCount: 0,
      children: [
        makeData({ type: eventTime.type, data: BigInt64Array.from(hours, (hour) => FIRST_HOUR + hour * HOUR + 1n) }),
        makeData({ type: value.type, data: BigInt64Array.from(hours) }),
      ],
    }),
  );
}

/** A writer for `name` on hourly partitions of `timeColumn`. */
function table(name: string, timeColumn?: string): Promise<Bifrost> {
  return Bifrost.connect({
    table: TableConfig.fromArrow(`${NAMESPACE}.${name}`, SCHEMA, {
      layout: { partitionGranularity: "hour", timeColumn },
    }),
  });
}

const test = serverTest();

test("a user table's time column prunes a time range", async ({ server }) => {
  const readingsTable = await table(READINGS, "event_time");
  await readingsTable.register();
  await readingsTable.writeBatch(`${NAMESPACE}.${READINGS}`, readings());
  await readingsTable.flush();
  server.flushBifrost();
  const defaults = await table(DEFAULTS);
  await defaults.register();

  const inRange = await readingsTable.sql(
    `SELECT CAST(value AS INT) AS value FROM ${NAMESPACE}.${READINGS} ` +
      "WHERE event_time >= TIMESTAMP '2026-10-06T01:00:00Z' " +
      "AND event_time < TIMESTAMP '2026-10-06T03:00:00Z' ORDER BY value",
    [],
    Value,
  );

  expect(inRange).toEqual([{ value: 1 }, { value: 2 }]);
  expect((await readingsTable.describeTable(NAMESPACE, READINGS)).physical_layout.time_column).toBe("event_time");
  expect((await defaults.describeTable(NAMESPACE, DEFAULTS)).physical_layout.time_column).toBe("wyrd_ingested_at");
}, 60_000);

test("an invalid time column is refused naming the field", async () => {
  const refused = await table(REFUSED, "value");

  await expect(refused.register()).rejects.toMatchObject({
    code: "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT",
    message: expect.stringContaining("TimeColumn"),
  });
});
