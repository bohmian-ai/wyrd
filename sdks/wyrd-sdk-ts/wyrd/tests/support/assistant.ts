import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Bifrost, Cards, Gateway, WyrdClient, WyrdState } from "wyrd-ai";
import { z } from "zod";

import type { ReceivedRequest, Reply } from "./http-receiver.js";
import { configureGateway } from "./local-development.js";
import { type DeclaredRef, fixture, declared, runtimeAccount, serverTest } from "./server.js";

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

/** One assistant tool call of `tool` with JSON `args`, as the model returns it: under the provider-facing name, which spells the dotted wire name with underscores. */
export function toolCall(id: string, tool: string, args: unknown): unknown {
  const name = tool.replaceAll(".", "_");
  return { id, type: "function", function: { name, arguments: JSON.stringify(args) } };
}

/** A Chat Completions answer whose one assistant turn calls `calls`. */
export function toolTurn(calls: readonly unknown[]): Reply {
  const reply = judgeReply("");
  const message = { role: "assistant", content: null, tool_calls: calls };
  return {
    ...reply,
    body: { ...(reply.body as object), choices: [{ index: 0, finish_reason: "tool_calls", message }] },
  };
}

/** The sharing-policy judge's first turn: `fs.read` the policy path its prompt in `body` names. */
function readPolicy(body: string): ReturnType<typeof toolCall>[] {
  const path = /read (\/inputs\/[\w./-]+) with fs\.read/i.exec(body)?.[1];
  if (path === undefined) {
    throw new Error("the judge's prompt names no policy to read");
  }
  return [toolCall("call-policy", "fs.read", { path })];
}

/** The sharing-policy judge's Verdict once `body` carries its `fs.read` result; no Verdict when the read has no `external-audit` line. */
function sharingVerdict(body: string): string {
  const messages = (JSON.parse(body) as { messages: { role: string; content: string | null }[] }).messages;
  const release = JSON.stringify(messages.filter(({ role }) => role === "user").map(({ content }) => content));
  let output: { sources?: string[]; units?: { text: string; locator: unknown }[] };
  try {
    output = JSON.parse(messages.find(({ role }) => role === "tool")?.content ?? "") as typeof output;
  } catch {
    return "the policy did not read";
  }
  const line = (output.units ?? []).find(({ text }) => text.includes("external-audit"));
  if (line === undefined) {
    return "the policy did not read";
  }
  // The finance example's Verifier takes the report inside its one `release` slot.
  const report = release.includes("Decide whether the company sharing policy permits")
    ? { slot: "release", locator: { json_pointer: "/report/text" } }
    : { slot: "report", locator: { json_pointer: "/text" } };
  const citations = [report, { slot: output.sources?.[0], locator: line.locator }];
  if (release.includes("PROVIDER-UNCERTAIN")) {
    return JSON.stringify({
      outcome: "inconclusive",
      reason: "the policy does not settle this release",
      citations: [],
    });
  }
  if (release.includes("BANK-ACCOUNT-DEMO-001")) {
    return JSON.stringify({ outcome: "failed", reason: "the report names a bank account identifier", citations });
  }
  return JSON.stringify({ outcome: "passed", reason: "an external audit may receive this report", citations });
}

/**
 * The sharing-policy judge's reply: it reads the policy its prompt names with
 * `fs.read` first, then is inconclusive for a release naming
 * `PROVIDER-UNCERTAIN`, fails one naming `BANK-ACCOUNT-DEMO-001`, and
 * otherwise passes, citing the report text and the policy line permitting
 * `external-audit`.
 */
export function sharingPolicyJudge(request: ReceivedRequest): Reply {
  return /"role"\s*:\s*"tool"/.test(request.body)
    ? judgeReply(sharingVerdict(request.body))
    : toolTurn(readPolicy(request.body));
}

/** One judge call's `bifrost.gateway.calls` identity and attribution. */
export const JudgeCall = z.object({
  call_id: z.string(),
  sub_principal_id: z.string(),
  act_principal_id: z.string(),
  run_id: z.string().nullable(),
  subject_card_uid: z.string().nullable(),
});

/** The declared assistant Service, its Card bundle, and its runtime account's key and principal. */
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
    "SELECT call_id, sub_principal_id, act_principal_id, run_id, subject_card_uid FROM bifrost.gateway.calls",
    [],
    JudgeCall,
  );
}

/**
 * Deploy `gpt-4o` and capture call metadata on the gateway, register the
 * latency baseline, the latency Model, and the assistant Service, and hydrate
 * the assistant, all as `client` in its tenant. The returned key is the
 * assistant's runtime account's, holding only `workload`.
 */
export async function registerAssistant(client: WyrdClient): Promise<Assistant> {
  await configureGateway(client);
  await (await Gateway.connect({ client })).putCapturePolicy({ mode: "metadata", payload_fields: [] });
  const cards = await Cards.connect({ client });
  await cards.declareFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  await cards.declareFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
  const receipt = await cards.declareFromPath(fixture("cards/verify_in_real_time/assistant.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-assistant-"));
  await cards.pull(receipt.root, bundle);
  const { principalId, key } = await runtimeAccount({ kind: "Service", name: receipt.root.name }, client);
  return {
    bundle,
    key,
    principal: principalId,
    service: declared(receipt),
    answerCheck: declared(receipt, "answer-check"),
    answerJudged: declared(receipt, "answer-judged"),
    agent: declared(receipt, "assistant-agent"),
  };
}

/** How an assistant story file starts its server. */
export interface AssistantOptions {
  /** Start a fresh server for every test instead of one per story file. */
  readonly perTest?: boolean;
  /** The local judge's answer; {@link AGREES} at once by default. */
  readonly judge?: (request: ReceivedRequest) => Reply;
  /** Scribe outbox capacity in items; the production capacity by default. */
  readonly scribeOutboxCapacity?: number;
}

/**
 * `test` on a server running the verification runtime whose gateway deploys
 * `gpt-4o` on a local judge that answers {@link AGREES}, with the assistant
 * registered and hydrated by {@link registerAssistant}. The gateway captures
 * call metadata, so every judge call lands in Bifrost. `state` is the
 * assistant as its own Service.
 */
export const assistantTest = (options: AssistantOptions = {}) =>
  serverTest({
    provider: options.judge ?? (() => judgeReply(AGREES)),
    verificationRuntime: true,
    perTest: options.perTest,
    scribeOutboxCapacity: options.scribeOutboxCapacity,
  }).extend<{
    assistant: Assistant;
    state: WyrdState;
  }>({
    assistant: [
      async ({ server: _ }, use) => {
        await use(await registerAssistant(WyrdClient.connect()));
      },
      { scope: options.perTest ? "test" : "file" },
    ],
    state: async ({ assistant }, use) => use(assistantAs(assistant, WyrdClient.connect({ credential: assistant.key }))),
  });
