import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Bifrost, Cards, Gateway, WyrdClient, WyrdState } from "wyrd-ai";
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

/** The typed questions the native Jev judge Prompt asks about every reply. */
const JUDGE_QUESTIONS = {
  helpful: {
    type: "noul",
    instructions: "Does the reply help the customer?",
    criteria: { true: "it resolves or advances the request", false: "it ignores or deflects the request" },
  },
  tone: {
    type: "choice",
    instructions: "What is the tone of the reply?",
    criteria: { polite: "courteous and respectful", rude: "curt or dismissive" },
  },
};

/** State text → the status the upstream refuses it with. */
const REFUSALS: Record<string, number> = {
  "refused-form": 400,
  "rate-limited": 429,
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
 * refused with a body that echoes the operator key after `key cred`.
 */
function jev(request: ReceivedRequest): Reply {
  const state = JSON.stringify((JSON.parse(request.body) as { state?: unknown }).state);
  const refused = Object.entries(REFUSALS).find(([needle]) => state.includes(needle));
  if (refused) {
    return { status: refused[1], body: { detail: `key cred ${JEV_KEY}` } };
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

/** A stock OpenAI client pointed at the Gateway with the administrator's access token. */
async function openaiClient(): Promise<OpenAI> {
  const client = WyrdClient.connect();
  return new OpenAI({ baseURL: `${client.serverUrl}/v1`, apiKey: await client.accessToken(), maxRetries: 0 });
}

/** Messages whose last user turn is the typed Jev request about `state`. */
function chat(state: string): OpenAI.Chat.ChatCompletionMessageParam[] {
  return [
    { role: "system", content: "Judge the reply." },
    { role: "user", content: JSON.stringify({ state, questions: QUESTIONS }) },
  ];
}

/**
 * A verification-capable server with one Jev deployment serving
 * `system_one` and the `chat_completions` pass-through, call metadata
 * captured so every judge call lands in Bifrost, and the hydrated
 * `reviewer` Service acting as its own Card-scoped key.
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
        capabilities: ["system_one", "chat_completions"],
        routing_weight: 1,
      });
      await Gateway.connect().putCapturePolicy({ mode: "metadata", payload_fields: [] });
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

test("chat completions pass typed jev requests through", async ({ provider, reviewer: _ }) => {
  const before = provider.received.length;
  const openai = await openaiClient();
  const state = 'He said "thanks".\nAnything else?';

  const completion = await openai.chat.completions.create({ model: "jev/jev-latest", messages: chat(state) });

  expect(JSON.parse(completion.choices[0]?.message.content ?? "")).toEqual(answer(0.93, "polite"));
  expect(completion.usage).toMatchObject({ prompt_tokens: 378, completion_tokens: 65, total_tokens: 443 });
  const [upstream, ...rest] = provider.received.slice(before);
  expect(rest).toEqual([]);
  expect(upstream).toMatchObject({ path: "/v1/systemone", headers: { authorization: `Bearer ${JEV_KEY}` } });
  expect(JSON.parse(upstream?.body ?? "")).toEqual({ model: "jev-latest", state, questions: QUESTIONS });

  await expect(
    openai.chat.completions.create({
      model: "jev/jev-latest",
      messages: [{ role: "user", content: "Is this reply helpful?" }],
    }),
  ).rejects.toMatchObject({ status: 400, code: "WYRD_GATEWAY_400_INVALID_REQUEST" });
  expect(provider.received.length).toBe(before + 1);
});

test("jev judge passes a helpful polite reply", async ({ provider, reviewer }) => {
  const before = provider.received.length;

  const judgment = await reviewer
    .run("agent")
    .observe.verify("reply-quality", { question: "Where is my refund?", answer: "Your refund is on its way." });

  expect(judgment).toMatchObject({ passed: true, verdict: "passed", kind: "eval_llm_judge" });
  const [upstream] = provider.received.slice(before);
  expect(JSON.parse(upstream?.body ?? "")).toMatchObject({
    model: "jev-latest",
    state: "Question: Where is my refund?\nAnswer: Your refund is on its way.",
  });
});

test("jev judge fails an unhelpful rude reply", async ({ reviewer }) => {
  const judgment = await reviewer
    .run("agent")
    .observe.verify("reply-quality", { question: "Where is my refund?", answer: "Go away." });

  // The judge task itself passes; both threshold assertions fail.
  expect(judgment).toMatchObject({
    passed: false,
    verdict: "failed",
    counts: { implementation: "eval", passed_tasks: 1, failed_tasks: 2, total_tasks: 3 },
  });
});

test("jev judge judges a reply with quotes and newlines", async ({ server, provider, reviewer }) => {
  const before = provider.received.length;
  // The file shares one server, so the judge's call is the one `system_one`
  // call this verify adds. Correlating it to the judgment's Run is owned by
  // the verify-attribution change, not by the Jev provider.
  const systemOneCalls = async (): Promise<Array<Record<string, unknown>>> => {
    server.flushBifrost();
    return (
      await (
        await Bifrost.connect()
      ).sql(
        "SELECT call_id, outcome, CAST(attempt_count AS INT) AS attempt_count FROM vala.gateway.calls WHERE operation = 'system_one'",
      )
    )
      .toArrow()
      .toArray()
      .map((row) => row.toJSON());
  };
  const earlier = new Set((await systemOneCalls()).map((call) => call.call_id));
  const question = 'Is my "refund" done?\nOrder 7';
  const reply = 'He said "your refund is on its way".\nAnything else?';

  const judgment = await reviewer.run("agent").observe.verify("reply-quality", { question, answer: reply });

  expect(judgment).toMatchObject({ passed: true, verdict: "passed" });
  const [upstream, ...rest] = provider.received.slice(before);
  expect(rest).toEqual([]);
  expect(upstream?.headers.authorization).toBe(`Bearer ${JEV_KEY}`);
  expect(JSON.parse(upstream?.body ?? "")).toEqual({
    model: "jev-latest",
    state: `Question: ${question}\nAnswer: ${reply}`,
    questions: JUDGE_QUESTIONS,
  });
  const judged = (await systemOneCalls()).filter((call) => !earlier.has(call.call_id));
  expect(judged.map(({ outcome, attempt_count }) => ({ outcome, attempt_count }))).toEqual([
    { outcome: "succeeded", attempt_count: 1 },
  ]);
  const recorded = (
    await (
      await Bifrost.connect()
    ).sql("SELECT implementation, verdict FROM vala.verification.results WHERE result_id = $1", [judgment.execution_id])
  )
    .toArrow()
    .toArray()
    .map((row) => row.toJSON());
  expect(recorded).toEqual([{ implementation: "eval", verdict: "passed" }]);
});

test("jev judge refusal is a typed error without retry", async ({ provider, reviewer }) => {
  const before = provider.received.length;

  const refused = reviewer
    .run("agent")
    .observe.verify("reply-quality", { question: "Where is my refund?", answer: "refused-form" });

  await expect(refused).rejects.toMatchObject({ code: "WYRD_VERIFICATION_502_DEPENDENCY_FAILED" });
  await expect(refused).rejects.not.toThrow(JEV_KEY);
  expect(provider.received.length).toBe(before + 1);
});

/** The OpenAI client error a Chat Completions call to the Jev model about `state` rejects with. */
async function chatRefusal(openai: OpenAI, state: string): Promise<InstanceType<typeof OpenAI.APIError> | undefined> {
  return openai.chat.completions.create({ model: "jev/jev-latest", messages: chat(state) }).then(
    () => undefined,
    (error: unknown) => error as InstanceType<typeof OpenAI.APIError>,
  );
}

// Last in the file: an upstream throttle cools the shared deployment down
// for 30 seconds. A validation refusal and a refused key do not, so both
// routes prove them first; throttling is proven last. The 5xx statuses share
// throttling's retry class and are pinned by the gateway adapter's unit tests.
// Every refusal relays exactly like every other provider's: the upstream
// status with Jev's body natively and an OpenAI error envelope on Chat
// Completions, the operator key removed, and no Wyrd code invented.
test("jev refusals relay like other providers", async ({ provider, reviewer: _ }) => {
  const openai = await openaiClient();
  const native = (state: string) => post("/v1/systemone", { model: "jev-latest", state, questions: QUESTIONS });

  for (const [state, status] of [
    ["refused-form", 400],
    ["bad-key", 401],
  ] as const) {
    const before = provider.received.length;
    const refused = await native(state);
    expect({ status: refused.status, reply: refused.reply }).toEqual({ status, reply: { detail: "key cred " } });
    const refusedChat = await chatRefusal(openai, state);
    expect({ state, status: refusedChat?.status, code: refusedChat?.code ?? null }).toEqual({
      state,
      status,
      code: null,
    });
    expect(JSON.stringify(refusedChat?.error)).not.toContain(JEV_KEY);
    expect(provider.received.length).toBe(before + 2);
  }

  const throttled = await chatRefusal(openai, "rate-limited");
  expect({ status: throttled?.status, code: throttled?.code ?? null }).toEqual({ status: 429, code: null });
  expect(JSON.stringify(throttled?.error)).not.toContain(JEV_KEY);
});
