import { expect, vi } from "vitest";
import { z } from "zod";

import { WyrdClient, WyrdState } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { observedServiceTest as test } from "../support/observed-service.js";

vi.setConfig({ testTimeout: 120_000 });

const DriftRow = z.object({
  series: z.string(),
  num_value: z.number().nullable(),
  str_value: z.string().nullable(),
  card_uid: z.string(),
});

test("run observations read back by run id", async ({ server, reader, state }) => {
  await state.startBifrost();
  const run = state.run();

  run.forCard("model").observe.drift({ latency: 12.5, tier: "gold" });
  await state.shutdown();
  server.flushBifrost();

  const rows = await reader.sql(
    `SELECT series, num_value, str_value, card_uid
       FROM vala.drift.observations WHERE run_id = $1 ORDER BY series`,
    [run.runId],
    DriftRow,
  );
  const model = state.cardRef("model").uid;
  expect(rows).toEqual([
    { series: "latency", num_value: 12.5, str_value: "12.5", card_uid: model },
    { series: "tier", num_value: null, str_value: "gold", card_uid: model },
  ]);
});

test("run view exposes its alias", ({ state }) => {
  const run = state.run();
  const model = run.forCard("model");
  const agentRun = state.run("agent");

  expect([run.alias, model.alias, agentRun.alias]).toEqual(["root", "model", "agent"]);
  expect(model.runId).toBe(run.runId);
  expect(agentRun.runId).not.toBe(run.runId);
});

test("card scoped key cannot write another cards observations", async ({ bundle }) => {
  const agentKey = await cli.issueKey({ kind: "Agent", name: "observed-agent", version: "1.0.0", space: "default" });
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: agentKey.key }) });
  await state.startBifrost();

  state.run().forCard("model").observe.drift({ latency: 12.5 });

  await expect(state.flush()).rejects.toMatchObject({ code: "WYRD_VALA_403_BIFROST_CARD_SCOPE" });
  await state.shutdown();
});

test("unsealable byte budget is refused at connect", async ({ state }) => {
  await expect(state.startBifrost({ clientByteLimitBytes: 1024 })).rejects.toMatchObject({
    code: "WYRD_CLIENT_400_CONFIG_INVALID",
  });
});
