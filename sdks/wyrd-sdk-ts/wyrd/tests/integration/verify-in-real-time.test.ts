import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { Bifrost, Cards, type CardRef, WyrdState } from "@wyrd/sdk";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** The Chat Completions answer the local judge returns: the answer passes. */
const JUDGE_PASSES = {
  id: "chatcmpl-judge",
  object: "chat.completion",
  created: 1,
  model: "gpt-test",
  choices: [{ index: 0, finish_reason: "stop", message: { role: "assistant", content: '{"passed":true}' } }],
  usage: { prompt_tokens: 5, completion_tokens: 3, total_tokens: 8 },
};

/** One hundred latencies spread like the healthy baseline's 0 to 99 ms. */
const HEALTHY_LATENCIES = Array.from({ length: 100 }, (_, row) => ({ latency: (row * 37) % 100 }));

/** One hundred latencies all at the slow end of the baseline. */
const SLOW_LATENCIES = Array.from({ length: 100 }, () => ({ latency: 99 }));

/**
 * The assistant Service, registered after its latency baseline and Model,
 * and the unfitted assistant whose `tier-drift` baseline never fits. Each test
 * gets a fresh offline state that verifies as its Service's own principal.
 */
const test = serverTest({ provider: () => ({ status: 200, body: JUDGE_PASSES }), verificationRuntime: true }).extend<{
  assistant: { readonly bundle: string; readonly latencyDrift: CardRef };
  unfittedBundle: { readonly bundle: string; readonly key: string };
  unfitted: WyrdState;
  assistantKey: string;
  state: WyrdState;
}>({
  assistant: [
    async ({ server: _ }, use) => {
      const cards = Cards.connect();
      await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
      await cards.registerFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
      const receipt = await cards.registerFromPath(fixture("cards/verify_in_real_time/assistant.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-assistant-"));
      await cards.hydrate(receipt.root, bundle);
      const latencyDrift = receipt.outcomes.find((outcome) => outcome.card_ref.name === "latency-drift");
      await use({ bundle, latencyDrift: latencyDrift?.card_ref as CardRef });
    },
    { scope: "file" },
  ],
  assistantKey: [
    async ({ server, assistant: _ }, use) =>
      use(server.credentialRegisteredService("default/Service/assistant@1.0.0", ["admin"])),
    { scope: "file" },
  ],
  unfittedBundle: [
    async ({ server, assistant: _ }, use) => {
      const cards = Cards.connect();
      const receipt = await cards.registerFromPath(fixture("cards/verify_in_real_time/unfitted-assistant.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-unfitted-"));
      await cards.hydrate(receipt.root, bundle);
      const key = server.credentialRegisteredService("default/Service/unfitted-assistant@1.0.0", ["admin"]);
      await use({ bundle, key });
    },
    { scope: "file" },
  ],
  unfitted: async ({ deployment: _, unfittedBundle }, use) => {
    vi.stubEnv("WYRD_API_KEY", unfittedBundle.key);
    await use(WyrdState.fromPath(unfittedBundle.bundle));
  },
  state: async ({ deployment: _, assistant, assistantKey }, use) => {
    vi.stubEnv("WYRD_API_KEY", assistantKey);
    await use(WyrdState.fromPath(assistant.bundle));
  },
});

// `tier-drift` control-charts a text column, so its baseline never fits.
test("verify before baseline ready is refused", async ({ unfitted }) => {
  await expect(unfitted.run("model").observe.verify("tier-drift", Array(5).fill({ tier: "gold" }))).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_409_BASELINE_NOT_READY",
  });
});

test("agent answer passes its verifier", async ({ state }) => {
  const judgment = await state.run("agent").observe.verify("answer-is-yes", { answer: "yes" });

  expect(judgment).toMatchObject({
    passed: true,
    verdict: "passed",
    kind: "eval_assertion",
    verifier: { name: "answer-is-yes" },
    subject: state.cardRef("agent"),
  });
});

test("agent answer fails its verifier", async ({ state }) => {
  const judgment = await state.run("agent").observe.verify("answer-is-yes", { answer: "no" });

  expect(judgment).toMatchObject({ passed: false, verdict: "failed", kind: "eval_assertion" });
});

test("judged answer passes the llm judge", async ({ provider, state }) => {
  const judgment = await state.run("agent").observe.verify("answer-is-judged", { answer: "yes" });

  expect(judgment).toMatchObject({ passed: true, kind: "eval_llm_judge" });
  const [graded] = await provider.requests(1);
  expect(graded?.path).toBe("/v1/chat/completions");
  expect(graded?.body).toContain("Grade the answer yes.");
});

test("model latency like the baseline passes its verifier", async ({ server, assistant, state }) => {
  server.waitForBaseline(assistant.latencyDrift.uid ?? "", 90_000);

  const judgment = await state.run("model").observe.verify("latency-drift", HEALTHY_LATENCIES);

  expect(judgment).toMatchObject({ passed: true, kind: "drift_psi", subject: state.cardRef("model") });
});

test("model latency drift is judged failed", async ({ server, assistant, state }) => {
  server.waitForBaseline(assistant.latencyDrift.uid ?? "", 90_000);

  const judgment = await state.run("model").observe.verify("latency-drift", SLOW_LATENCIES);

  expect(judgment).toMatchObject({
    passed: false,
    verdict: "failed",
    kind: "drift_psi",
    counts: { implementation: "drift", drifted_features: 1, total_features: 1 },
  });
});

test("unbound verifier fails locally", async ({ state }) => {
  await expect(state.run("agent").observe.verify("not-bound-here", { answer: "yes" })).rejects.toMatchObject({
    code: "WYRD_SDK_404_UNKNOWN_VERIFIER",
  });
});

test("input of the wrong shape fails locally", async ({ state }) => {
  await expect(state.run("agent").observe.verify("answer-is-yes", [{ answer: "yes" }])).rejects.toMatchObject({
    code: "WYRD_SDK_400_INVALID_OBSERVATION",
  });
});

test("caller without evals run is refused", async ({ server, state }) => {
  vi.stubEnv("WYRD_API_KEY", server.scopedApiKey("assistant_reader", ["cards:read"]));

  await expect(state.run("agent").observe.verify("answer-is-yes", { answer: "yes" })).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("another tenant cannot verify the assistant", async ({ server, state }) => {
  const otherTenant = server.seedTenant("other-tenant");
  vi.stubEnv("WYRD_API_KEY", server.bootstrapServiceInTenant(otherTenant, ["admin"], "other-service"));

  await expect(state.run("agent").observe.verify("answer-is-yes", { answer: "yes" })).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
  });
});

test("verify records no observation", async ({ server, state }) => {
  const run = state.run("agent");

  await run.observe.verify("answer-is-yes", { answer: "yes" });
  server.flushBifrost();

  const reader = await Bifrost.connect();
  const observed = await reader.sql("SELECT record_id FROM vala.eval.observations WHERE run_id = $1", [run.runId]);
  expect(observed.numRows).toBe(0);
});
