import { tableFromIPC } from "apache-arrow";
import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, WyrdError } from "@wyrd/sdk";

/**
 * Write `values` into the harness table through the public SDK and make them queryable.
 *
 * Each row correlates to the Card the writer principal is scoped to; the
 * client flush proves acceptance and the Scribe flush makes the rows readable.
 */
async function seed(server: NativeWyrdTestServer, values: readonly number[]): Promise<void> {
  const writer = await Bifrost.connect({
    serverUrl: server.baseUrl,
    credential: server.apiKey,
    grpcUrl: server.grpcUrl,
  });
  await writer.useTableByName(server.tableFqn);
  for (const value of values) {
    writer.insert({ value }, { cardRef: server.cardRef });
  }
  await writer.flush();
  await writer.shutdown();
  server.flushBifrost();
}

describe("Oracle query journey", () => {
  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      await seed(server, [11, 22, 33]);
      const expected = [11, 22, 33];
      const stream = await client.stream({
        sql: `SELECT * FROM ${server.tableFqn} ORDER BY value`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(3);
      expect(batches[0]?.schema.fields[0]?.name).toBe("value");
      expect(batches[0]?.schema.fields.length).toBeGreaterThan(1);
      const values = batches.flatMap((batch) =>
        Array.from(batch.getChildAt(0)?.toArray() ?? []),
      );
      expect(values).toEqual(expected.map(BigInt));
      expect(stream.terminal).toBeDefined();
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(3);
      expect(stream.terminal?.warnings).toEqual([]);
      expect(stream.terminal).not.toHaveProperty("freshness");
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("converts a collected result to Arrow and to IPC bytes", async () => {
    const server = startTestServer();
    try {
      const expected = [61, 62, 63];
      await seed(server, expected);
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const result = await client.sql(
        `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      );

      // Both conversions are views over the batches the result already holds,
      // so each must carry exactly the rows the result reports.
      const table = result.toArrow();
      expect(table.numRows).toBe(result.numRows);
      expect(Array.from(table.getChild("value")?.toArray() ?? [])).toEqual(
        expected.map(BigInt),
      );

      const decoded = tableFromIPC(result.toBytes());
      expect(decoded.numRows).toBe(result.numRows);
      expect(decoded.schema.fields.map((field) => field.name)).toEqual(
        table.schema.fields.map((field) => field.name),
      );
      expect(Array.from(decoded.getChild("value")?.toArray() ?? [])).toEqual(
        expected.map(BigInt),
      );
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("rejects an authenticated token before Oracle work", async () => {
    const server = startTestServer();
    try {
      await seed(server, [41, 42]);
      // A principal that may read Cards but not query Bifrost.
      const denied = server.scopedApiKey("ts_query_denied", ["cards:read"]);
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: denied,
        grpcUrl: server.grpcUrl,
      });
      await expect(
        client.stream({ sql: `SELECT * FROM ${server.tableFqn}` }),
      ).rejects.toMatchObject({
        code: "WYRD_PERMISSION_403_DENIED_RBAC",
        status: 403,
        title: "Permission denied (RBAC)",
        details: expect.any(Object),
      } satisfies Partial<WyrdError>);
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("rejects out-of-range deadlines with the shared validation error", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      for (const deadlineMs of [0, 4_294_967_296, 1.5, Number.NaN]) {
        await expect(
          client.stream({ sql: "SELECT 1", deadlineMs }),
        ).rejects.toMatchObject({
          code: "WYRD_VALA_400_QUERY_INVALID_SQL",
          status: 400,
        } satisfies Partial<WyrdError>);
      }
    } finally {
      server.shutdown();
    }
  }, 15_000);
});
