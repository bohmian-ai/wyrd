import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, WyrdClient } from "@wyrd/sdk";

const SCHEMA = {
  type: "object",
  properties: { value: { type: "integer" } },
  required: ["value"],
} as const;

describe("Bifrost write journey", () => {
  it("writes, flushes, shuts down, and reads back through the public SDK", async () => {
    const server = startTestServer();
    try {
      const bifrost = await Bifrost.connect(
        server.baseUrl,
        server.apiKey,
        server.grpcUrl,
      );
      const written = [71, 72, 73];
      for (const value of written) {
        bifrost.insert({
          table: server.tableFqn,
          schema: SCHEMA,
          row: { value },
          cardRef: server.cardRef,
        });
      }
      // Batching means nothing is durable until the drain is acknowledged.
      expect(bifrost.producerCount).toBe(1);
      await bifrost.flush();
      await bifrost.shutdown();
      server.flushBifrost();

      const stream = await new WyrdClient(
        server.baseUrl,
        server.token,
        server.grpcUrl,
      ).bifrost.query({
        sql: `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      });
      const values = [];
      for await (const batch of stream) {
        values.push(...Array.from(batch.getChildAt(0)?.toArray() ?? []));
      }
      expect(values).toEqual(written.map(BigInt));
      expect(stream.terminal?.outcome).toBe("success");
    } finally {
      server.shutdown();
    }
  }, 30_000);

  it("counts observe drops instead of throwing, and rejects a bad card reference", async () => {
    const server = startTestServer();
    try {
      const bifrost = await Bifrost.connect(
        server.baseUrl,
        server.apiKey,
        server.grpcUrl,
      );
      bifrost.record({
        table: server.tableFqn,
        schema: SCHEMA,
        row: { value: 81 },
        cardRef: server.cardRef,
      });
      // The transport drains immediately here, so a drop would be a real loss
      // rather than backpressure the observe path is entitled to swallow.
      expect(bifrost.dropped).toBe(0);
      expect(() =>
        bifrost.insert({
          table: server.tableFqn,
          schema: SCHEMA,
          row: { value: 82 },
          cardRef: "not-a-ref",
        }),
      ).toThrow(/invalid cardRef/);
      await bifrost.shutdown();
    } finally {
      server.shutdown();
    }
  }, 30_000);
});
