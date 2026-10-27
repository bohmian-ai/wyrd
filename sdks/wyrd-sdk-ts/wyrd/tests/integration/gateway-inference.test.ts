import { randomBytes } from "node:crypto";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Agent, run, setTracingDisabled, tool } from "@openai/agents-core";
import { OpenAIResponsesModel } from "@openai/agents-openai";
import Anthropic from "@anthropic-ai/sdk";
import OpenAI from "openai";
import { expect, vi } from "vitest";
import { z } from "zod";

import {
  Bifrost,
  Cards,
  Gateway,
  Principals,
  type ProviderDeployment,
  type DeclarationReceipt,
  WyrdClient,
  WyrdError,
  WyrdState,
} from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { ancestry, columnValues, fixture, declared, runtimeAccount, serverTest, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The provider key the server's `test-provider-key` binding resolves. */
const PROVIDER_KEY = "sk-native-upstream";
vi.stubEnv("WYRD_TEST_GATEWAY_PROVIDER_KEY", PROVIDER_KEY);

/** Buffered Chat Completions answer the upstream returns. */
const COMPLETION = {
  id: "chatcmpl-1",
  object: "chat.completion",
  created: 1,
  model: "gpt-4o",
  choices: [{ index: 0, message: { role: "assistant", content: "hi" }, finish_reason: "stop", logprobs: null }],
  usage: { prompt_tokens: 11, completion_tokens: 4, total_tokens: 15 },
};

/** Buffered Anthropic Messages answer the upstream returns. */
const ANTHROPIC_MESSAGE = {
  id: "msg_1",
  type: "message",
  role: "assistant",
  model: "claude-sonnet-5",
  content: [{ type: "text", text: "hi" }],
  stop_reason: "end_turn",
  stop_sequence: null,
  usage: { input_tokens: 5, output_tokens: 3 },
};

/** Responses usage the upstream reports. */
/** {@link ANTHROPIC_MESSAGE} as the Anthropic Messages event stream a `stream: true` request receives. */
const ANTHROPIC_STREAM = [
  ["message_start", { type: "message_start", message: { ...ANTHROPIC_MESSAGE, content: [], stop_reason: null } }],
  ["content_block_start", { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } }],
  ["content_block_delta", { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "hi" } }],
  ["content_block_stop", { type: "content_block_stop", index: 0 }],
  [
    "message_delta",
    { type: "message_delta", delta: { stop_reason: "end_turn", stop_sequence: null }, usage: { output_tokens: 3 } },
  ],
  ["message_stop", { type: "message_stop" }],
]
  .map(([name, data]) => `event: ${String(name)}\ndata: ${JSON.stringify(data)}\n\n`)
  .join("");

const RESPONSE_USAGE = {
  input_tokens: 5,
  output_tokens: 3,
  total_tokens: 8,
  input_tokens_details: { cached_tokens: 0 },
  output_tokens_details: { reasoning_tokens: 0 },
};

/** A buffered Responses answer carrying `output`. */
function response(output: unknown[]): unknown {
  return {
    id: "resp_1",
    object: "response",
    created_at: 1,
    model: "gpt-4o-mini",
    status: "completed",
    output,
    usage: RESPONSE_USAGE,
  };
}

/**
 * The upstream's Responses turn: an `echo` call until the input carries its
 * result, then the final answer.
 */
function responsesTurn(body: string): unknown {
  const answered = (JSON.parse(body) as { input: { type?: string }[] }).input.some(
    (item) => item.type === "function_call_output",
  );
  return answered
    ? response([
        {
          type: "message",
          id: "msg_1",
          role: "assistant",
          status: "completed",
          content: [{ type: "output_text", text: "echoed", annotations: [] }],
        },
      ])
    : response([
        {
          type: "function_call",
          id: "fc_1",
          call_id: "c1",
          name: "echo",
          arguments: '{"text":"hi"}',
          status: "completed",
        },
      ]);
}

/** One user turn to `openai/gpt-4o`. */
const ASK = { model: "openai/gpt-4o", max_completion_tokens: 16, messages: [{ role: "user" as const, content: "hi" }] };

/** An OpenAI deployment named `name` that authenticates with the `openai-key` credential. */
function openaiDeployment(name: string, access: ProviderDeployment["access"] = "open"): ProviderDeployment {
  return { name, adapter: "openai", auth: { bearer: { credential: "openai-key" } }, access };
}

/**
 * A server whose `openai` and `anthropic` deployments reach the local upstream
 * with the operator's provider keys, call metadata capture, and the registered
 * `ask-agent` Agent.
 */
const test = serverTest({
  provider: ({ path, body }) =>
    path === "/v1/messages" && body.includes('"stream":true')
      ? { status: 200, body: ANTHROPIC_STREAM, contentType: "text/event-stream" }
      : {
          status: 200,
          body:
            path === "/v1/responses" ? responsesTurn(body) : path === "/v1/messages" ? ANTHROPIC_MESSAGE : COMPLETION,
        },
}).extend<{
  ask: DeclarationReceipt;
}>({
  ask: [
    async ({ server: _ }, use) => {
      await cli.putProviderCredential({
        name: "openai-key",
        provider: "openai",
        source: { environment: { binding: "test-provider-key" } },
      });
      await cli.putProviderCredential({
        name: "anthropic-key",
        provider: "anthropic",
        source: { environment: { binding: "test-anthropic-key" } },
      });
      const gateway = await Gateway.connect();
      await gateway.putDeployment(openaiDeployment("openai"));
      await gateway.putDeployment({
        name: "anthropic",
        adapter: "anthropic",
        auth: { api_key_header: { header: "x-api-key", credential: "anthropic-key" } },
      });
      await gateway.putCapturePolicy({ mode: "metadata", payload_fields: [] });
      await use(await (await Cards.connect()).declareFromPath(fixture("cards/gateway_inference/ask-agent.yaml")));
    },
    { scope: "file" },
  ],
});

/** An OpenAI client pointed at the Gateway with `caller`'s Wyrd access token. */
async function openai(caller: WyrdClient): Promise<{ client: OpenAI; token: string }> {
  const token = await caller.accessToken();
  return { client: new OpenAI({ baseURL: `${caller.serverUrl}/v1`, apiKey: token }), token };
}

test("openai client calls the gateway with an access token", async ({ provider, ask: _ }) => {
  const { client, token } = await openai(WyrdClient.connect());
  const before = provider.received.length;

  const reply = await client.chat.completions.create(ASK);

  expect(reply.choices[0]?.message.content).toBe("hi");
  expect(reply.usage?.total_tokens).toBe(15);
  const [upstream] = provider.received.slice(before);
  expect(upstream).toMatchObject({
    path: "/v1/chat/completions",
    headers: { authorization: `Bearer ${PROVIDER_KEY}` },
  });
  expect(Object.values(upstream?.headers ?? {})).not.toContain(token);
  expect(Object.keys(upstream?.headers ?? {}).filter((name) => name.startsWith("x-wyrd"))).toEqual([]);
});

/** The gateway's model-call spans in the trace `traceId`, after publishing every captured row. */
test("gateway span joins the caller's trace", async ({ server, ask: _ }) => {
  const [traceId, spanId] = [randomBytes(16).toString("hex"), randomBytes(8).toString("hex")];
  const token = await WyrdClient.connect().accessToken();
  const client = new OpenAI({
    baseURL: `${WyrdClient.connect().serverUrl}/v1`,
    apiKey: token,
    defaultHeaders: { traceparent: `00-${traceId}-${spanId}-01`, tracestate: "wyrd=journey" },
  });

  const reply = await client.chat.completions.create(ASK);

  expect(reply.choices[0]?.message.content).toBe("hi");
  const spans = await timeline(server, traceId);
  const attempts = spans.filter((span) => span.scope.name === "wyrd.gateway");
  expect(attempts).toHaveLength(1);
  expect(ancestry(spans, attempts[0]).at(-1)?.parent_span_id).toBe(spanId);
  expect(attempts[0].trace_state).toBe("wyrd=journey");
});

test("malformed traceparent is ignored by the gateway", async ({ server, ask: _ }) => {
  const traceId = randomBytes(16).toString("hex");
  const token = await WyrdClient.connect().accessToken();
  const client = new OpenAI({
    baseURL: `${WyrdClient.connect().serverUrl}/v1`,
    apiKey: token,
    defaultHeaders: { traceparent: `00-${traceId}-not-a-span-01` },
  });

  const reply = await client.chat.completions.create(ASK);

  expect(reply.choices[0]?.message.content).toBe("hi");
  expect(await timeline(server, traceId)).toEqual([]);
});

// Each client keeps a placeholder provider key; `httpAuth` drops it and sends
// the ambient client's current access token, so the gateway sees one credential.
test("stock openai client authenticates with httpAuth alone", async ({ provider, ask: _ }) => {
  const wyrd = WyrdClient.connect();
  const client = new OpenAI({ baseURL: `${wyrd.serverUrl}/v1`, apiKey: "wyrd", fetch: wyrd.httpAuth });
  const before = provider.received.length;

  for (let turn = 0; turn < 2; turn++) {
    const reply = await client.chat.completions.create(ASK);
    expect(reply.choices[0]?.message.content).toBe("hi");
  }
  for (const upstream of provider.received.slice(before)) {
    expect(upstream.headers).toMatchObject({ authorization: `Bearer ${PROVIDER_KEY}` });
  }
});

test("stock anthropic client authenticates with httpAuth alone", async ({ provider, ask: _ }) => {
  const wyrd = WyrdClient.connect();
  const client = new Anthropic({ baseURL: wyrd.serverUrl, apiKey: "wyrd", fetch: wyrd.httpAuth, maxRetries: 0 });
  const before = provider.received.length;

  const message = await client.messages.create({
    model: "anthropic/claude-sonnet-5",
    max_tokens: 16,
    messages: [{ role: "user", content: "hi" }],
  });

  expect(message.content).toEqual([{ type: "text", text: "hi" }]);
  const [upstream] = provider.received.slice(before);
  expect(upstream).toMatchObject({ path: "/v1/messages", headers: { "x-api-key": PROVIDER_KEY } });
});

// No transport hook and no Wyrd call: the static key from the environment
// is the client's own API key, and the gateway exchanges it for the caller.
test("stock openai client authenticates with a Wyrd API key", async ({ provider, ask: _ }) => {
  const apiKey = process.env.WYRD_API_KEY ?? "";
  const client = new OpenAI({ baseURL: `${process.env.WYRD_SERVER_URL}/v1`, apiKey, maxRetries: 0 });
  const before = provider.received.length;

  expect(apiKey).toMatch(/^wyrd_sk_/);
  for (let turn = 0; turn < 2; turn++) {
    const reply = await client.chat.completions.create(ASK);
    expect(reply.choices[0]?.message.content).toBe("hi");
  }
  const upstream = provider.received.slice(before);
  expect(upstream).toHaveLength(2);
  for (const call of upstream) {
    expect(call.headers).toMatchObject({ authorization: `Bearer ${PROVIDER_KEY}` });
    expect(Object.values(call.headers).join(" ")).not.toContain(apiKey);
  }
});

// The status of the refusal is not pinned: only that the client's own token
// exchange rejected the request, so no gateway call was made.
test("revoked api key is refused by the token exchange, before the gateway", async ({ provider, ask: _ }) => {
  const principals = await Principals.connect();
  const { principal_id, credential } = await principals.createServicePrincipal({
    name: "revoked-gateway-caller",
    roles: [],
  });
  await principals.revokePrincipal(principal_id, { principal_kind: "service", reason: "journey revokes this key" });
  vi.stubEnv("WYRD_API_KEY", credential);
  const client = new OpenAI({
    baseURL: `${process.env.WYRD_SERVER_URL}/v1`,
    apiKey: "wyrd",
    fetch: WyrdClient.connect().httpAuth,
    maxRetries: 0,
  });
  const before = provider.received.length;

  const refusal = await client.chat.completions.create(ASK).catch((reason: unknown) => reason);

  expect((refusal as { cause?: unknown }).cause).toBeInstanceOf(WyrdError);
  expect(provider.received.length).toBe(before);
});

test("caller without gateway invoke is refused", async ({ server, ask: _ }) => {
  const reader = WyrdClient.connect({ credential: server.scopedApiKey("gateway_reader", ["gateway:read"]) });
  const { client } = await openai(reader);

  await expect(client.chat.completions.create(ASK)).rejects.toMatchObject({
    status: 403,
    error: { code: "WYRD_PERMISSION_403_DENIED_RBAC" },
  });
});

test("gateway call records a lone run or card", async ({ server, ask }) => {
  const service = await (await Cards.connect()).declareFromPath(fixture("cards/gateway_inference/ask-service.yaml"));
  const cardUid = declared(ask, "ask-agent").uid;
  const issued = await runtimeAccount({ kind: "Service", name: "ask-service" });
  const caller = WyrdClient.connect({ credential: issued.key }).forWorkload(declared(service).uid);
  const client = new OpenAI({ baseURL: `${caller.serverUrl}/v1`, apiKey: "wyrd", fetch: caller.httpAuth });

  for (const headers of [{ "wyrd-run-id": "run-alone" }, { "wyrd-card-uid": cardUid }]) {
    const reply = await client.chat.completions.create(ASK, { headers });
    expect(reply.choices[0]?.message.content).toBe("hi");
  }

  server.flushBifrost();
  const calls = (
    await (
      await Bifrost.connect()
    ).sql("SELECT run_id, subject_card_uid FROM bifrost.gateway.calls WHERE sub_principal_id = $1 ORDER BY run_id", [
      issued.principalId,
    ])
  ).toArrow();
  expect(columnValues(calls.getChild("run_id"))).toEqual(["run-alone", null]);
  expect(columnValues(calls.getChild("subject_card_uid"))).toEqual([null, cardUid]);
});

test("state httpAuth supplies service and agent context", async ({ server, provider, ask: _ }) => {
  const cards = await Cards.connect();
  const { root } = await cards.declareFromPath(fixture("cards/observe_a_run/configured-service.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-http-auth-"));
  await cards.pull(root, bundle);
  const issued = await runtimeAccount({ kind: "Service", name: "configured-service" });
  const configured = (environment: string): WyrdState => {
    const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: issued.key }) });
    state.configure({ deploymentEnvironment: environment });
    return state;
  };
  const [prod, staging] = [configured("prod"), configured("staging")];
  const [service, agent] = [prod.rootRef.uid, prod.cardRef("agent").uid];
  const openaiOf = (state: WyrdState): OpenAI =>
    new OpenAI({
      baseURL: `${WyrdClient.connect().serverUrl}/v1`,
      apiKey: "wyrd",
      fetch: state.httpAuth,
      maxRetries: 0,
    });
  const anthropic = new Anthropic({
    baseURL: WyrdClient.connect().serverUrl,
    apiKey: "wyrd",
    fetch: prod.httpAuth,
    maxRetries: 0,
  });
  const before = provider.received.length;

  await openaiOf(prod).chat.completions.create(ASK);
  const serviceRun = prod.run();
  await serviceRun.scope(() => openaiOf(prod).chat.completions.create(ASK));
  const agentRun = prod.run("agent");
  await agentRun.scope(() =>
    staging.run().scope(async () => {
      const stream = anthropic.messages.stream({
        model: "anthropic/claude-sonnet-5",
        max_tokens: 16,
        messages: ASK.messages,
      });
      expect(await stream.finalText()).toBe("hi");
    }),
  );
  const concurrentRuns = await Promise.all(
    [prod, staging].map((state) => {
      const run = state.run("agent");
      return run.scope(async () => {
        await openaiOf(state).chat.completions.create(ASK);
        return run.runId;
      });
    }),
  );

  // The provider SDK wraps the local refusal as its connection error's cause.
  const delivered = provider.received.length;
  await expect(
    prod.run().scope(() => openaiOf(prod).chat.completions.create(ASK, { headers: { "wyrd-card-uid": agent } })),
  ).rejects.toMatchObject({ cause: { code: "WYRD_CLIENT_400_CONFIG_INVALID" } });
  expect(provider.received.length).toBe(delivered);
  for (const upstream of provider.received.slice(before)) {
    const names = Object.keys(upstream.headers);
    expect(names.filter((name) => name.startsWith("x-wyrd") || name.startsWith("wyrd-"))).toEqual([]);
    expect(Object.values(upstream.headers)).not.toContain(issued.key);
  }

  server.flushBifrost();
  const columns = [
    "subject_card_uid",
    "run_id",
    "deployment_environment",
    "workload_card_uid",
    "service_name",
    "service_version",
    "subject_kind",
    "subject_name",
    "subject_version",
  ];
  const calls = (
    await (
      await Bifrost.connect()
    ).sql(`SELECT ${columns.join(", ")} FROM bifrost.gateway.calls WHERE sub_principal_id = $1 ORDER BY started_at`, [
      issued.principalId,
    ])
  ).toArrow();
  const values = columns.map((name) => columnValues(calls.getChild(name)) as unknown[]);
  const rows = (values[0] ?? []).map((_, index) => values.map((column) => column[index]));
  const owner = [service, "configured-service", "1.0.0"];
  const asService = [...owner, "Service", "configured-service", "1.0.0"];
  const asAgent = [...owner, "Agent", "observed-agent", "1.0.0"];
  expect(rows.slice(0, 3)).toEqual([
    [service, null, "prod", ...asService],
    [service, serviceRun.runId, "prod", ...asService],
    [agent, agentRun.runId, "prod", ...asAgent],
  ]);
  expect(rows.slice(3).sort()).toEqual(
    [
      [agent, concurrentRuns[0], "prod", ...asAgent],
      [agent, concurrentRuns[1], "staging", ...asAgent],
    ].sort(),
  );
});

/** An Agents SDK agent on the Gateway's Responses API whose `echo` tool returns content parts. */
function echoAgent(client: OpenAI): Agent {
  setTracingDisabled(true);
  const echo = tool({
    name: "echo",
    description: "Echo the text.",
    parameters: z.object({ text: z.string() }),
    execute: async ({ text }) => ({ type: "text" as const, text }),
  });
  return new Agent({
    name: "echo-agent",
    instructions: "Call echo.",
    model: new OpenAIResponsesModel(client, "openai/gpt-4o-mini"),
    tools: [echo],
  });
}

test("agents sdk responses tool loop runs through the gateway", async ({ provider, ask: _ }) => {
  const { client } = await openai(WyrdClient.connect());
  const before = provider.received.length;

  const result = await run(echoAgent(client), "go");

  expect(result.finalOutput).toBe("echoed");
  const upstream = provider.received.slice(before);
  expect(upstream.map(({ path }) => path)).toEqual(["/v1/responses", "/v1/responses"]);
  const [first, second] = upstream.map(({ body }) => JSON.parse(body) as { model: string; input: unknown[] });
  expect(first?.model).toBe("gpt-4o-mini");
  expect(first?.input).toEqual([{ role: "user", content: "go" }]);
  expect(second?.input).toContainEqual({ role: "user", content: "go" });
  expect(second?.input).toContainEqual(
    expect.objectContaining({
      type: "function_call_output",
      call_id: "c1",
      output: [{ type: "input_text", text: "hi" }],
    }),
  );
});

test("caller without gateway invoke cannot run an agents sdk agent", async ({ server, provider, ask: _ }) => {
  // Each scoped key mints a principal named for its role on this file's server,
  // so this caller's role must differ from the earlier refused caller's.
  const reader = WyrdClient.connect({ credential: server.scopedApiKey("agent_reader", ["gateway:read"]) });
  const { client } = await openai(reader);
  const before = provider.received.length;

  await expect(run(echoAgent(client), "go")).rejects.toMatchObject({ status: 403 });
  expect(provider.received.slice(before)).toEqual([]);
});

test("cli writes a provider credential", async ({ ask: _ }) => {
  const view = await cli.putProviderCredential({
    name: "managed-openai-key",
    provider: "openai",
    source: { managed_secret: { secret: "sk-managed" } },
  });

  expect(view).toMatchObject({ name: "managed-openai-key", provider: "openai", state: "active" });
  expect(JSON.stringify(view)).not.toContain("sk-managed");
});

/** The portable `{{question}}` or native `${question}` placeholder of an authored Prompt. */
const QUESTION = /\{\{question\}\}|\$\{question\}/g;

/**
 * Ask the `alias` Agent of `state` `question` on `deployment` as `caller`, the
 * way an application calls its model: the Agent's authored Prompt messages,
 * with `question` inserted literally for the portable `{{question}}` or the
 * native `${question}` placeholder, sent to its model addressed at
 * `deployment` through the stock OpenAI client, correlated to a fresh Run of
 * the Agent with the `wyrd-run-id` and `wyrd-card-uid` headers.
 */
async function askAgent(
  state: WyrdState,
  caller: WyrdClient,
  alias: string,
  deployment: string,
  question: string,
): Promise<string> {
  const { spec } = state.agent(alias).prompt();
  const authored = spec.messages ?? ((spec.body?.messages ?? []) as readonly { role: string; content: string }[]);
  const messages = authored.map(({ role, content }) => ({
    role,
    content: String(content).replace(QUESTION, () => question),
  })) as OpenAI.ChatCompletionMessageParam[];
  const run = state.run(alias);
  const client = new OpenAI({
    baseURL: `${caller.serverUrl}/v1`,
    apiKey: "wyrd",
    fetch: caller.httpAuth,
    maxRetries: 0,
  });
  const completion = await client.chat.completions.create(
    { model: `${deployment}/${spec.model}`, messages },
    { headers: { "wyrd-run-id": run.runId, "wyrd-card-uid": state.cardRef(alias).uid ?? "" } },
  );
  return completion.choices[0]?.message.content ?? "";
}

test("agent prompt routes", async ({ server, provider, ask: _ }) => {
  const gateway = await Gateway.connect();
  await gateway.putDeployment(openaiDeployment("openai-backup"));
  await gateway.putDeployment(openaiDeployment("openai-granted", "granted"));
  const cards = await Cards.connect();
  const receipt = await cards.declareFromPath(fixture("cards/gateway_inference/ask-service.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-ask-"));
  await cards.pull(receipt.root, bundle);
  const issued = await runtimeAccount({ kind: "Service", name: "ask-service" });
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: issued.key }) });
  const caller = WyrdClient.connect({ credential: issued.key }).forWorkload(declared(receipt).uid);
  const before = provider.received.length;
  // The Service family's one runtime account is shared with earlier tests in this file.
  const deployments = async (): Promise<unknown[]> => {
    server.flushBifrost();
    const calls = (
      await (
        await Bifrost.connect()
      ).sql(
        "SELECT requested_model.deployment AS deployment FROM bifrost.gateway.calls WHERE sub_principal_id = $1 ORDER BY started_at",
        [issued.principalId],
      )
    ).toArrow();
    return columnValues(calls.getChild("deployment")) as unknown[];
  };
  const earlier = (await deployments()).length;

  const asked: string[] = [];
  for (const alias of ["agent", "body-agent"]) {
    const { execution_route: route } = state.agent(alias).spec;
    expect(route).toEqual({ kind: "wyrd_gateway", deployment: "openai" });
    for (const deployment of [route.kind === "wyrd_gateway" ? route.deployment : "", "openai-backup"]) {
      const question = `${alias} on ${deployment}`;
      asked.push(question);
      expect(await askAgent(state, caller, alias, deployment, question)).toBe("hi");
    }
  }
  const upstream = provider.received
    .slice(before)
    .map(({ body }) => JSON.parse(body) as { model: string; messages: unknown[] });
  expect(upstream.map(({ model }) => model)).toEqual(Array(4).fill("gpt-4o"));
  expect(upstream.map(({ messages }) => messages)).toEqual(asked.map((content) => [{ role: "user", content }]));

  expect((await deployments()).slice(earlier)).toEqual(["openai", "openai-backup", "openai", "openai-backup"]);

  await expect(askAgent(state, caller, "agent", "openai-granted", "denied")).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
  expect(provider.received.length - before).toBe(4);
});
