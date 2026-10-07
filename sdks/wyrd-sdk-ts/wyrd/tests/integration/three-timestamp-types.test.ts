// Record orders with the three Wyrd timestamp types and read them back.
//
// An order has a store opening time (`TIMESTAMP_NTZ`, a wall-clock reading),
// the moment the server received it (`TIMESTAMP_LTZ`, one instant), and the
// moment the customer submitted it in their own zone (`TIMESTAMP_TZ`, the
// instant plus the customer's wall clock). A table declared with Wyrd types and
// one declared with Zod's own time types both go through the Wyrd types: each
// reads back as Wyrd timestamp text, and the customer's local hour is queryable.
import { RecordBatch, TimestampMicrosecond, makeVector } from "apache-arrow";
import { z } from "zod";
import { beforeAll, describe, expect, it } from "vitest";

import { Bifrost, TableConfig, TimestampLTZ, TimestampNTZ, TimestampTZ } from "@wyrd/sdk";

import { useTestServer } from "../support/test-server.js";

const WYRD_ORDERS = "vala.datasets.wyrd_orders";
const ZOD_ORDERS = "vala.datasets.zod_orders";
const ARROW_ORDERS = "vala.datasets.arrow_orders";
const SCHEMA_PARSE = "WYRD_VALA_400_SCHEMA_PARSE";

/** 2026-10-06T17:00:00Z in microseconds. */
const RECEIVED = 1_791_306_000_000_000n;
const SUBMITTED = { chicago: "2026-10-06T12:00:00-05:00", tokyo: "2026-10-07T02:00:00+09:00" };

const WyrdOrder = z.object({
  order_id: z.string(),
  store_opens: TimestampNTZ.schema(z),
  received_at: TimestampLTZ.schema(z),
  submitted_at: TimestampTZ.schema(z),
});

const ZodOrder = z.object({
  order_id: z.string(),
  store_opens: z.iso.datetime({ local: true }),
  received_at: z.iso.datetime({ offset: true }),
});

const LocalHour = z.object({ order_id: z.string(), local_hour: z.number(), utc_hour: z.number() });
const Received = z.object({ received_at: z.iso.datetime() });

/** A one-column batch of the received instant labelled with `zone`, or naive. */
function received(zone: string | null): RecordBatch {
  const type = new TimestampMicrosecond(zone);
  return new RecordBatch({ received_at: makeVector({ type, data: new BigInt64Array([RECEIVED]) }).data[0]! });
}

describe("three timestamp types", () => {
  const server = useTestServer();
  let wyrdOrders: Bifrost;
  let zodOrders: Bifrost;

  /** Register `table` from `model`, write `rows`, and publish them. */
  async function orders(
    table: string,
    model: z.ZodObject,
    rows: Record<string, string>[],
  ): Promise<Bifrost> {
    const bifrost = await Bifrost.connect({ table: TableConfig.fromJsonSchema(table, model) });
    await bifrost.register();
    for (const row of rows) {
      bifrost.insert(row);
    }
    await bifrost.flush();
    server().flushBifrost();
    return bifrost;
  }

  beforeAll(async () => {
    wyrdOrders = await orders(
      WYRD_ORDERS,
      WyrdOrder,
      Object.entries(SUBMITTED).map(([customer, submitted]) => ({
        order_id: customer,
        store_opens: TimestampNTZ.parse("2026-10-06T09:00:00"),
        received_at: TimestampLTZ.parse("2026-10-06T17:00:00Z"),
        submitted_at: TimestampTZ.parse(submitted),
      })),
    );
    zodOrders = await orders(
      ZOD_ORDERS,
      ZodOrder,
      Object.entries(SUBMITTED).map(([customer, submitted]) => ({
        order_id: customer,
        store_opens: "2026-10-06T09:00:00",
        received_at: submitted,
      })),
    );
  }, 60_000);

  it("wyrd timestamps read back as wyrd timestamp text", async () => {
    const rows = await wyrdOrders.sql(`SELECT * FROM ${WYRD_ORDERS} ORDER BY order_id`, WyrdOrder);

    expect(rows).toEqual(
      Object.entries(SUBMITTED).map(([customer, submitted]) => ({
        order_id: customer,
        store_opens: "2026-10-06T09:00:00",
        received_at: "2026-10-06T17:00:00Z",
        submitted_at: submitted,
      })),
    );
  });

  it("the customer's local hour is queryable", async () => {
    const rows = await wyrdOrders.sql(
      "SELECT order_id, CAST(date_part('hour', submitted_at['local']) AS INT) AS local_hour, " +
        `CAST(date_part('hour', submitted_at['utc']) AS INT) AS utc_hour FROM ${WYRD_ORDERS} ORDER BY order_id`,
      LocalHour,
    );

    expect(rows).toEqual([
      { order_id: "chicago", local_hour: 12, utc_hour: 17 },
      { order_id: "tokyo", local_hour: 2, utc_hour: 17 },
    ]);
  });

  it("zod timestamps go through wyrd types", async () => {
    const { arrowSchema } = await TableConfig.describe(ZOD_ORDERS);
    const columns = Object.fromEntries(arrowSchema.fields.map((field) => [field.name, String(field.type)]));
    const rows = await zodOrders.sql(`SELECT * FROM ${ZOD_ORDERS} ORDER BY order_id`, ZodOrder);

    expect(columns.store_opens).toBe("Timestamp<MICROSECOND>");
    expect(columns.received_at).toBe("Timestamp<MICROSECOND, +00:00>");
    expect(rows).toEqual([
      { order_id: "chicago", store_opens: "2026-10-06T09:00:00", received_at: "2026-10-06T17:00:00Z" },
      { order_id: "tokyo", store_opens: "2026-10-06T09:00:00", received_at: "2026-10-06T17:00:00Z" },
    ]);
  });

  it("a reading without a zone is refused for an instant", () => {
    expect(() =>
      zodOrders.insert({ order_id: "naive", store_opens: "2026-10-06T09:00:00", received_at: "2026-10-06T17:00:00" }),
    ).toThrow(expect.objectContaining({ code: SCHEMA_PARSE }));
  });

  it("an instant is refused for a wall-clock column", () => {
    expect(() =>
      zodOrders.insert({
        order_id: "zoned",
        store_opens: "2026-10-06T09:00:00-05:00",
        received_at: "2026-10-06T17:00:00Z",
      }),
    ).toThrow(expect.objectContaining({ code: SCHEMA_PARSE }));
  });

  it("an arrow instant in any zone is one instant and a naive one is refused", async () => {
    const arrow = await Bifrost.connect({
      table: TableConfig.fromArrow(ARROW_ORDERS, received("UTC").schema),
    });
    await arrow.register();
    for (const zone of ["UTC", "America/Chicago", "Asia/Tokyo"]) {
      await arrow.writeBatch(ARROW_ORDERS, received(zone));
    }
    server().flushBifrost();

    const read = await arrow.sql(`SELECT received_at FROM ${ARROW_ORDERS}`);
    const rows = await arrow.sql(`SELECT received_at FROM ${ARROW_ORDERS}`, Received);

    expect(String(read.schema.fields[0]!.type)).toBe("Timestamp<MICROSECOND, +00:00>");
    expect(rows).toEqual(Array(3).fill({ received_at: "2026-10-06T17:00:00Z" }));
    await expect(arrow.writeBatch(ARROW_ORDERS, received(null))).rejects.toMatchObject({
      code: "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
    });
  });
});
