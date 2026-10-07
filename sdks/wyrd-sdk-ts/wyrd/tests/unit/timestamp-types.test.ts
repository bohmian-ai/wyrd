// The three Wyrd timestamp types, and every Zod time type converted to one.
import { describe, expect, it } from "vitest";
import { z } from "zod";

import { TableConfig, TimestampLTZ, TimestampNTZ, TimestampTZ } from "@wyrd/sdk";

const NTZ = "Timestamp<MICROSECOND>";
const LTZ = "Timestamp<MICROSECOND, +00:00>";
const TZ = `Struct<{utc:${LTZ}, local:${NTZ}}>`;

describe("Wyrd timestamp types", () => {
  it("accept only their own text and return it canonically", () => {
    expect(TimestampNTZ.parse("2026-10-06T09:00:00")).toBe("2026-10-06T09:00:00");
    expect(TimestampLTZ.parse("2026-10-06T12:00:00-05:00")).toBe("2026-10-06T17:00:00Z");
    expect(TimestampTZ.parse("2026-10-06T12:00:00.000001-05:00")).toBe(
      "2026-10-06T12:00:00.000001-05:00",
    );
    expect(() => TimestampNTZ.parse("2026-10-06T12:00:00-05:00")).toThrow(
      'TIMESTAMP_NTZ does not accept "2026-10-06T12:00:00-05:00"',
    );
    expect(() => TimestampLTZ.parse("2026-10-06T12:00:00")).toThrow(TypeError);
    expect(() => TimestampTZ.parse("2026-10-06T12:00:00")).toThrow(TypeError);
  });

  it("declare each Wyrd and Zod time field as its Wyrd column type", () => {
    const order = z.object({
      store_opens: TimestampNTZ.schema(z),
      received_at: TimestampLTZ.schema(z),
      submitted_at: TimestampTZ.schema(z),
      utc: z.iso.datetime(),
      offset: z.iso.datetime({ offset: true }),
      local: z.iso.datetime({ local: true }),
    });

    const schema = TableConfig.fromJsonSchema("vala.datasets.orders", order).arrowSchema;

    expect(Object.fromEntries(schema.fields.map((field) => [field.name, String(field.type)]))).toEqual({
      store_opens: NTZ,
      received_at: LTZ,
      submitted_at: TZ,
      utc: LTZ,
      offset: LTZ,
      local: NTZ,
    });
  });

  it("give Zod fields that refuse text of another type", () => {
    const submitted = TimestampTZ.schema(z);

    expect(submitted.parse("2026-10-06T12:00:00-05:00")).toBe("2026-10-06T12:00:00-05:00");
    expect(submitted.safeParse("2026-10-06T12:00:00").success).toBe(false);
    expect(TimestampNTZ.schema(z).safeParse("2026-10-06T17:00:00Z").success).toBe(false);
  });
});
