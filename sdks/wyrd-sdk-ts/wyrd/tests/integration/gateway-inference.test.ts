import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { AddressInfo } from "node:net";

import { startTestServer } from "@wyrd/testing";
import OpenAI, { APIError } from "openai";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { Gateway } from "@wyrd/sdk";

/** Operator credential the harness binds to `test-provider-key`. */
const PROVIDER_KEY = "sk-native-upstream";

/** Buffered Chat Completions answer the mock upstream returns. */
const COMPLETION = {
  id: "chatcmpl-1",
  object: "chat.completion",
  created: 1,
  model: "gpt-4o",
  choices: [
    { index: 0, message: { role: "assistant", content: "hi" }, finish_reason: "stop", logprobs: null },
  ],
  usage: { prompt_tokens: 11, completion_tokens: 4, total_tokens: 15 },
};

/** Streamed Chat Completions frames, ending with the `[DONE]` sentinel. */
const CHUNKS = [
  {
    id: "chatcmpl-2",
    object: "chat.completion.chunk",
    created: 1,
    model: "gpt-4o",
    choices: [{ index: 0, delta: { role: "assistant", content: "h" } }],
  },
  {
    id: "chatcmpl-2",
    object: "chat.completion.chunk",
    created: 1,
    model: "gpt-4o",
    choices: [{ index: 0, delta: { content: "i" }, finish_reason: "stop" }],
  },
];

/** One request the mock upstream served. */
type Received = { path: string; headers: Record<string, string> };

/** Requests the mock upstream served, in order. */
const received: Received[] = [];

/** Local mock OpenAI upstream every built-in adapter is rooted at. */
let upstream: Server;

/** Absolute root of {@link upstream}, which roots the harness adapters. */
let upstreamUrl: string;

/** Environment variable the harness operator credential bindings resolve. */
const PROVIDER_KEY_VARIABLE = "WYRD_TEST_GATEWAY_PROVIDER_KEY";

/** Value of {@link PROVIDER_KEY_VARIABLE} before this file published it. */
let priorProviderKey: string | undefined;

/** Answers one upstream request, recording its path and headers first. */
function serve(request: IncomingMessage, response: ServerResponse): void {
  const chunks: Buffer[] = [];
  request.on("data", (chunk: Buffer) => chunks.push(chunk));
  request.on("end", () => {
    const body = JSON.parse(Buffer.concat(chunks).toString()) as { stream?: boolean };
    received.push({
      path: request.url ?? "",
      headers: Object.fromEntries(
        Object.entries(request.headers).map(([name, value]) => [name, String(value)]),
      ),
    });
    if (body.stream) {
      const events = [...CHUNKS.map((chunk) => JSON.stringify(chunk)), "[DONE]"]
        .map((event) => `data: ${event}\n\n`)
        .join("");
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end(events);
      return;
    }
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify(COMPLETION));
  });
}

beforeAll(async () => {
  priorProviderKey = process.env[PROVIDER_KEY_VARIABLE];
  process.env[PROVIDER_KEY_VARIABLE] = PROVIDER_KEY;
  upstream = createServer(serve);
  await new Promise<void>((resolve) => upstream.listen(0, "127.0.0.1", resolve));
  const { port } = upstream.address() as AddressInfo;
  upstreamUrl = `http://127.0.0.1:${port}`;
});

afterAll(async () => {
  if (priorProviderKey === undefined) {
    delete process.env[PROVIDER_KEY_VARIABLE];
  } else {
    process.env[PROVIDER_KEY_VARIABLE] = priorProviderKey;
  }
  await new Promise<void>((resolve, reject) =>
    upstream.close((error) => (error ? reject(error) : resolve())),
  );
});

/** Exchanges an API key for a Wyrd access token through the public auth route. */
async function exchange(baseUrl: string, apiKey: string): Promise<string> {
  const response = await fetch(`${baseUrl}/auth/token`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ grant_type: "wyrd_api_key", api_key: apiKey }),
  });
  expect(response.status).toBe(200);
  return ((await response.json()) as { access_token: string }).access_token;
}

/**
 * Submit or delete one provider credential over the HTTP operation the CLI uses.
 *
 * The SDK exposes no credential mutation — submitting, rotating, revoking, and
 * deleting are CLI and scoped MCP paths, because a name-only revoke or delete
 * cannot be told apart from one aimed at a managed secret. A journey that needs
 * a credential in place therefore calls the public route directly, as the CLI
 * does, and the raw response lets a caller assert a conflict or a denial.
 */
async function credentialRequest(
  baseUrl: string,
  token: string,
  name: string,
  body?: unknown,
): Promise<Response> {
  return fetch(`${baseUrl}/v1/admin/gateway/provider-credentials/${name}`, {
    method: body === undefined ? "DELETE" : "PUT",
    headers: {
      "x-wyrd-access-token": `Bearer ${token}`,
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}

describe("Gateway inference journey", () => {
  it("relays buffered and streamed answers and refuses an unprivileged caller", async () => {
    const server = startTestServer(upstreamUrl);
    try {
      const gateway = Gateway.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });
      expect(
        (
          await credentialRequest(server.baseUrl, server.token, "ts-openai", {
            name: "ts-openai",
            provider: "openai",
            source: { environment: { binding: "test-provider-key" } },
          })
        ).status,
      ).toBe(200);
      await gateway.putDeployment({
        name: "ts-gpt-4o",
        model: { provider: "openai", model: "gpt-4o" },
        adapter: "openai",
        auth: { bearer: { credential: "ts-openai" } },
        capabilities: ["chat_completions"],
        routing_weight: 1,
      });

      const client = new OpenAI({ baseURL: `${server.baseUrl}/v1`, apiKey: server.token });
      const answer = await client.chat.completions.create({
        model: "openai/gpt-4o",
        max_completion_tokens: 16,
        messages: [{ role: "user", content: "hi" }],
      });
      expect(answer.choices[0]?.message.content).toBe("hi");
      expect(answer.usage?.total_tokens).toBe(15);

      const stream = await client.chat.completions.create({
        model: "openai/gpt-4o",
        max_completion_tokens: 16,
        messages: [{ role: "user", content: "hi" }],
        stream: true,
      });
      let streamed = "";
      let finish: string | null | undefined;
      for await (const chunk of stream) {
        streamed += chunk.choices[0]?.delta.content ?? "";
        finish = chunk.choices[0]?.finish_reason ?? finish;
      }
      expect(streamed).toBe("hi");
      expect(finish).toBe("stop");

      const readerKey = server.scopedApiKey("ts_gateway_inference_reader", ["gateway:read"]);
      const reader = new OpenAI({
        baseURL: `${server.baseUrl}/v1`,
        apiKey: await exchange(server.baseUrl, readerKey),
      });
      const refusal = await reader.chat.completions
        .create({
          model: "openai/gpt-4o",
          max_completion_tokens: 16,
          messages: [{ role: "user", content: "hi" }],
        })
        .then(
          () => undefined,
          (reason: unknown) => reason as APIError,
        );
      expect(refusal?.status).toBe(403);
      expect((refusal?.error as { code?: string } | undefined)?.code).toBe(
        "WYRD_PERMISSION_403_DENIED_RBAC",
      );

      expect(received.map((call) => call.path)).toEqual([
        "/v1/chat/completions",
        "/v1/chat/completions",
      ]);
      for (const call of received) {
        expect(call.headers.authorization).toBe(`Bearer ${PROVIDER_KEY}`);
        expect(Object.values(call.headers)).not.toContain(server.token);
        expect(Object.keys(call.headers).filter((name) => name.startsWith("x-wyrd"))).toEqual([]);
      }
    } finally {
      server.shutdown();
    }
  }, 120_000);
});
