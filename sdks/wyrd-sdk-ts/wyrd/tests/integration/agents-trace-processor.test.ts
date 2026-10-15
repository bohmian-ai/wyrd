// OpenAI Agents SDK runs, traced through `WyrdAgentsTraceProcessor`, read back
// from Bifrost. The agent's stock OpenAI Chat Completions model calls a
// scripted local upstream, directly or through the Wyrd gateway, and the
// application is configured only through the environment: the server URL and
// the `observed-service` key.
import { context, propagation, type Span, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { Agent, run, setTraceProcessors, setTracingDisabled, tool } from "@openai/agents-core";
import { OpenAIChatCompletionsModel } from "@openai/agents-openai";
import OpenAI from "openai";
import { afterEach, expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, type SpanRecord, WyrdAgentsTraceProcessor, WyrdClient, WyrdError, WyrdState } from "wyrd-ai";
import { runCorrelationProcessor, spanExporter } from "wyrd-ai/otel";
import type { NativeWyrdTestServer } from "@wyrd/testing";

import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { observedService } from "../support/observed-service.js";
import { ancestry } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** The scripted model's name; its spans carry it as `gen_ai.request.model`. */
const MODEL = "local-model";
/** Sentinel prompt content no span may carry. */
const PROMPT = "SECRET-PROMPT";
/** Sentinel tool output no span may carry. */
const TOOL_RESULT = "SECRET-RESULT";

/**
 * A scripted Chat Completions upstream. A first turn offering tools calls
 * `lookup`; every later turn answers `done`, with 7 input and 2 output
 * tokens. A request naming `FAIL-MODEL` fails with 500, and one naming
 * `FAIL-TOOL` passes that topic to the tool, which throws.
 */
function scripted(request: ReceivedRequest): Reply {
  if (request.body.includes("FAIL-MODEL")) {
    return { status: 500, body: { error: { message: "upstream failed" } } };
  }
  const body = JSON.parse(request.body) as {
    model: string;
    stream?: boolean;
    tools?: unknown[];
    messages: { role: string }[];
  };
  const topic = request.body.includes("FAIL-TOOL") ? "FAIL-TOOL" : "x";
  const calling = body.tools !== undefined && !body.messages.some((message) => message.role === "tool");
  const call = { id: "call_1", type: "function", function: { name: "lookup", arguments: JSON.stringify({ topic }) } };
  const message = calling
    ? { role: "assistant", content: null, tool_calls: [call] }
    : { role: "assistant", content: "done" };
  const finish = calling ? "tool_calls" : "stop";
  const usage = { prompt_tokens: 7, completion_tokens: 2, total_tokens: 9 };
  const head = { id: "c", created: 1, model: body.model };
  if (!body.stream) {
    return {
      status: 200,
      body: { ...head, object: "chat.completion", choices: [{ index: 0, message, finish_reason: finish }], usage },
    };
  }
  const delta = calling ? { ...message, tool_calls: [{ index: 0, ...call }] } : message;
  const chunk = { ...head, object: "chat.completion.chunk" };
  const events = [
    { ...chunk, choices: [{ index: 0, delta, finish_reason: null }] },
    { ...chunk, choices: [{ index: 0, delta: {}, finish_reason: finish }] },
    { ...chunk, choices: [], usage },
  ];
  const stream = events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("");
  return { status: 200, body: `${stream}data: [DONE]\n\n`, contentType: "text/event-stream" };
}

const test = observedService({ provider: scripted });

/** Look a topic up; `FAIL-TOOL` throws. */
const lookup = tool({
  name: "lookup",
  description: "Look a topic up.",
  parameters: z.object({ topic: z.string() }),
  execute: async ({ topic }) => {
    if (topic === "FAIL-TOOL") {
      throw new Error("lookup failed");
    }
    return `${TOOL_RESULT} ${topic}`;
  },
});

/** One prompt to the agent, as the application sends it. */
type Ask = (prompt: string, stream?: boolean) => Promise<string>;

/** An agent on `client`'s `model` whose traces the Wyrd processor alone receives. */
function agent(client: OpenAI, model: string = MODEL): Ask {
  setTracingDisabled(false);
  setTraceProcessors([new WyrdAgentsTraceProcessor()]);
  const assistant = new Agent({
    name: "assistant",
    model: new OpenAIChatCompletionsModel(client, model),
    tools: [lookup],
  });
  return async (prompt, stream = false) => {
    if (!stream) {
      return String((await run(assistant, prompt)).finalOutput);
    }
    const streamed = await run(assistant, prompt, { stream: true });
    for await (const _ of streamed) {
      // Consume every event so the run completes.
    }
    return String(streamed.finalOutput);
  };
}

/** The agent on the local upstream, called directly. */
function direct(upstream: string): Ask {
  return agent(new OpenAI({ baseURL: `${upstream}/v1`, apiKey: "local", maxRetries: 0 }));
}

/** Run `fn` inside a new application span named `name`; return its trace id and `fn`'s result. */
function within<T>(name: string, fn: () => Promise<T>): Promise<[string, T]> {
  return trace.getTracer("framework-app").startActiveSpan(name, async (span: Span): Promise<[string, T]> => {
    try {
      return [span.spanContext().traceId, await fn()];
    } finally {
      span.end();
    }
  });
}

/** The application's ambient state: the `observed-service` key from the environment. */
function ambientState(bundle: string, serviceKey: string): WyrdState {
  vi.stubEnv("WYRD_API_KEY", serviceKey);
  return WyrdState.fromPath(bundle);
}

/** Install an application-owned provider exporting to Wyrd with Run correlation. */
function ownedProvider(): BasicTracerProvider {
  const provider = new BasicTracerProvider({
    spanProcessors: [runCorrelationProcessor(), new BatchSpanProcessor(spanExporter(WyrdClient.connect()))],
  });
  trace.setGlobalTracerProvider(provider);
  context.setGlobalContextManager(new AsyncLocalStorageContextManager().enable());
  return provider;
}

/** Publish captured rows, then read trace `traceId` as `credential` (the administrator). */
async function timeline(server: NativeWyrdTestServer, traceId: string, credential = server.apiKey) {
  server.flushBifrost();
  const reader = await Bifrost.connect({ client: WyrdClient.connect({ credential }) });
  try {
    return await reader.traceTimeline(traceId);
  } finally {
    await reader.shutdown();
  }
}

/** The spans whose `gen_ai.operation.name` is `name`. */
function operation(spans: readonly SpanRecord[], name: string): SpanRecord[] {
  return spans.filter((span) => span.attributes["gen_ai.operation.name"] === name);
}

/** The `wyrd.run_id` each span carries. */
function runIds(spans: readonly SpanRecord[]): unknown[] {
  return spans.map((span) => span.attributes["wyrd.run_id"]);
}

/**
 * One tool-calling turn: two model calls and one tool call, every span under
 * the application's `request` root and, below it, in Run `runId`; each call
 * recorded by exactly one span. The Chat Completions model of
 * `@openai/agents-openai` 0.20 supplies no usage to its generation spans, so
 * none is expected.
 */
function expectTurn(spans: readonly SpanRecord[], runId: string, cardUid: string, model: string = MODEL): void {
  const roots = spans.filter((span) => span.parent_span_id === undefined);
  expect(roots.map((span) => span.name)).toEqual(["request"]);
  for (const span of spans.filter((each) => each !== roots[0])) {
    expect(ancestry(spans, span).at(-1)).toBe(roots[0]);
    expect(span.attributes).toMatchObject({ "wyrd.run_id": runId, "wyrd.card_uid": cardUid });
  }
  const models = operation(spans, "chat");
  expect(models).toHaveLength(2);
  for (const span of models) {
    expect(span.attributes["gen_ai.request.model"]).toBe(model);
  }
  expect(operation(spans, "execute_tool").map((span) => span.attributes["gen_ai.tool.name"])).toEqual(["lookup"]);
  expect(new Set(spans.map((span) => span.span_id)).size).toBe(spans.length);
}

/** One turn on `ask` in a Run of `state`, inside the application's `request` span. */
async function turn(state: WyrdState, ask: Ask, prompt = PROMPT) {
  const agentRun = state.run("agent");
  const [traceId, output] = await within("request", () => agentRun.scope(() => ask(prompt)));
  return { traceId, output, runId: agentRun.runId, cardUid: state.cardRef("agent").uid ?? "" };
}

afterEach(() => {
  trace.disable();
  context.disable();
  propagation.disable();
});

test("managed framework telemetry", async ({ server, provider, bundle, serviceKey }) => {
  const state = ambientState(bundle, serviceKey);
  await state.startTelemetry();

  const result = await turn(state, direct(provider.url));
  await state.shutdown();

  expect(result.output).toBe("done");
  expectTurn(await timeline(server, result.traceId), result.runId, result.cardUid);
});

test("owned provider framework telemetry", async ({ server, provider, bundle, serviceKey }) => {
  const state = ambientState(bundle, serviceKey);
  const owned = ownedProvider();

  const result = await turn(state, direct(provider.url));
  await owned.shutdown();

  expectTurn(await timeline(server, result.traceId), result.runId, result.cardUid);
});

test("framework execution edges", async ({ server, provider, bundle, serviceKey }) => {
  const state = ambientState(bundle, serviceKey);
  await state.startTelemetry();
  const ask = direct(provider.url);

  const failedModel = state.run("agent");
  const [modelTrace, modelError] = await within("model_failure", () =>
    failedModel.scope(() => ask("FAIL-MODEL").catch((error: unknown) => error)),
  );
  const failedTool = state.run("agent");
  const [toolTrace] = await within("tool_failure", () => failedTool.scope(() => ask("FAIL-TOOL").catch(() => "")));
  // Started inside its Run, consumed after the scope returned: still attributed.
  const streamed = state.run("agent");
  const [streamTrace, streamOutput] = await within("stream", async () => {
    const pending = streamed.scope(() => ask("hello", true));
    return await pending;
  });
  const outer = state.run("agent");
  const inner = state.run("agent");
  const [nestedTrace] = await within("nested", () =>
    outer.scope(async () => {
      await inner.scope(() => ask("inner"));
      return ask("outer");
    }),
  );
  const concurrent = [state.run("agent"), state.run("agent")];
  const [concurrentTrace] = await within("concurrent", () =>
    Promise.all(concurrent.map((each) => each.scope(() => ask("hello")))),
  );
  const [uncorrelatedTrace] = await within("uncorrelated", () => ask("hello"));
  await state.shutdown();

  expect(modelError).toBeInstanceOf(Error);
  const models = operation(await timeline(server, modelTrace), "chat");
  expect(runIds(models)).toEqual([failedModel.runId]);
  expect(models[0]?.status.code).toBe("error");

  const tools = operation(await timeline(server, toolTrace), "execute_tool");
  expect(runIds(tools)).toEqual([failedTool.runId]);
  expect(tools[0]?.status.code).toBe("error");

  expect(streamOutput).toBe("done");
  const streamedModels = operation(await timeline(server, streamTrace), "chat");
  expect(runIds(streamedModels)).toEqual([streamed.runId, streamed.runId]);

  expect(runIds(operation(await timeline(server, nestedTrace), "chat"))).toEqual([
    inner.runId,
    inner.runId,
    outer.runId,
    outer.runId,
  ]);
  expect(runIds(operation(await timeline(server, concurrentTrace), "chat")).sort()).toEqual(
    concurrent.flatMap((each) => [each.runId, each.runId]).sort(),
  );

  const uncorrelated = await timeline(server, uncorrelatedTrace);
  expect(operation(uncorrelated, "chat")).toHaveLength(2);
  expect(runIds(uncorrelated).every((runId) => runId === undefined)).toBe(true);
});

test("framework sampling privacy and authorization", async ({ server, provider, bundle, serviceKey }) => {
  /** One managed turn exporting as `key`, with `sampler` stubbed; returns the turn and any shutdown error. */
  async function exported(key: string, sampler?: string) {
    vi.stubEnv("OTEL_TRACES_SAMPLER", sampler);
    const state = ambientState(bundle, key);
    await state.startTelemetry();
    const result = await turn(state, direct(provider.url));
    const refusal = await state.shutdown().catch((error: unknown) => error);
    trace.disable();
    return { ...result, refusal };
  }

  const unsampled = await exported(serviceKey, "always_off");
  expect(unsampled.refusal).toBeUndefined();
  expect(await timeline(server, unsampled.traceId)).toEqual([]);

  // The refused export surfaces at shutdown; the run itself completed.
  const denied = await exported(server.bootstrapService(["viewer"], "agents-viewer"));
  expect(denied.output).toBe("done");
  expect(denied.refusal).toMatchObject({ code: "WYRD_SDK_503_TELEMETRY_FLUSH_FAILED" });
  expect(await timeline(server, denied.traceId)).toEqual([]);

  // Another tenant's credential cannot attribute spans to this tenant's
  // Card: the server rejects each one, so only that tenant's uncorrelated
  // `request` span lands, and only there.
  const elsewhere = server.bootstrapServiceInTenant(server.seedTenant("agents-elsewhere"), ["admin"], "agents-app");
  const rejected = await exported(elsewhere);
  expect(rejected.refusal).toBeUndefined();
  expect(await timeline(server, rejected.traceId)).toEqual([]);
  const landed = await timeline(server, rejected.traceId, elsewhere);
  expect(landed.map((span) => [span.name, span.attributes["wyrd.card_uid"]])).toEqual([["request", undefined]]);

  const allowed = await exported(serviceKey);
  expect(allowed.refusal).toBeUndefined();
  const stored = await timeline(server, allowed.traceId);
  expect(operation(stored, "chat")).toHaveLength(2);
  const rendered = JSON.stringify(stored);
  expect(rendered).not.toContain(PROMPT);
  expect(rendered).not.toContain(TOOL_RESULT);
});

test("ambient auth gateway telemetry", async ({ server, provider, bundle, serviceKey }) => {
  await configureGateway(WyrdClient.connect());
  const state = ambientState(bundle, serviceKey);
  await state.startTelemetry();
  const wyrd = WyrdClient.connect();
  const gateway = new OpenAI({ baseURL: `${wyrd.serverUrl}/v1`, apiKey: "wyrd", fetch: wyrd.httpAuth, maxRetries: 0 });
  const before = provider.received.length;

  const result = await turn(state, agent(gateway, "openai/gpt-4o"));
  await state.shutdown();

  expect(result.output).toBe("done");
  const upstream = provider.received.slice(before);
  expect(upstream).toHaveLength(2);
  for (const request of upstream) {
    expect(request.headers.authorization).toBe("Bearer sk-local-upstream");
    expect(Object.keys(request.headers).filter((name) => name.startsWith("x-wyrd"))).toEqual([]);
  }
  expectTurn(await timeline(server, result.traceId), result.runId, result.cardUid, "openai/gpt-4o");

  vi.stubEnv("WYRD_API_KEY", undefined);
  const refusal = await Promise.resolve()
    .then(() => WyrdClient.connect().accessToken())
    .catch((error: unknown) => error);
  expect(refusal).toBeInstanceOf(WyrdError);
  expect((refusal as WyrdError).code).toBe("WYRD_CLIENT_401_NO_CREDENTIALS");
});
