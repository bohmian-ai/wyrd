import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { Agent, run, setTracingDisabled, tool } from "@openai/agents-core";
import { OpenAIResponsesModel } from "@openai/agents-openai";
import Anthropic from "@anthropic-ai/sdk";
import OpenAI from "openai";
import { expect, vi } from "vitest";
import { z } from "zod";

import {
  Cards,
  Gateway,
  type ProviderDeployment,
  type RegistrationReceipt,
  Workflow,
  WyrdClient,
  WyrdError,
} from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { fixture, registered, revokedApiKey, serverTest } from "../support/server.js";

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

/** The code-review example Workflow directory; its Prompts call `gpt-5-5` through the Wyrd gateway. */
const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/workflows/code-review");

/** The example's own input. */
const EXAMPLE_INPUT = JSON.parse(readFileSync(resolve(EXAMPLE, "input.json"), "utf8")) as { code: string };

/** One user turn to `openai/gpt-4o`. */
const ASK = { model: "openai/gpt-4o", max_completion_tokens: 16, messages: [{ role: "user" as const, content: "hi" }] };

/** An `openai` deployment of `model` that authenticates with the `openai-key` credential. */
function deployment(model: string): ProviderDeployment {
  return {
    name: model,
    model: { provider: "openai", model },
    adapter: "openai",
    auth: { bearer: { credential: "openai-key" } },
    capabilities: ["chat_completions"],
    routing_weight: 1,
  };
}

/**
 * A server whose `openai` deployments of `gpt-4o` and `gpt-5-5` reach the
 * local upstream with the operator's provider key, and the registered `ask`
 * Workflow.
 */
const test = serverTest({
  provider: ({ path, body }) => ({
    status: 200,
    body: path === "/v1/responses" ? responsesTurn(body) : path === "/v1/messages" ? ANTHROPIC_MESSAGE : COMPLETION,
  }),
}).extend<{
  ask: RegistrationReceipt;
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
      const gateway = Gateway.connect();
      await gateway.putDeployment(deployment("gpt-4o"));
      await gateway.putDeployment({
        name: "claude-sonnet-5",
        model: { provider: "anthropic", model: "claude-sonnet-5" },
        adapter: "anthropic",
        auth: { api_key_header: { header: "x-api-key", credential: "anthropic-key" } },
        capabilities: ["chat_completions"],
        routing_weight: 1,
      });
      await gateway.putDeployment(deployment("gpt-5-5"));
      await gateway.putDeployment({ ...deployment("gpt-4o-mini"), capabilities: ["responses"] });
      await use(await Cards.connect().registerFromPath(fixture("cards/gateway_inference/ask.yaml")));
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
    model: "claude-sonnet-5",
    max_tokens: 16,
    messages: [{ role: "user", content: "hi" }],
  });

  expect(message.content).toEqual([{ type: "text", text: "hi" }]);
  const [upstream] = provider.received.slice(before);
  expect(upstream).toMatchObject({ path: "/v1/messages", headers: { "x-api-key": PROVIDER_KEY } });
});

// The status of the refusal is not pinned: only that the client's own token
// exchange rejected the request, so no gateway call was made.
test("revoked api key is refused by the token exchange, before the gateway", async ({ provider, ask: _ }) => {
  vi.stubEnv("WYRD_API_KEY", await revokedApiKey("revoked-gateway-caller"));
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
  const reader = WyrdClient.connect({ credential: server.scopedApiKey("gateway_reader", ["gateway:read"]) });
  const { client } = await openai(reader);
  const before = provider.received.length;

  await expect(run(echoAgent(client), "go")).rejects.toMatchObject({ status: 403 });
  expect(provider.received.slice(before)).toEqual([]);
});

test("cli issues a card scoped key and writes a provider credential", async ({ ask: _ }) => {
  const agent = { kind: "Agent", name: "ask-agent", version: "1.0.0", space: "default" } as const;
  const issued = await cli.issueKey(agent);
  const view = await cli.putProviderCredential({
    name: "managed-openai-key",
    provider: "openai",
    source: { managed_secret: { secret: "sk-managed" } },
  });

  expect(issued.card_ref).toEqual(agent);
  expect(issued.key.startsWith(issued.prefix)).toBe(true);
  expect(view).toMatchObject({ name: "managed-openai-key", provider: "openai", state: "active" });
  expect(JSON.stringify(view)).not.toContain("sk-managed");
});

test("loaded workflow calls the gateway through its loading client", async ({ provider, ask }) => {
  const before = provider.received.length;
  const workflow = await Cards.connect().workflow.load({ uid: registered(ask).uid });

  const run = await workflow.run({ question: "hi" });

  expect(run).toMatchObject({ status: "succeeded", outputs: { answer: "hi" } });
  expect(provider.received.slice(before)).toMatchObject([
    { path: "/v1/chat/completions", headers: { authorization: `Bearer ${PROVIDER_KEY}` } },
  ]);
});

test("example workflow runs through the wyrd gateway", async ({ provider, ask: _ }) => {
  const before = provider.received.length;
  const example = await Workflow.fromPath(resolve(EXAMPLE, "workflow.yaml"));

  const run = await example.run(EXAMPLE_INPUT);

  expect(run).toMatchObject({ status: "succeeded", outputs: { review: "hi" } });
  expect(provider.received.slice(before)).toHaveLength(3);
});

test("applying a workflow calls no model", async ({ provider, ask: _ }) => {
  const before = provider.received.length;

  const applied = await cli.apply(EXAMPLE);

  expect(applied.root).toMatchObject({ kind: "Workflow", space: "engineering", name: "code-review" });
  expect(provider.received.slice(before)).toEqual([]);
});

test("registered example runs through the gateway", async ({ provider, ask: _ }) => {
  await cli.apply(EXAMPLE);
  const before = provider.received.length;
  const registeredExample = await Cards.connect().workflow.load({
    space: "engineering",
    name: "code-review",
    version: "1.0.0",
  });

  const run = await registeredExample.run(EXAMPLE_INPUT);

  expect(run).toMatchObject({ status: "succeeded", outputs: { review: "hi" } });
  expect(provider.received.slice(before)).toHaveLength(3);
});
