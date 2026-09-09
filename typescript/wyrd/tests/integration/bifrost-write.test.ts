import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, TableConfig } from "@wyrd/sdk";

const SCHEMA = {
  type: "object",
  properties: { value: { type: "integer" } },
  required: ["value"],
} as const;

function connect(server: ReturnType<typeof startTestServer>, table?: TableConfig) {
  return Bifrost.connect({
    table,
    serverUrl: server.baseUrl,
    credential: server.apiKey,
    grpcUrl: server.grpcUrl,
  });
}

describe("Bifrost write journey", () => {
  it("registers, writes, flushes, swaps tables, and reads back", async () => {
    const server = startTestServer();
    try {
      // `vala.datasets` is the caller-owned namespace, so this is the one
      // table a user actually registers; the built-ins are server-owned.
      const ownFqn = `vala.datasets.journey_${Date.now().toString(36)}`;
      const bifrost = await connect(server, TableConfig.fromJsonSchema(ownFqn, SCHEMA));

      expect(await bifrost.register()).toBe("created");
      // Re-registering the same columns is a match, not a conflict.
      expect(await bifrost.register()).toBe("already_exists");
      expect(bifrost.table?.resolved?.tableUid).toBeTruthy();
      expect(bifrost.table?.arrowSchema.fields.map((f) => f.name)).toEqual(["value"]);

      const written = [71, 72, 73];
      for (const value of written) {
        bifrost.insert({ value }, { cardRef: server.cardRef });
      }
      expect(bifrost.producerCount).toBe(1);

      // Swap before flushing: the swapped-away producer must still drain, so
      // the rows above are not stranded by the rebinding.
      await bifrost.useTableByName(server.tableFqn);
      expect(bifrost.table?.fqn).toBe(server.tableFqn);
      bifrost.insert({ value: 91 }, { cardRef: server.cardRef });
      expect(bifrost.producerCount).toBe(2);

      await bifrost.flush();
      await bifrost.shutdown();
      server.flushBifrost();

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      const result = await reader.sql(`SELECT value FROM ${ownFqn} ORDER BY value`);
      const values = result.batches.flatMap((batch) =>
        Array.from(batch.getChildAt(0)?.toArray() ?? []),
      );
      expect(values).toEqual(written.map(BigInt));
      expect(result.terminal.outcome).toBe("success");
      expect(result.numRows).toBe(written.length);

      const swapped = await reader.sql(
        `SELECT value FROM ${server.tableFqn} WHERE value = 91`,
      );
      expect(swapped.numRows).toBe(1);
    } finally {
      server.shutdown();
    }
  }, 30_000);

  it("refuses a write with no active table and a bad card reference", async () => {
    const server = startTestServer();
    try {
      const bifrost = await connect(server);
      expect(() => bifrost.insert({ value: 81 })).toThrow(
        expect.objectContaining({ code: "WYRD_VALA_412_NO_ACTIVE_TABLE" }),
      );
      expect(bifrost.producerCount).toBe(0);

      bifrost.useTable(TableConfig.fromJsonSchema(server.tableFqn, SCHEMA));
      expect(() =>
        bifrost.insert({ value: 82 }, { cardRef: "not-a-ref" }),
      ).toThrow(/invalid cardRef/);
      await bifrost.shutdown();
    } finally {
      server.shutdown();
    }
  }, 30_000);

  it("describes an existing table without restating its schema", async () => {
    const server = startTestServer();
    try {
      const described = await TableConfig.describe(server.tableFqn, {
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      expect(described.fqn).toBe(server.tableFqn);
      expect(described.resolved).toBeDefined();

      const bifrost = await connect(server, described);
      bifrost.insert({ value: 91 }, { cardRef: server.cardRef });
      await bifrost.flush();
      await bifrost.shutdown();
      server.flushBifrost();

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const result = await reader.sql(
        `SELECT value FROM ${server.tableFqn} WHERE value = 91`,
      );
      expect(result.numRows).toBe(1);
    } finally {
      server.shutdown();
    }
  }, 30_000);
});
