import { expect, test } from "vitest";

import { Bifrost, WyrdClient } from "wyrd-ai";

type NativeWyrdClient = Parameters<typeof WyrdClient.fromNative>[0];

// Internal: a fake native client whose Bifrost handle answers one wire
// description, so versions no real table reaches still prove the exact
// `bigint` projection.
test.each(["1", "9007199254740993", "9223372036854775807"])(
  "a described table version %s projects as an exact bigint",
  async (version) => {
    const bifrost = {
      async describeTable() {
        return { valueJson: JSON.stringify({ table_version: version }) };
      },
    };
    const client = WyrdClient.fromNative({
      async connectBifrost() {
        return { bifrost, error: null };
      },
    } as unknown as NativeWyrdClient);

    const described = await (await Bifrost.connect({ client })).describeTable("bifrost.datasets", "events");

    expect(typeof described.table_version).toBe("bigint");
    expect(described.table_version).toBe(BigInt(version));
  },
);
