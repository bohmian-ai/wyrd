import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { trace } from "@opentelemetry/api";
import OpenAI from "openai";
import { expect } from "vitest";
import { z } from "zod";

import { Bifrost, Cards, Gateway, type Run, type WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { fixture } from "./server.js";

/** The Chat Completions answer the local upstream returns. */
export const COMPLETION = {
  id: "chatcmpl-local",
  object: "chat.completion",
  created: 1,
  model: "gpt-4o",
  choices: [{ index: 0, message: { role: "assistant", content: "hi" }, finish_reason: "stop", logprobs: null }],
  usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
};

/** One evidence row: the Card it is attributed to. */
export const Attributed = z.object({ card_uid: z.string().nullable() });

/** What {@link work} leaves running for a caller to keep using. */
export interface Worked {
  readonly state: WyrdState;
  readonly bundle: string;
  readonly agentUid: string;
}

/**
 * Run the whole local workflow as `client`: register and hydrate the
 * assistant, invoke the Gateway, observe and verify a Run, export a span
 * through `state.startTelemetry`, and
 * read the evidence back. No key is issued and nothing is flushed.
 */
export async function work(client: WyrdClient): Promise<Worked> {
  await configureGateway(client);
  const cards = await Cards.connect({ client });
  await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  await cards.registerFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
  const receipt = await cards.registerFromPath(fixture("cards/verify_in_real_time/assistant.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-local-"));
  await cards.hydrate(receipt.root, bundle);
  const state = WyrdState.fromPath(bundle, { client });
  await state.startBifrost();

  expect(await invoke(client)).toBe("hi");

  const run = state.run();
  expect(await run.forCard("model").observe.submit("latency-drift", { latency: 12.5 })).toEqual({ kind: "recorded" });
  const result = await run.forCard("agent").observe.verify("answer-check", { answer: "yes" });
  expect(result.outcome.verdict).toBe("passed");
  expect(result.verifier.name).toBe("answer-check");

  await state.startTelemetry();
  await state.startTelemetry();
  exportSpan(run.forCard("agent"));
  // Shutdown flushes the exported span.
  await state.shutdown();

  const bifrost = await Bifrost.connect({ client });
  for (const [table, alias] of [
    ["vala.drift.observations", "model"],
    ["vala.traces.spans", "agent"],
  ] as const) {
    const rows = await bifrost.sql(`SELECT card_uid FROM ${table} WHERE run_id = $1`, [run.runId], Attributed);
    expect(rows, table).toEqual([{ card_uid: state.cardRef(alias).uid }]);
  }
  return { state, bundle, agentUid: state.cardRef("agent").uid ?? "" };
}

/** Start one `answer` span inside `run`'s scope, which attributes it to that run and Card. */
export function exportSpan(run: Run): void {
  run.scope(() => trace.getTracer("wyrd.tests.local").startSpan("answer").end());
}

/** Write the `openai-key` credential and deploy `openai` on it as `client`. */
export async function configureGateway(client: WyrdClient): Promise<void> {
  await cli.putProviderCredential(
    { name: "openai-key", provider: "openai", source: { managed_secret: { secret: "sk-local-upstream" } } },
    { client },
  );
  await (await Gateway.connect({ client })).putDeployment({
    name: "openai",
    adapter: "openai",
    auth: { bearer: { credential: "openai-key" } },
    capabilities: ["chat_completions"],
  });
}

/** Send one Chat Completions turn through the Gateway with the stock OpenAI client. */
export async function invoke(client: WyrdClient): Promise<string> {
  const gateway = new OpenAI({
    baseURL: `${client.serverUrl}/v1`,
    apiKey: "wyrd",
    fetch: client.httpAuth,
    maxRetries: 0,
  });
  const completion = await gateway.chat.completions.create({
    model: "openai/gpt-4o",
    messages: [{ role: "user", content: "hi" }],
  });
  return completion.choices[0]?.message.content ?? "";
}
