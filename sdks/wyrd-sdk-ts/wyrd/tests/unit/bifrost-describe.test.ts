import { expect, test } from "vitest";

import { Bifrost } from "wyrd-ai";

type NativeBifrost = Parameters<typeof Bifrost.fromNative>[0];

// Internal: a fake native handle answering one wire description, so versions
// no real table reaches still prove the exact `bigint` projection.
test.each(["1", "9007199254740993", "9223372036854775807"])(
  "a described table version %s projects as an exact bigint",
  async (version) => {
    const native = {
      async describeTable() {
        return { valueJson: JSON.stringify({ table_version: version }) };
      },
    } as unknown as NativeBifrost;

    const described = await Bifrost.fromNative(native).describeTable("vala.datasets", "events");

    expect(typeof described.table_version).toBe("bigint");
    expect(described.table_version).toBe(BigInt(version));
  },
);
