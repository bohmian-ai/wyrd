import type { Table } from "apache-arrow";
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

const INFERENCE_SCHEMA = {
  type: "object",
  properties: {
    call_id: { type: "integer" },
    model: { type: "string" },
    tokens: { type: "integer" },
    latency_ms: { type: "number" },
    status: { type: "string" },
  },
  required: ["call_id", "model", "tokens", "latency_ms", "status"],
} as const;

const MODEL_INFO_SCHEMA = {
  type: "object",
  properties: { model: { type: "string" }, vendor: { type: "string" } },
  required: ["model", "vendor"],
} as const;

const INFERENCES = [
  { call_id: 1, model: "opus", tokens: 100, latency_ms: 120.5, status: "ok" },
  { call_id: 2, model: "opus", tokens: 300, latency_ms: 240.0, status: "ok" },
  { call_id: 3, model: "opus", tokens: 200, latency_ms: 180.25, status: "error" },
  { call_id: 4, model: "haiku", tokens: 50, latency_ms: 30.0, status: "ok" },
  { call_id: 5, model: "haiku", tokens: 150, latency_ms: 60.75, status: "ok" },
];

const MODEL_INFO = [
  { model: "opus", vendor: "anthropic" },
  { model: "haiku", vendor: "anthropic" },
];

/**
 * Register a caller-owned table, write `rows`, and publish them for a reader.
 *
 * Rows are only queryable once the client drains and the server-owned Scribe
 * publishes, so both halves live here rather than in each caller.
 */
async function publishRows(
  server: ReturnType<typeof startTestServer>,
  fqn: string,
  schema: Parameters<typeof TableConfig.fromJsonSchema>[1],
  rows: readonly Record<string, unknown>[],
): Promise<void> {
  const bifrost = await connect(server, TableConfig.fromJsonSchema(fqn, schema));
  expect(await bifrost.register()).toBe("created");
  for (const row of rows) {
    bifrost.insert(row, { cardRef: server.cardRef });
  }
  await bifrost.flush();
  await bifrost.shutdown();
  server.flushBifrost();
}

/** Flatten one column of an Arrow table into a plain array, nulls preserved. */
function col(table: Table, name: string): unknown[] {
  return Array.from(table.getChild(name) ?? []);
}

describe("Bifrost analytical read journey", () => {
  it("runs group-by, window, join, and scalar SQL over written tables", async () => {
    const server = startTestServer();
    try {
      const suffix = Date.now().toString(36);
      const facts = `vala.datasets.inference_${suffix}`;
      const dims = `vala.datasets.model_info_${suffix}`;
      await publishRows(server, facts, INFERENCE_SCHEMA, INFERENCES);
      await publishRows(server, dims, MODEL_INFO_SCHEMA, MODEL_INFO);

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });

      // Aggregates with a HAVING filter: the per-group summary table.
      const grouped = (
        await reader.sql(
          `SELECT model,
                  CAST(COUNT(*) AS BIGINT) AS runs,
                  CAST(SUM(tokens) AS BIGINT) AS total_tokens,
                  CAST(AVG(tokens) AS DOUBLE) AS avg_tokens,
                  CAST(MIN(latency_ms) AS DOUBLE) AS fastest,
                  CAST(MAX(latency_ms) AS DOUBLE) AS slowest
           FROM ${facts}
           GROUP BY model HAVING COUNT(*) > 1 ORDER BY total_tokens DESC`,
        )
      ).toArrow();
      expect(col(grouped, "model")).toEqual(["opus", "haiku"]);
      expect(col(grouped, "runs")).toEqual([3n, 2n]);
      expect(col(grouped, "total_tokens")).toEqual([600n, 200n]);
      expect(col(grouped, "avg_tokens")).toEqual([200, 100]);
      expect(col(grouped, "fastest")).toEqual([120.5, 30]);
      expect(col(grouped, "slowest")).toEqual([240, 60.75]);

      // Window functions: rank within a partition, a running total, and a lag.
      const windowed = (
        await reader.sql(
          `SELECT call_id,
                  CAST(ROW_NUMBER() OVER (PARTITION BY model ORDER BY tokens DESC) AS BIGINT) AS rank_in_model,
                  CAST(SUM(tokens) OVER (PARTITION BY model ORDER BY call_id) AS BIGINT) AS running_tokens,
                  CAST(LAG(tokens) OVER (PARTITION BY model ORDER BY call_id) AS BIGINT) AS prev_tokens
           FROM ${facts} ORDER BY call_id`,
        )
      ).toArrow();
      expect(col(windowed, "rank_in_model")).toEqual([3n, 1n, 2n, 2n, 1n]);
      expect(col(windowed, "running_tokens")).toEqual([100n, 400n, 600n, 50n, 200n]);
      expect(col(windowed, "prev_tokens")).toEqual([null, 100n, 300n, null, 50n]);

      // Join to the dimension table, with a filtering aggregate on the facts.
      const joined = (
        await reader.sql(
          `SELECT d.vendor, f.model,
                  CAST(COUNT(*) FILTER (WHERE f.status = 'ok') AS BIGINT) AS successes,
                  CAST(COUNT(*) AS BIGINT) AS attempts
           FROM ${facts} AS f INNER JOIN ${dims} AS d ON f.model = d.model
           GROUP BY d.vendor, f.model ORDER BY f.model`,
        )
      ).toArrow();
      expect(col(joined, "vendor")).toEqual(["anthropic", "anthropic"]);
      expect(col(joined, "model")).toEqual(["haiku", "opus"]);
      expect(col(joined, "successes")).toEqual([2n, 2n]);
      expect(col(joined, "attempts")).toEqual([2n, 3n]);

      // Scalar expressions over a CTE: the reshaping step before a chart.
      const scalars = (
        await reader.sql(
          `WITH labelled AS (
             SELECT call_id, UPPER(model) AS model_label,
                    CAST(ROUND(latency_ms) AS DOUBLE) AS latency_whole,
                    CASE WHEN latency_ms > 100 THEN 'slow' ELSE 'fast' END AS bucket,
                    CAST(CHARACTER_LENGTH(status) AS BIGINT) AS status_len
             FROM ${facts}
           ) SELECT * FROM labelled ORDER BY call_id LIMIT 3`,
        )
      ).toArrow();
      expect(scalars.numRows).toBe(3);
      expect(col(scalars, "model_label")).toEqual(["OPUS", "OPUS", "OPUS"]);
      expect(col(scalars, "latency_whole")).toEqual([121, 240, 180]);
      expect(col(scalars, "bucket")).toEqual(["slow", "slow", "slow"]);
      expect(col(scalars, "status_len")).toEqual([2n, 2n, 5n]);
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
