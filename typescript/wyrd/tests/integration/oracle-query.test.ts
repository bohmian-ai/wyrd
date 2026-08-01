import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { WyrdClient } from "@wyrd/sdk";

describe("Oracle query journey", () => {
  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = new WyrdClient(server.baseUrl, server.token);
      const stream = await client.bifrost.query({
        sql: `SELECT * FROM ${server.tableFqn}`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(0);
      expect(stream.terminal?.outcome).not.toBe("failed");
    } finally {
      server.shutdown();
    }
  });
});
