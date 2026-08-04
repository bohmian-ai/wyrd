import { startTestServer } from "@wyrd/testing";
import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
  IncompleteQueryStreamError,
  WyrdClient,
  WyrdError,
} from "@wyrd/sdk";

describe("Oracle query journey", () => {
  it("scrubs native gRPC connection diagnostics", async () => {
    const client = new WyrdClient(
      "http://127.0.0.1:1",
      "test-token",
      "://invalid-grpc-endpoint",
    );
    const batchId = new Uint8Array(16);
    batchId[6] = 0x70;
    batchId[8] = 0x80;

    await expect(
      client.bifrost.insertBatch(
        "vala.bifrost.events",
        batchId,
        new Uint8Array(),
      ),
    ).rejects.toMatchObject({
      code: "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
      status: 503,
      detail: "Bifrost ingest transport is unavailable",
      details: { transport: "grpc" },
    });
  }, 10_000);

  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = new WyrdClient(server.baseUrl, server.token, server.grpcUrl);
      const expected = [11, 22];
      const batchId = new Uint8Array(16);
      batchId[6] = 0x70;
      batchId[8] = 0x80;
      const ipc = tableToIPC(
        tableFromArrays({ value: [11n, 22n] }),
        "stream",
      );
      const ack = await client.bifrost.insertBatch(server.tableFqn, batchId, ipc);
      expect(Array.from(ack.batchId)).toEqual(Array.from(batchId));
      server.waitForBifrostPublication();
      const stream = await client.bifrost.query({
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

  it.each([
    ["schema", "failNextQueryAfterSchema"],
    ["batch", "failNextQueryAfterBatch"],
  ] as const)("fails closed on EOF after %s", async (_label, fault) => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [31, 32]);
      server[fault]();
      const stream = await new WyrdClient(server.baseUrl, server.token, server.grpcUrl).bifrost.query({
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
      const client = new WyrdClient(server.baseUrl, denied, server.grpcUrl);
      await expect(
        client.bifrost.query({ sql: `SELECT * FROM ${server.tableFqn}` }),
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
