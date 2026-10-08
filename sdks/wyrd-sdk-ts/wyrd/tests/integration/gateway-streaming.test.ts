// TypeScript's own surface: the OpenAI Node client's async stream relayed
// through the Gateway. Not one of the shared stories in `fixtures/README.md`.
import OpenAI from "openai";
import { expect, vi } from "vitest";

import { Gateway, WyrdClient } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

vi.stubEnv("WYRD_TEST_GATEWAY_PROVIDER_KEY", "sk-native-upstream");

/** The answer `hi` as Chat Completions Server-Sent Events, ending with `[DONE]`. */
const STREAMED = [
  { choices: [{ index: 0, delta: { role: "assistant", content: "h" } }] },
  { choices: [{ index: 0, delta: { content: "i" }, finish_reason: "stop" }] },
]
  .map((chunk) => ({ id: "chatcmpl-2", object: "chat.completion.chunk", created: 1, model: "gpt-4o", ...chunk }))
  .map((chunk) => `data: ${JSON.stringify(chunk)}\n\n`)
  .join("")
  .concat("data: [DONE]\n\n");

/** A server whose `openai` deployment of `gpt-4o` streams from the local upstream. */
const test = serverTest({
  provider: () => ({ status: 200, body: STREAMED, contentType: "text/event-stream" }),
}).extend<{ gpt4o: void }>({
  gpt4o: [
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
      await use();
    },
    { scope: "file" },
  ],
});

test("streamed answer relays through the gateway", async ({ gpt4o: _ }) => {
  const caller = WyrdClient.connect();
  const client = new OpenAI({ baseURL: `${caller.serverUrl}/v1`, apiKey: await caller.accessToken() });

  const deltas = [];
  for await (const chunk of await client.chat.completions.create({
    model: "openai/gpt-4o",
    max_completion_tokens: 16,
    messages: [{ role: "user", content: "hi" }],
    stream: true,
  })) {
    deltas.push(chunk.choices[0]?.delta.content);
  }

  // `join` writes a missing delta as nothing.
  expect(deltas.join("")).toBe("hi");
});
