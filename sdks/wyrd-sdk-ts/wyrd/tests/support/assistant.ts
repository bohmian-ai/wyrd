import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Bifrost, Cards, Gateway, WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";
import { z } from "zod";

import type { Reply } from "./http-receiver.js";
import { configureGateway } from "./local-development.js";
import { type DeclaredRef, fixture, declared, serverTest } from "./server.js";

/** The Verdict the local judge returns for every answer: passed, citing the whole answer. */
export const AGREES =
  '{"outcome":"passed","reason":"it agrees","citations":[{"slot":"answer","locator":{"json_pointer":""}}]}';

/** A Chat Completions answer from the local judge whose message is `content`. */
export function judgeReply(content: string): Reply {
  return {
    status: 200,
    body: {
      id: "chatcmpl-judge",
      object: "chat.completion",
      created: 1,
      model: "gpt-4o",
      choices: [{ index: 0, finish_reason: "stop", message: { role: "assistant", content } }],
      usage: { prompt_tokens: 5, completion_tokens: 3, total_tokens: 8 },
    },
  };
}

/** One judge call's `vala.gateway.calls` identity and attribution. */
export const JudgeCall = z.object({
  call_id: z.string(),
  sub_principal_id: z.string(),
  act_principal_id: z.string(),
  run_id: z.string().nullable(),
  card_uid: z.string().nullable(),
});

/** The declared assistant Service, its Card bundle, and its own key. */
export interface Assistant {
  readonly bundle: string;
  readonly key: string;
  readonly principal: string;
  readonly service: DeclaredRef;
  readonly answerCheck: DeclaredRef;
  readonly answerJudged: DeclaredRef;
  readonly agent: DeclaredRef;
}

/** The hydrated assistant, with its server calls fixed to `client`. */
export function assistantAs(assistant: Assistant, client: WyrdClient): WyrdState {
  return WyrdState.fromPath(assistant.bundle, { client });
}

/** Publish every captured row, then read each judge call as the administrator. */
export async function judgeCalls(server: { flushBifrost(): void }): Promise<z.infer<typeof JudgeCall>[]> {
  server.flushBifrost();
  return (await Bifrost.connect()).sql(
    "SELECT call_id, sub_principal_id, act_principal_id, run_id, card_uid FROM vala.gateway.calls",
    [],
    JudgeCall,
  );
}

/**
 * `test` on a server running the verification runtime whose gateway deploys
 * `gpt-4o` on a local judge that answers {@link AGREES}, with the latency
 * baseline, the latency Model, and the assistant Service declared and
 * hydrated. The gateway captures call metadata, so every judge call lands in
 * Bifrost. `state` is the assistant as its own Service, through the key
 * `issueKey` mints with no Role grant beyond its default `workload`.
 */
export const assistantTest = (options: { readonly perTest?: boolean } = {}) =>
  serverTest({ provider: () => judgeReply(AGREES), verificationRuntime: true, perTest: options.perTest }).extend<{
    assistant: Assistant;
    state: WyrdState;
  }>({
    assistant: [
      async ({ server: _ }, use) => {
        await configureGateway(WyrdClient.connect());
        await (await Gateway.connect()).putCapturePolicy({ mode: "metadata", payload_fields: [] });
        const cards = await Cards.connect();
        await cards.declareFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
        await cards.declareFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
        const receipt = await cards.declareFromPath(fixture("cards/verify_in_real_time/assistant.yaml"));
        const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-assistant-"));
        await cards.pull(receipt.root, bundle);
        const issued = await cli.issueKey({ ...receipt.root, space: receipt.root.space ?? "default" });
        await use({
          bundle,
          key: issued.key,
          principal: issued.principal_id,
          service: declared(receipt),
          answerCheck: declared(receipt, "answer-check"),
          answerJudged: declared(receipt, "answer-judged"),
          agent: declared(receipt, "assistant-agent"),
        });
      },
      { scope: options.perTest ? "test" : "file" },
    ],
    state: async ({ assistant }, use) => use(assistantAs(assistant, WyrdClient.connect({ credential: assistant.key }))),
  });
