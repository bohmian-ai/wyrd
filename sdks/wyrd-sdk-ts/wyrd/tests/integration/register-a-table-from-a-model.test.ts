// Register a table from nested Zod models, write rows, and query inside them.
//
// Nested objects register as struct columns, queried by path:
// `prediction['feature_importance']['importance']`. An open record registers
// as a Variant column, queried with `->` and `->>`.
import { z } from "zod";
import { beforeAll, describe, expect, it } from "vitest";

import { Bifrost, TableConfig } from "@wyrd/sdk";

import { useTestServer } from "../support/test-server.js";

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

const Importance = z.object({ request_id: z.string(), importance: z.number() });
const Region = z.object({ request_id: z.string(), region: z.string() });

describe("register a table from a model", () => {
  const server = useTestServer();
  let apiRequests: Bifrost;

  beforeAll(async () => {
    apiRequests = await Bifrost.connect({
      table: TableConfig.fromJsonSchema("vala.datasets.api_requests", ApiRequest),
    });
    await apiRequests.register();
    for (const request of REQUESTS) {
      apiRequests.insert(request);
    }
    await apiRequests.flush();
    server().flushBifrost();
  }, 60_000);

  it("api requests read back as the same models", async () => {
    const rows = await apiRequests.sql(
      "SELECT request_id, outcome, prediction, attributes " +
        "FROM vala.datasets.api_requests ORDER BY request_id",
      ApiRequest,
    );

    expect(rows).toEqual(REQUESTS);
  });

  it("a nested model field is selected by its path", async () => {
    const rows = await apiRequests.sql(
      "SELECT request_id, prediction['feature_importance']['importance'] AS importance " +
        "FROM vala.datasets.api_requests ORDER BY request_id",
      Importance,
    );

    expect(rows).toEqual([
      { request_id: "req-1", importance: 0.72 },
      { request_id: "req-2", importance: 0.31 },
    ]);
  });

  it("a nested model field filters rows", async () => {
    const rows = await apiRequests.sql(
      "SELECT request_id, prediction['feature_importance']['importance'] AS importance " +
        "FROM vala.datasets.api_requests " +
        "WHERE prediction['feature_importance']['importance'] > 0.5",
      Importance,
    );

    expect(rows).toEqual([{ request_id: "req-1", importance: 0.72 }]);
  });

  it("an open dict field is queried by key", async () => {
    const rows = await apiRequests.sql(
      "SELECT request_id, attributes -> 'client' ->> 'region' AS region " +
        "FROM vala.datasets.api_requests ORDER BY request_id",
      Region,
    );

    expect(rows).toEqual([
      { request_id: "req-1", region: "us-east" },
      { request_id: "req-2", region: "eu-west" },
    ]);
  });

  it("registering the same model again finds the existing table", async () => {
    await expect(apiRequests.register()).resolves.toBe("already_exists");
  });
});
