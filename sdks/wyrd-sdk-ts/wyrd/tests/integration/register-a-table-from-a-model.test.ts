// Register a table from nested Zod models, write rows, and query inside them.
//
// Nested objects register as struct columns, queried by path:
// `prediction['feature_importance']['importance']`. An open record registers
// as a Variant column, queried with `->` and `->>`.
import { z } from "zod";
import { expect, vi } from "vitest";

import { Bifrost, TableConfig, WyrdClient } from "wyrd-ai";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const FeatureImportance = z.object({
  name: z.string(),
  importance: z.number(),
});

const Prediction = z.object({
  result: z.int(),
  probabilities: z.array(z.number()),
  feature_importance: FeatureImportance,
});

const ApiRequest = z.object({
  request_id: z.string(),
  outcome: z.int(),
  prediction: Prediction,
  attributes: z.record(z.string(), z.unknown()),
});

const REQUESTS: z.infer<typeof ApiRequest>[] = [
  {
    request_id: "req-1",
    outcome: 1,
    prediction: {
      result: 1,
      probabilities: [0.1, 0.9],
      feature_importance: { name: "tenure", importance: 0.72 },
    },
    attributes: { client: { region: "us-east" }, retries: 0 },
  },
  {
    request_id: "req-2",
    outcome: 0,
    prediction: {
      result: 0,
      probabilities: [0.8, 0.2],
      feature_importance: { name: "spend", importance: 0.31 },
    },
    attributes: { client: { region: "eu-west" }, retries: 2, coupon: null },
  },
];

// Arrow returns a 64-bit integer column as a `bigint`, so the read model
// widens the `z.int()` fields the declaring model registers as Int64.
const ApiRequestRow = ApiRequest.extend({
  outcome: z.bigint(),
  prediction: Prediction.extend({ result: z.bigint() }),
});
const REQUEST_ROWS = REQUESTS.map((request) => ({
  ...request,
  outcome: BigInt(request.outcome),
  prediction: { ...request.prediction, result: BigInt(request.prediction.result) },
}));

const Importance = z.object({ request_id: z.string(), importance: z.number() });
const Region = z.object({ request_id: z.string(), region: z.string() });

/** The `api_requests` table, registered from {@link ApiRequest} and holding {@link REQUESTS}. */
const test = serverTest().extend<{ apiRequests: Bifrost }>({
  apiRequests: [
    async ({ server }, use) => {
      const apiRequests = await Bifrost.connect({
        table: TableConfig.fromJsonSchema("vala.datasets.api_requests", ApiRequest),
      });
      await apiRequests.register();
      for (const request of REQUESTS) {
        apiRequests.insert(request);
      }
      await apiRequests.flush();
      server.flushBifrost();
      await use(apiRequests);
    },
    { scope: "file" },
  ],
});

test("api requests read back as the same models", async ({ apiRequests }) => {
  const rows = await apiRequests.sql(
    "SELECT request_id, outcome, prediction, attributes " + "FROM vala.datasets.api_requests ORDER BY request_id",
    [],
    ApiRequestRow,
  );

  expect(rows).toEqual(REQUEST_ROWS);
});

test("a nested model field is selected by its path", async ({ apiRequests }) => {
  const rows = await apiRequests.sql(
    "SELECT request_id, prediction['feature_importance']['importance'] AS importance " +
      "FROM vala.datasets.api_requests ORDER BY request_id",
    [],
    Importance,
  );

  expect(rows).toEqual([
    { request_id: "req-1", importance: 0.72 },
    { request_id: "req-2", importance: 0.31 },
  ]);
});

test("a nested model field filters rows", async ({ apiRequests }) => {
  const rows = await apiRequests.sql(
    "SELECT request_id, prediction['feature_importance']['importance'] AS importance " +
      "FROM vala.datasets.api_requests " +
      "WHERE prediction['feature_importance']['importance'] > 0.5",
    [],
    Importance,
  );

  expect(rows).toEqual([{ request_id: "req-1", importance: 0.72 }]);
});

test("an open dict field is queried by key", async ({ apiRequests }) => {
  const rows = await apiRequests.sql(
    "SELECT request_id, attributes -> 'client' ->> 'region' AS region " +
      "FROM vala.datasets.api_requests ORDER BY request_id",
    [],
    Region,
  );

  expect(rows).toEqual([
    { request_id: "req-1", region: "us-east" },
    { request_id: "req-2", region: "eu-west" },
  ]);
});

test("registering the same model again finds the existing table", async ({ apiRequests }) => {
  await expect(apiRequests.register()).resolves.toBe("already_exists");
});

const Scored = z.object({ request_id: z.string(), region: z.string() });
const Labelled = z.object({ request_id: z.string(), area: z.string(), note: z.string().nullable() });

test("table configuration preserves versions and values", async ({ server }) => {
  const scored = await Bifrost.connect({ table: TableConfig.fromJsonSchema("vala.datasets.scored", Scored) });
  await scored.register();
  scored.insert({ request_id: "req-1", region: "us-east" });
  await scored.flush();
  server.flushBifrost();
  const described = await scored.describeTable("vala.datasets", "scored");
  expect(described.table_version).toBe(1n);

  // Supplied layout lists name columns as they are after this edit's renames.
  const edited = await scored.alterTable("vala.datasets", "scored", {
    expected_table_version: described.table_version,
    physical_layout: { bloom_columns: ["area"] },
    add_columns: [{ name: "note", data_type: "Utf8", nullable: true }],
    rename_columns: [{ from: "region", to: "area" }],
  });

  expect(edited.table_version).toBe(2n);
  expect(edited.user_fields.map((field) => field.name)).toEqual(["request_id", "area", "note"]);
  expect(edited.physical_layout.bloom_columns).toEqual(["area"]);
  expect(edited.physical_layout.partition_by).toEqual(described.physical_layout.partition_by);
  await expect(scored.describeTable("vala.datasets", "scored")).resolves.toEqual(edited);
  await expect(scored.sql("SELECT request_id, area, note FROM vala.datasets.scored", [], Labelled)).resolves.toEqual([
    { request_id: "req-1", area: "us-east", note: null },
  ]);

  const cleared = await scored.alterTable("vala.datasets", "scored", {
    expected_table_version: edited.table_version,
    physical_layout: { bloom_columns: [] },
  });
  expect(cleared.table_version).toBe(3n);
  expect(cleared.physical_layout.bloom_columns).toEqual([]);
  expect(cleared.user_fields).toEqual(edited.user_fields);

  await expect(
    scored.alterTable("vala.datasets", "scored", {
      expected_table_version: 2n,
      physical_layout: { bloom_columns: ["area"] },
    }),
  ).rejects.toMatchObject({ code: "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT" });

  const viewer = await Bifrost.connect({
    client: WyrdClient.connect({ credential: server.bootstrapService(["viewer"], "table-viewer") }),
  });
  await expect(viewer.describeTable("vala.datasets", "scored")).resolves.toEqual(cleared);
  await expect(
    viewer.alterTable("vala.datasets", "scored", {
      expected_table_version: 3n,
      physical_layout: { bloom_columns: ["area"] },
    }),
  ).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });

  // A version is a positive signed 64-bit integer, sent exactly past 2^53.
  const edit = (version: bigint) =>
    scored.alterTable("vala.datasets", "scored", {
      expected_table_version: version,
      physical_layout: { bloom_columns: [] },
    });
  for (const version of [0n, -1n, 2n ** 63n]) {
    await expect(edit(version)).rejects.toMatchObject({ code: "WYRD_SPEC_400_VALIDATION" });
  }
  for (const version of [2n ** 53n + 1n, 2n ** 63n - 1n]) {
    await expect(edit(version)).rejects.toMatchObject({ code: "WYRD_VALA_409_BIFROST_TABLE_VERSION_CONFLICT" });
  }
  await expect(scored.describeTable("vala.datasets", "scored")).resolves.toEqual(cleared);
});
