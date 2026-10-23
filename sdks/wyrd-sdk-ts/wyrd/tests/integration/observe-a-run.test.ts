import { expect, vi } from "vitest";
import { z } from "zod";

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Cards, WyrdClient, WyrdState } from "wyrd-ai";

import { observedService } from "../support/observed-service.js";
import { binding, fixture, runtimeAccount, scheduledResult } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

const DriftRow = z.object({
  series: z.string(),
  num_value: z.number().nullable(),
  str_value: z.string().nullable(),
  session_id: z.string().nullable(),
  card_uid: z.string(),
  run_id: z.string(),
});

/**
 * The observed Service on one file-scoped server that also runs scheduled
 * Verifiers; vitest shares file-scoped fixtures by name, so the file uses one.
 */
const test = observedService({ verificationRuntime: true });

test("run observations read back by run id", async ({ server, reader, state }) => {
  await state.startBifrost();
  const run = state.run();

  expect(await run.forCard("model").observe.submit("observed-drift", { latency: 12.5 })).toEqual({ kind: "recorded" });
  await state.shutdown();
  server.flushBifrost();

  const rows = await reader.sql(
    `SELECT series, num_value, str_value, session_id, card_uid, run_id
       FROM vala.drift.observations WHERE run_id = $1 ORDER BY series`,
    [run.runId],
    DriftRow,
  );
  expect(rows).toEqual([
    {
      series: "latency",
      num_value: 12.5,
      str_value: "12.5",
      session_id: null,
      card_uid: state.cardRef("model").uid,
      run_id: run.runId,
    },
  ]);
});

test("custom drift judges the submitted window on schedule", async ({ server, state }) => {
  await state.startBifrost();
  await state.run("model").observe.submit("observed-drift", { latency: 30 });
  await state.shutdown();
  const id = await binding(await Cards.connect(), state.rootRef);
  server.makeBindingDue(id);

  const result = await scheduledResult(server, id, 90_000);

  expect(result.outcome.verdict).toBe("failed");
  expect(result.subject).toEqual(state.cardRef("model"));
  expect(result.outcome.requirements).toHaveLength(1);
  const features = Object.values(result.outcome.requirements[0]?.detail?.drift.features ?? {});
  expect(features.some((feature) => (feature.score ?? 0) > 5)).toBe(true);
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
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect() });
  await state.startBifrost();

  expect(await state.run().forCard("model").observe.submit("observed-drift", { latency: 12.5 })).toEqual({
    kind: "recorded",
  });

  await expect(state.flush()).rejects.toMatchObject({ code: "WYRD_VALA_403_BIFROST_CARD_SCOPE" });
  await state.shutdown();
});

test("unsealable byte budget is refused at connect", async ({ state }) => {
  await expect(state.startBifrost({ clientByteLimitBytes: 1024 })).rejects.toMatchObject({
    code: "WYRD_CLIENT_400_CONFIG_INVALID",
  });
});

test("configured service context reaches both verification subjects", async ({ server: _ }) => {
  const cards = await Cards.connect();
  const { root } = await cards.declareFromPath(fixture("cards/observe_a_run/configured-service.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-configured-"));
  await cards.pull(root, bundle);
  const { key } = await runtimeAccount({ kind: "Service", name: "configured-service" });

  for (const environment of ["prod", "staging"]) {
    const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
    state.configure({ deploymentEnvironment: environment });
    const run = state.run();
    const agent = run.forCard("agent");

    const result = await agent.observe.verify("agent-check", "yes");
    expect(result.subject).toEqual(state.cardRef("agent"));
    expect(result.deployment_environment).toBe(environment);
    const submitted = await agent.observe.submit("agent-check", "yes");
    if (submitted.kind !== "queued") {
      throw new Error("Run evidence queues an execution");
    }
    const queued = await submitted.execution.wait({ timeoutMs: 60_000 });
    expect(queued.subject).toEqual(state.cardRef("agent"));
    expect(queued.deployment_environment).toBe(environment);

    const rootCheck = run.observe.verify("service-check", "yes");
    if (environment === "prod") {
      const checked = await rootCheck;
      expect(checked.subject).toEqual(state.rootRef);
      expect(checked.deployment_environment).toBe("prod");
    } else {
      await expect(rootCheck).rejects.toMatchObject({
        code: "WYRD_SPEC_400_VALIDATION",
        details: { field: "deployment_environment" },
      });
    }
    expect(() => state.configure()).toThrow(expect.objectContaining({ code: "WYRD_SDK_409_STATE_CONFIG_FROZEN" }));
  }
});
