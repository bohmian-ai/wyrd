import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Cards, Gateway, type ProviderDeployment, type RegistrationReceipt, Workflow, WyrdClient } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { fixture, registered, serverTest } from "../support/server.js";

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
const test = serverTest({ provider: () => ({ status: 200, body: COMPLETION }) }).extend<{
  ask: RegistrationReceipt;
}>({
  ask: [
    async ({ server: _ }, use) => {
      await cli.putProviderCredential({
        name: "openai-key",
        provider: "openai",
        source: { environment: { binding: "test-provider-key" } },
      });
      const gateway = Gateway.connect();
      await gateway.putDeployment(deployment("gpt-4o"));
      await gateway.putDeployment(deployment("gpt-5-5"));
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

test("caller without gateway invoke is refused", async ({ server, ask: _ }) => {
  const reader = WyrdClient.connect({ credential: server.scopedApiKey("gateway_reader", ["gateway:read"]) });
  const { client } = await openai(reader);

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
