import { tableFromIPC } from "apache-arrow";
import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, IncompleteQueryStreamError, WyrdError } from "@wyrd/sdk";

describe("Oracle query journey", () => {
  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const expected = [11, 22];
      server.seedBifrostRows(server.tableFqn, expected);
      server.waitForBifrostPublication();
      const stream = await client.stream({
        sql: `SELECT * FROM ${server.tableFqn}`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(2);
      expect(batches[0]?.schema.fields[0]?.name).toBe("value");
      expect(batches[0]?.schema.fields.length).toBeGreaterThan(1);
      const values = batches.flatMap((batch) =>
        Array.from(batch.getChildAt(0)?.toArray() ?? []),
      );
      expect(values).toEqual(expected.map(BigInt));
      expect(stream.terminal).toBeDefined();
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(2);
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("uses schema-once IPC with explicit EOS", async () => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [51, 52, 53]);
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const stream = await client.stream({
        sql: `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.length).toBeGreaterThan(0);
      // One query is one Arrow IPC stream, so every decoded batch carries the
      // query's single schema rather than one re-declared per batch.
      const fields = batches.map((batch) =>
        batch.schema.fields.map((field) => field.name).join(","),
      );
      expect(new Set(fields).size).toBe(1);
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(3);
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(3);
      // The terminal closes that stream explicitly; an empty delta would mean a
      // truncated result rather than a complete one.
      expect(stream.terminal?.arrow_ipc_eos).toEqual([255, 255, 255, 255, 0, 0, 0, 0]);
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("converts a collected result to Arrow and to IPC bytes", async () => {
    const server = startTestServer();
    try {
      const expected = [61, 62, 63];
      server.seedBifrostRows(server.tableFqn, expected);
      server.waitForBifrostPublication();
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

  it.each([
    ["schema", "failNextQueryAfterSchema"],
    ["batch", "failNextQueryAfterBatch"],
  ] as const)("fails closed on EOF after %s", async (_label, fault) => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [31, 32]);
      server[fault]();
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const stream = await client.stream({
        sql: `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      });
      await expect((async () => {
        for await (const _batch of stream) {
          // Drain until the deterministic fault closes the native stream.
        }
      })()).rejects.toBeInstanceOf(IncompleteQueryStreamError);
      await expect(stream.next()).resolves.toMatchObject({ done: true });
    } finally {
      server.shutdown();
    }
  });

  it("rejects an authenticated token before Oracle work", async () => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [41, 42]);
      const before = server.bifrostReadDecisionCount();
      const denied = server.queryDeniedToken();
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
      expect(server.bifrostReadDecisionCount()).toBe(before);
    } finally {
      server.shutdown();
    }
  });
});
