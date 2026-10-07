import type { NativeWyrdTestServer } from "@wyrd/testing";
import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Cards, Gateway, type RegistrationReceipt, WyrdClient, cli } from "@wyrd/sdk";

import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { fixture, serverTest } from "../support/server.js";

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

/** The same answer as Server-Sent Events, ending with `[DONE]`. */
const STREAMED = [
  { choices: [{ index: 0, delta: { role: "assistant", content: "h" } }] },
  { choices: [{ index: 0, delta: { content: "i" }, finish_reason: "stop" }] },
]
  .map((chunk) => ({ id: "chatcmpl-2", object: "chat.completion.chunk", created: 1, model: "gpt-4o", ...chunk }))
  .map((chunk) => `data: ${JSON.stringify(chunk)}\n\n`)
  .join("")
  .concat("data: [DONE]\n\n");

/** Answer a Chat Completions request buffered or streamed, as it asked. */
function answer(request: ReceivedRequest): Reply {
  return (JSON.parse(request.body) as { stream?: boolean }).stream
    ? { status: 200, body: STREAMED, contentType: "text/event-stream" }
    : { status: 200, body: COMPLETION };
}

/** One user turn to `openai/gpt-4o`. */
const ASK = { model: "openai/gpt-4o", max_completion_tokens: 16, messages: [{ role: "user" as const, content: "hi" }] };

/**
 * A server whose `openai` deployment reaches the local upstream with the
 * operator's provider key, and the registered `ask` Workflow.
 */
const test = serverTest({ provider: answer }).extend<{ ask: RegistrationReceipt }>({
  ask: [
    async ({ server: _ }, use) => {
      await cli.putProviderCredential({
        name: "openai-key",
        provider: "openai",
        source: { environment: { binding: "test-provider-key" } },
      });
      await Gateway.connect().putDeployment({
        name: "gpt-4o",
        model: { provider: "openai", model: "gpt-4o" },
        adapter: "openai",
        auth: { bearer: { credential: "openai-key" } },
        capabilities: ["chat_completions"],
        routing_weight: 1,
      });
      await use(await Cards.connect().registerFromPath(fixture("cards/gateway_inference/ask.yaml")));
    },
    { scope: "file" },
  ],
});

/** An OpenAI client pointed at the Gateway with the caller's Wyrd access token. */
async function openai(server: NativeWyrdTestServer): Promise<{ client: OpenAI; token: string }> {
  const token = await WyrdClient.connect().accessToken();
  return { client: new OpenAI({ baseURL: `${server.baseUrl}/v1`, apiKey: token }), token };
}

test("openai client calls the gateway with an access token", async ({ server, provider, ask: _ }) => {
  const { client, token } = await openai(server);
  const before = provider.received.length;

  const reply = await client.chat.completions.create(ASK);

  expect(reply.choices[0]?.message.content).toBe("hi");
  expect(reply.usage?.total_tokens).toBe(15);
  const [upstream] = provider.received.slice(before);
  expect(upstream).toMatchObject({ path: "/v1/chat/completions", headers: { authorization: `Bearer ${PROVIDER_KEY}` } });
  expect(Object.values(upstream?.headers ?? {})).not.toContain(token);
  expect(Object.keys(upstream?.headers ?? {}).filter((name) => name.startsWith("x-wyrd"))).toEqual([]);
});

test("streamed answer relays through the gateway", async ({ server, ask: _ }) => {
  const { client } = await openai(server);

  let streamed = "";
  for await (const chunk of await client.chat.completions.create({ ...ASK, stream: true })) {
    streamed += chunk.choices[0]?.delta.content ?? "";
  }

  expect(streamed).toBe("hi");
});

test("caller without gateway invoke is refused", async ({ server, ask: _ }) => {
  vi.stubEnv("WYRD_API_KEY", server.scopedApiKey("gateway_reader", ["gateway:read"]));
  const { client } = await openai(server);

  await expect(client.chat.completions.create(ASK)).rejects.toMatchObject({
    status: 403,
    error: { code: "WYRD_PERMISSION_403_DENIED_RBAC" },
  });
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
  const workflow = await Cards.connect().workflow.load({ uid: ask.root.uid ?? "" });

  const run = await workflow.run({ question: "hi" });

  expect(run).toMatchObject({ status: "succeeded", outputs: { answer: "hi" } });
  expect(provider.received.slice(before)).toMatchObject([
    { path: "/v1/chat/completions", headers: { authorization: `Bearer ${PROVIDER_KEY}` } },
  ]);
});
