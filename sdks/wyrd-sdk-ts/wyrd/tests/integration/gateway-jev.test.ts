import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Cards, Gateway, WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** The Jev key the server's `test-jev-key` binding resolves; only the upstream may see it. */
const JEV_KEY = "jev-operator-key";
vi.stubEnv("WYRD_TEST_GATEWAY_PROVIDER_KEY", JEV_KEY);

/** The questions every direct call in this story asks. */
const QUESTIONS = {
  helpful: { type: "noul", instructions: "Does the reply help the customer?" },
  tone: {
    type: "choice",
    instructions: "What is the tone of the reply?",
    criteria: { polite: "courteous", rude: "curt" },
  },
};

/** State text → the status the upstream refuses it with. */
const REFUSALS: Record<string, number> = {
  "refused-form": 400,
  "rate-limited": 429,
  "server-error": 500,
  overloaded: 529,
  "bad-key": 401,
};

/** Jev's answer: `helpful` with probability `noul`, tone `tone`. */
function answer(noul: number, tone: string) {
  return {
    model: "jev-1.13.0",
    answers: {
      helpful: { type: "noul", noul },
      tone: { type: "choice", choice: tone, confidence: 0.8, probabilities: { polite: 0.8, rude: 0.2 } },
    },
    usage: { input_tokens: 378, output_tokens: 65 },
  };
}

/**
 * The local Jev upstream: helpful and polite unless the state names a
 * scripted case — `Go away` is unhelpful and rude, each `REFUSALS` key is
 * refused.
 */
function jev(request: ReceivedRequest): Reply {
  const state = JSON.stringify((JSON.parse(request.body) as { state?: unknown }).state);
  const refused = Object.entries(REFUSALS).find(([needle]) => state.includes(needle));
  if (refused) {
    return { status: refused[1], body: { detail: "key cred" } };
  }
  return { status: 200, body: state.includes("Go away") ? answer(0.12, "rude") : answer(0.93, "polite") };
}

/** A Gateway JSON reply; a refusal carries its catalog code. */
type GatewayReply = { readonly error?: { readonly code?: string } };

/** POST `body` to the Gateway `route` with the administrator's access token. */
async function post(route: string, body: unknown): Promise<{ status: number; reply: GatewayReply }> {
  const client = WyrdClient.connect();
  const response = await fetch(`${client.serverUrl}${route}`, {
    method: "POST",
    headers: { authorization: `Bearer ${await client.accessToken()}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: response.status, reply: (await response.json()) as GatewayReply };
}

/**
 * A verification-capable server with one Jev deployment serving
 * `system_one`, the only operation Jev serves, and the hydrated `reviewer` Service
 * acting as its own Card-scoped key.
 */
const test = serverTest({ provider: jev, verificationRuntime: true }).extend<{ reviewer: WyrdState }>({
  reviewer: [
    async ({ server: _ }, use) => {
      await cli.putProviderCredential({
        name: "jev-key",
        provider: "jev",
        source: { environment: { binding: "test-jev-key" } },
      });
      await Gateway.connect().putDeployment({
        name: "jev-latest",
        model: { provider: "jev", model: "jev-latest" },
        adapter: "jev",
        auth: { bearer: { credential: "jev-key" } },
        capabilities: ["system_one"],
        routing_weight: 1,
      });
      const cards = Cards.connect();
      const receipt = await cards.registerFromPath(fixture("cards/gateway_jev/reviewer.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-reviewer-"));
      await cards.hydrate(receipt.root, bundle);
      const { key } = await cli.issueKey({ kind: "Service", name: "reviewer", version: "1.0.0", space: "default" });
      await use(WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) }));
    },
    { scope: "file" },
  ],
});

test("native systemone answers relay intact", async ({ provider, reviewer: _ }) => {
  const before = provider.received.length;
  const body = { model: "jev-latest", state: "Thanks!", questions: QUESTIONS };

  const { status, reply } = await post("/v1/systemone", body);

  expect({ status, reply }).toEqual({ status: 200, reply: answer(0.93, "polite") });
  const [upstream, ...rest] = provider.received.slice(before);
  expect(rest).toEqual([]);
  expect(upstream).toMatchObject({ path: "/v1/systemone", headers: { authorization: `Bearer ${JEV_KEY}` } });
  expect(JSON.parse(upstream?.body ?? "")).toEqual(body);
});

test("invalid jev questions are refused before jev", async ({ provider, reviewer: _ }) => {
  const before = provider.received.length;

  for (const body of [
    { model: "jev-latest", state: "s", questions: { q: { type: "essay", instructions: "x" } } },
    { model: "jev-latest", state: { answer: "s" }, questions: QUESTIONS },
    { model: "jev-latest", state: "s", questions: QUESTIONS, temperature: 0 },
  ]) {
    const { status, reply } = await post("/v1/systemone", body);
    expect({ body, status, code: reply.error?.code }).toEqual({
      body,
      status: 400,
      code: "WYRD_GATEWAY_400_INVALID_REQUEST",
    });
  }
  expect(provider.received.slice(before)).toEqual([]);
});

test("chat completions never reach jev", async ({ provider, reviewer: _ }) => {
  const before = provider.received.length;
  const client = WyrdClient.connect();
  const openai = new OpenAI({ baseURL: `${client.serverUrl}/v1`, apiKey: await client.accessToken(), maxRetries: 0 });

  await expect(
    openai.chat.completions.create({
      model: "jev/jev-latest",
      messages: [{ role: "user", content: JSON.stringify({ state: "Thanks!", questions: QUESTIONS }) }],
    }),
  ).rejects.toMatchObject({ status: 404, code: "WYRD_GATEWAY_404_MODEL_UNAVAILABLE" });
  expect(provider.received.slice(before)).toEqual([]);
});

test("jev judge passes a helpful polite reply", async ({ provider, reviewer }) => {
  const before = provider.received.length;

  const judgment = await reviewer
    .run("agent")
    .observe.verify("reply-quality", { answer: "Your refund is on its way." });

  expect(judgment).toMatchObject({ passed: true, verdict: "passed", kind: "eval_llm_judge" });
  const [upstream] = provider.received.slice(before);
  expect(JSON.parse(upstream?.body ?? "")).toMatchObject({ model: "jev-latest", state: "Your refund is on its way." });
});

test("jev judge fails an unhelpful rude reply", async ({ reviewer }) => {
  const judgment = await reviewer.run("agent").observe.verify("reply-quality", { answer: "Go away." });

  // The judge task itself passes; both threshold assertions fail.
  expect(judgment).toMatchObject({
    passed: false,
    verdict: "failed",
    counts: { implementation: "eval", passed_tasks: 1, failed_tasks: 2, total_tasks: 3 },
  });
});

test("jev judge judges a reply with quotes and newlines", async ({ provider, reviewer }) => {
  const before = provider.received.length;
  const reply = 'He said "your refund is on its way".\nAnything else?';

  const judgment = await reviewer.run("agent").observe.verify("reply-quality", { answer: reply });

  expect(judgment).toMatchObject({ passed: true, verdict: "passed" });
  const [upstream, ...rest] = provider.received.slice(before);
  expect(rest).toEqual([]);
  expect(JSON.parse(upstream?.body ?? "")).toMatchObject({ state: reply });
});

// Last in the file: an upstream refusal cools the shared deployment down.
test("jev refusals map to stable errors", async ({ reviewer: _ }) => {
  for (const [state, status, code] of [
    ["refused-form", 400, "WYRD_GATEWAY_400_INVALID_REQUEST"],
    ["rate-limited", 429, "WYRD_GATEWAY_429_UPSTREAM_THROTTLED"],
    ["server-error", 502, "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"],
    ["overloaded", 502, "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"],
    ["bad-key", 502, "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE"],
  ] as const) {
    const { status: answered, reply } = await post("/v1/systemone", {
      model: "jev-latest",
      state,
      questions: QUESTIONS,
    });
    expect({ state, answered, code: reply.error?.code }).toEqual({ state, answered: status, code });
    expect(JSON.stringify(reply)).not.toContain("key cred");
  }
});
