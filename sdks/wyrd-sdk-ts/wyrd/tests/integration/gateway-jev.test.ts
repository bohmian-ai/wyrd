import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Gateway, verify, WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { serverTest } from "../support/server.js";

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

const { Agent, Check, ChoiceQuestion, Component, NoulQuestion, Prompt, Requirement, ScoreQuestion, Service, Verifier } =
  verify;

/** State marker → the refund question's selected label. */
const REFUND_CHOICES: Record<string, string> = {
  "CASE-PASS": "passed",
  "CASE-FAIL": "failed",
  "CASE-UNSURE": "inconclusive",
};

/** The refund desk's questions as the native Jev questions the typed classes lower to. */
const NATIVE_DESK_QUESTIONS = {
  refund_policy: {
    type: "choice",
    instructions: "Does the reply promise a refund the policy does not allow?",
    criteria: {
      passed: "The reply makes no refund promise.",
      failed: "The reply promises a refund.",
      inconclusive: "The reply cannot be judged.",
    },
  },
  helpful: {
    type: "noul",
    instructions: "Does the reply help the customer?",
    criteria: { true: "It answers the question.", false: "It does not." },
  },
  polite: { type: "noul", instructions: "Is the reply polite?" },
  urgency: {
    type: "score",
    instructions: "How urgently does the reply need follow-up?",
    criteria: ["Routine", "Soon", "Immediate"],
  },
} as const;

/** The Prompt's state template: the bound `context` input. */
const STATE = `\${context}`;

/** The same questions authored with the public question classes. */
const TYPED_DESK_QUESTIONS = {
  refund_policy: new ChoiceQuestion({
    instructions: "Does the reply promise a refund the policy does not allow?",
    criteria: {
      passed: "The reply makes no refund promise.",
      failed: "The reply promises a refund.",
      inconclusive: "The reply cannot be judged.",
    },
  }),
  helpful: new NoulQuestion({
    instructions: "Does the reply help the customer?",
    criteria: { true_description: "It answers the question.", false_description: "It does not." },
  }),
  polite: new NoulQuestion({ instructions: "Is the reply polite?" }),
  urgency: new ScoreQuestion({
    instructions: "How urgently does the reply need follow-up?",
    criteria: ["Routine", "Soon", "Immediate"],
  }),
};

/** The policy desk's questions: a custom Choice, two Nouls, and a three-level Score. */
const POLICY_QUESTIONS = {
  refund: {
    type: "choice",
    instructions: "Does the reply follow the refund policy?",
    criteria: {
      compliant: "It follows the policy.",
      violation: "It breaks the policy.",
      unknown: "It cannot be judged.",
    },
  },
  helpful: { type: "noul", instructions: "Does the reply help the customer?" },
  unresolved: { type: "noul", instructions: "Does the reply leave the issue unresolved?" },
  compliance: { type: "score", instructions: "How compliant is the reply?", criteria: ["Low", "Medium", "High"] },
} as const;

/** Jev's answers to the policy desk questions. */
function policyAnswers(label: string, confidence: number, helpful: number, unresolved: number, score: number) {
  return {
    refund: { type: "choice", choice: label, confidence, probabilities: { [label]: confidence } },
    helpful: { type: "noul", noul: helpful },
    unresolved: { type: "noul", noul: unresolved },
    compliance: { type: "score", score, legend: { "0": "Low", "1": "Medium", "2": "High" } },
  };
}

/** State marker → Jev's policy desk answers: every bound, both polarities, and the band between. */
const POLICY_ANSWERS: Record<string, ReturnType<typeof policyAnswers>> = {
  "DESK-PASS": policyAnswers("compliant", 0.3, 0.75, 0.02, 2),
  "DESK-FAIL": policyAnswers("violation", 0.99, 0.25, 0.98, 1.8),
  "DESK-UNSURE": policyAnswers("unknown", 0.5, 0.5, 0.5, 2),
};

/** Jev's answer to every refund desk question, selecting `choice` for the refund question. */
function deskAnswer(choice: string) {
  return {
    model: "jev-1.13.0",
    answers: {
      refund_policy: {
        type: "choice",
        choice,
        confidence: 0.9,
        probabilities: Object.fromEntries(
          ["passed", "failed", "inconclusive"].map((label) => [label, label === choice ? 0.9 : 0.05]),
        ),
      },
      helpful: { type: "noul", noul: 0.8 },
      polite: { type: "noul", noul: 0.7 },
      urgency: { type: "score", score: 1, legend: { "0": "Routine", "1": "Soon", "2": "Immediate" } },
    },
    usage: { input_tokens: 50, output_tokens: 5 },
  };
}

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
  const desk = Object.entries(REFUND_CHOICES).find(([marker]) => state.includes(marker));
  if (desk) {
    return { status: 200, body: deskAnswer(desk[1]) };
  }
  const policy = Object.entries(POLICY_ANSWERS).find(([marker]) => state.includes(marker));
  if (policy) {
    return { status: 200, body: { model: "jev-1.13.0", answers: policy[1] } };
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

/** A server with one Jev deployment serving `system_one` and the `chat_completions` pass-through. */
const test = serverTest({ provider: jev, verificationRuntime: true }).extend<{ deployed: void }>({
  deployed: [
    async ({ server: _ }, use) => {
      await cli.putProviderCredential({
        name: "jev-key",
        provider: "jev",
        source: { environment: { binding: "test-jev-key" } },
      });
      await (await Gateway.connect()).putDeployment({
        name: "jev",
        adapter: "jev",
        auth: { bearer: { credential: "jev-key" } },
        capabilities: ["system_one", "chat_completions"],
      });
      await use();
    },
    { scope: "file" },
  ],
});

test("native systemone answers relay intact", async ({ provider, deployed: _ }) => {
  const before = provider.received.length;
  const body = { model: "jev-latest", state: "Thanks!", questions: QUESTIONS };

  const { status, reply } = await post("/v1/systemone", { ...body, model: "jev/jev-latest" });

  expect({ status, reply }).toEqual({ status: 200, reply: answer(0.93, "polite") });
  const [upstream, ...rest] = provider.received.slice(before);
  expect(rest).toEqual([]);
  expect(upstream).toMatchObject({ path: "/v1/systemone", headers: { authorization: `Bearer ${JEV_KEY}` } });
  expect(JSON.parse(upstream?.body ?? "")).toEqual(body);
});

test("invalid jev questions are refused before jev", async ({ provider, deployed: _ }) => {
  const before = provider.received.length;

  for (const body of [
    { model: "jev/jev-latest", state: "s", questions: { q: { type: "essay", instructions: "x" } } },
    { model: "jev/jev-latest", state: { answer: "s" }, questions: QUESTIONS },
    { model: "jev/jev-latest", state: "s", questions: QUESTIONS, temperature: 0 },
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

test("chat completions pass typed jev requests through", async ({ provider, deployed: _ }) => {
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
test("jev refusals relay like other providers", async ({ provider, deployed: _ }) => {
  const openai = await openaiClient();
  const native = (state: string) => post("/v1/systemone", { model: "jev/jev-latest", state, questions: QUESTIONS });

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

/**
 * `refund-desk`: the `refund-judge` Jev Agent asking `prompt`, verified by
 * `refund-promise`, which binds the `reply` slot to `${context}` and reads the
 * judge's answer through `transform`.
 */
function refundDesk(prompt: verify.Prompt, transform: verify.VerdictTransform): verify.Service {
  const judge = new Agent("refund-judge", {
    version: "1.0.0",
    prompt,
    execution_route: { kind: "wyrd_gateway", deployment: "jev" },
    run_config: { max_iterations: 1 },
  });
  const check = Check.agent(judge, { inputs: { context: "inputs.reply" }, verdict_transform: transform });
  const verifier = new Verifier("refund-promise", {
    version: "1.0.0",
    inputs: { reply: { type: "string" } },
    requirements: [
      new Requirement("no-refund-promise", "The reply makes no refund promise the policy does not allow.", check),
    ],
  });
  return new Service("refund-desk", { version: "1.0.0" }).addComponent(
    new Component("refund-judge", judge).verifiedBy([verifier]),
  );
}

// Typed classes, native literals, and a mix of both author one Prompt whose
// Choice answer is the verdict. The declared graph stores the native Jev
// request and the canonical transform, so redeclaring the native form
// changes nothing; each reply reaches Jev once as the bound state.
test("typed jev choice check becomes the verdict", async ({ provider, deployed: _ }) => {
  const [typed, native, mixed] = [
    TYPED_DESK_QUESTIONS,
    NATIVE_DESK_QUESTIONS,
    { ...NATIVE_DESK_QUESTIONS, helpful: TYPED_DESK_QUESTIONS.helpful, urgency: TYPED_DESK_QUESTIONS.urgency },
  ].map((questions) => Prompt.jev("jev-latest", { state: STATE, questions }));
  const transform: verify.ChoiceTransform = { kind: "choice", question: "refund_policy" };

  const state = await WyrdState.declare(refundDesk(typed, transform));
  const again = await WyrdState.declare(refundDesk(native, transform));
  expect(again.aliases.map((alias) => again.cardRef(alias))).toEqual(
    state.aliases.map((alias) => state.cardRef(alias)),
  );
  const mixedAgain = await WyrdState.declare(refundDesk(mixed, transform));
  expect(mixedAgain.cardRef("refund-judge")).toEqual(state.cardRef("refund-judge"));
  const cards = state.aliases.map((alias) => state.card(alias));
  const [verifier] = cards.filter((card) => card.kind === "Verifier");
  expect(verifier?.spec).toMatchObject({ requirements: [{ check: { verdict_transform: transform } }] });
  const prompts = cards.filter((card) => card.kind === "Prompt").map((card) => card.spec);
  expect(prompts.length).toBeGreaterThan(0);
  for (const spec of prompts) {
    expect(spec).toMatchObject({ body: { state: STATE, questions: NATIVE_DESK_QUESTIONS } });
  }

  for (const [reply, outcome] of [
    ['CASE-PASS: "Thanks" for asking.\nRefunds follow the policy.', "passed"],
    ["CASE-FAIL: you will get a full refund today.", "failed"],
    ["CASE-UNSURE: maybe.", "inconclusive"],
  ] as const) {
    const before = provider.received.length;
    const result = await state.run("refund-judge").observe.verify("refund-promise", { reply });

    const [judged, ...others] = result.outcome.requirements;
    expect(others).toEqual([]);
    expect({ outcome: judged?.outcome, reason: judged?.reason, citations: judged?.citations ?? [] }).toEqual({
      outcome,
      reason: `Question 'refund_policy' selected '${outcome}' (confidence 0.9).`,
      citations: [],
    });
    expect(result.outcome.verdict).toBe(outcome);
    const [upstream, ...rest] = provider.received.slice(before);
    expect(rest).toEqual([]);
    expect(upstream?.path).toBe("/v1/systemone");
    expect(JSON.parse(upstream?.body ?? "")).toEqual({
      model: "jev-latest",
      state: reply,
      questions: NATIVE_DESK_QUESTIONS,
    });
  }
});

/**
 * `policy-desk`: one Jev Agent's answer read by four acceptance policies.
 * `desk-policy` maps the custom `refund` labels, reads `helpful` expected
 * true and `unresolved` expected false, and requires `compliance >= 2`.
 */
function policyDesk(): verify.Service {
  const judge = new Agent("desk-judge", {
    version: "1.0.0",
    prompt: Prompt.jev("jev-latest", { state: STATE, questions: POLICY_QUESTIONS }),
    execution_route: { kind: "wyrd_gateway", deployment: "jev" },
    run_config: { max_iterations: 1 },
  });
  const refund: verify.ChoiceTransform = {
    kind: "choice",
    question: "refund",
    outcomes: { compliant: "passed", violation: "failed", unknown: "inconclusive" },
  };
  const helpful: verify.ProbabilityTransform = {
    kind: "probability",
    question: "helpful",
    expected: true,
    pass_at: 0.75,
    fail_at: 0.25,
  };
  const resolved: verify.ProbabilityTransform = {
    kind: "probability",
    question: "unresolved",
    expected: false,
    pass_at: 0.9,
    fail_at: 0.1,
  };
  const compliant: verify.ScoreTransform = {
    kind: "score",
    question: "compliance",
    operator: "greater_than_or_equals",
    expected: 2,
  };
  const verifier = new Verifier("desk-policy", {
    version: "1.0.0",
    inputs: { reply: { type: "string" } },
    requirements: (
      [
        ["refund-policy", "The reply follows the refund policy.", refund],
        ["helpful", "The reply helps the customer.", helpful],
        ["resolved", "The reply resolves the issue.", resolved],
        ["compliant", "The reply is at least highly compliant.", compliant],
      ] as const
    ).map(
      ([id, statement, transform]) =>
        new Requirement(
          id,
          statement,
          Check.agent(judge, { inputs: { context: "inputs.reply" }, verdict_transform: transform }),
        ),
    ),
  });
  return new Service("policy-desk", { version: "1.0.0" }).addComponent(
    new Component("desk-judge", judge).verifiedBy([verifier]),
  );
}

// Each acceptance policy reads its own question of one Jev answer: a custom
// Choice map assigns every label its outcome whatever the confidence, Noul
// bounds are inclusive for both polarities with an inconclusive band
// between, and a Score comparison passes at its bound and fails below it.
// The selected native answer is each requirement's actual value.
test.for([
  {
    reply: "DESK-PASS: the refund follows policy.",
    outcomes: ["passed", "passed", "passed", "passed"],
    verdict: "passed",
    reasons: [
      "Question 'refund' selected 'compliant' (confidence 0.3), which the policy maps to passed.",
      "Question 'helpful' answered true with probability 0.75; the expected answer true has probability 0.75, at least pass_at 0.75.",
      `Question 'unresolved' answered true with probability 0.02; the expected answer false has probability ${1 - 0.02}, at least pass_at 0.9.`,
      "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.",
    ],
  },
  {
    reply: "DESK-FAIL: you get a refund whatever the policy says.",
    outcomes: ["failed", "failed", "failed", "failed"],
    verdict: "failed",
    reasons: [
      "Question 'refund' selected 'violation' (confidence 0.99), which the policy maps to failed.",
      "Question 'helpful' answered true with probability 0.25; the expected answer true has probability 0.25, at most fail_at 0.25.",
      `Question 'unresolved' answered true with probability 0.98; the expected answer false has probability ${1 - 0.98}, at most fail_at 0.1.`,
      "Question 'compliance' scored 1.8 on levels 0 to 2; greater_than_or_equals 2 does not hold.",
    ],
  },
  {
    reply: "DESK-UNSURE: we will see.",
    outcomes: ["inconclusive", "inconclusive", "inconclusive", "passed"],
    verdict: "inconclusive",
    reasons: [
      "Question 'refund' selected 'unknown' (confidence 0.5), which the policy maps to inconclusive.",
      "Question 'helpful' answered true with probability 0.5; the expected answer true has probability 0.5, between fail_at 0.25 and pass_at 0.75.",
      "Question 'unresolved' answered true with probability 0.5; the expected answer false has probability 0.5, between fail_at 0.1 and pass_at 0.9.",
      "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.",
    ],
  },
])("jev policies judge $verdict answers", async ({ reply, outcomes, verdict, reasons }, { provider, deployed: _ }) => {
  const state = await WyrdState.declare(policyDesk());
  const before = provider.received.length;

  const result = await state.run("desk-judge").observe.verify("desk-policy", { reply });

  const answers = POLICY_ANSWERS[reply.split(":")[0] ?? ""];
  const judged = new Map(result.outcome.requirements.map((each) => [each.id, each]));
  const read = (
    [
      ["refund-policy", "refund"],
      ["helpful", "helpful"],
      ["resolved", "unresolved"],
      ["compliant", "compliance"],
    ] as const
  ).map(([id, question]) => {
    const each = judged.get(id);
    return {
      outcome: each?.outcome,
      reason: each?.reason,
      actual: each?.actual,
      citations: each?.citations ?? [],
      question,
    };
  });
  expect(read).toEqual(
    read.map(({ question }, index) => ({
      outcome: outcomes[index],
      reason: reasons[index],
      actual: answers?.[question],
      citations: [],
      question,
    })),
  );
  expect(result.outcome.verdict).toBe(verdict);
  const delivered = provider.received.slice(before);
  expect(delivered.length).toBeGreaterThan(0);
  for (const each of delivered) {
    expect({ path: each.path, body: JSON.parse(each.body) }).toEqual({
      path: "/v1/systemone",
      body: { model: "jev-latest", state: reply, questions: POLICY_QUESTIONS },
    });
  }
});
