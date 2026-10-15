import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { fileURLToPath } from "node:url";

import { SpanStatusCode, context, trace } from "@opentelemetry/api";
import { resourceFromAttributes } from "@opentelemetry/resources";
import {
  AlwaysOffSampler,
  BasicTracerProvider,
  InMemorySpanExporter,
  SimpleSpanProcessor,
} from "@opentelemetry/sdk-trace-base";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { WyrdAgentsTraceProcessor, WyrdClient, WyrdState } from "wyrd-ai";
import { runCorrelationProcessor, spanExporter } from "wyrd-ai/otel";

import { StorageContextManager } from "../support/otel-context.js";

// The Python SDK's offline complete Service bundle; loading it reads no server.
const BUNDLE = fileURLToPath(new URL("../../../../wyrd-sdk-python/tests/fixtures/bundles/complete", import.meta.url));
const UNREACHABLE = "http://127.0.0.1:9";

/** A state over the offline bundle whose explicit client is never dialled. */
function offlineState(credential = "wyrd_unit_unused"): WyrdState {
  return WyrdState.fromPath(BUNDLE, { client: WyrdClient.connect({ serverUrl: UNREACHABLE, credential }) });
}

/** The provider registered behind the global proxy. */
function globalProvider(): BasicTracerProvider {
  return (trace.getTracerProvider() as unknown as { getDelegate(): BasicTracerProvider }).getDelegate();
}

beforeEach(() => {
  trace.disable();
  context.disable();
  context.setGlobalContextManager(new StorageContextManager());
});

afterEach(async () => {
  const installed = globalProvider();
  if (installed instanceof BasicTracerProvider) {
    await installed.shutdown();
  }
  vi.restoreAllMocks();
  trace.disable();
  context.disable();
});

test("telemetry setup", async () => {
  const state = offlineState();
  await state.startTelemetry();
  const installed = globalProvider();
  expect(installed).toBeInstanceOf(BasicTracerProvider);
  await state.startTelemetry();
  expect(globalProvider()).toBe(installed);

  // Another state's provider is preserved and refused; the refused attempt releases its own.
  const release = vi.spyOn(BasicTracerProvider.prototype, "shutdown");
  await expect(offlineState("wyrd_unit_other").startTelemetry()).rejects.toMatchObject({
    code: "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS",
  });
  expect(globalProvider()).toBe(installed);
  expect(release).toHaveBeenCalledTimes(1);
  expect(release.mock.contexts[0]).not.toBe(installed);

  // An application's own provider is refused the same way.
  await installed.shutdown();
  trace.disable();
  const application = new BasicTracerProvider();
  trace.setGlobalTracerProvider(application);
  await expect(offlineState().startTelemetry()).rejects.toMatchObject({
    code: "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS",
  });
  expect(globalProvider()).toBe(application);
});

test("owned provider correlation", async () => {
  const state = offlineState();
  const exporter = new InMemorySpanExporter();
  const owned = new BasicTracerProvider({
    resource: resourceFromAttributes({ "service.name": "owned" }),
    spanProcessors: [runCorrelationProcessor(), new SimpleSpanProcessor(exporter)],
  });
  const tracer = owned.getTracer("framework");
  const run = state.run();
  const model = run.forCard("model");

  await Promise.all([
    run.scope(async () => {
      await Promise.resolve();
      tracer.startSpan("a").end();
    }),
    model.scope(async () => {
      await Promise.resolve();
      tracer.startSpan("b").end();
    }),
  ]);
  run.scope(() => {
    model.scope(() => tracer.startSpan("nested").end());
    tracer.startSpan("restored").end();
  });
  tracer.startSpan("unscoped").end();

  const correlation = Object.fromEntries(
    exporter.getFinishedSpans().map((span) => [span.name, span.attributes["wyrd.run_id"] ?? null]),
  );
  const cards = Object.fromEntries(
    exporter.getFinishedSpans().map((span) => [span.name, span.attributes["wyrd.card_uid"] ?? null]),
  );
  expect(correlation).toEqual({ a: run.runId, b: run.runId, nested: run.runId, restored: run.runId, unscoped: null });
  expect(cards.a).not.toBe(cards.b);
  expect(cards.nested).toBe(cards.b);
  expect(cards.restored).toBe(cards.a);
  expect(exporter.getFinishedSpans().every((span) => span.resource.attributes["service.name"] === "owned")).toBe(true);

  const dropped = new InMemorySpanExporter();
  const unsampled = new BasicTracerProvider({
    sampler: new AlwaysOffSampler(),
    spanProcessors: [runCorrelationProcessor(), new SimpleSpanProcessor(dropped)],
  });
  run.scope(() => unsampled.getTracer("framework").startSpan("dropped").end());
  expect(dropped.getFinishedSpans()).toEqual([]);
});

test("managed telemetry lifecycle", async () => {
  // Bifrost is never started, so a flush's drain reports BIFROST_NOT_STARTED and a
  // shutdown's drain succeeds: either outcome in the error proves the drain ran.
  const timeout = [new Error("Span processor did not completed within timeout period of 1 ms")];
  const failure = [new TypeError("SECRET payload in an exporter error")];
  const cases = [
    {
      method: "flush",
      outcome: timeout,
      code: "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT",
      native: "WYRD_SDK_400_BIFROST_NOT_STARTED",
    },
    {
      method: "flush",
      outcome: failure,
      code: "WYRD_SDK_503_TELEMETRY_FLUSH_FAILED",
      native: "WYRD_SDK_400_BIFROST_NOT_STARTED",
    },
    { method: "flush", outcome: undefined, code: "WYRD_SDK_400_BIFROST_NOT_STARTED", native: undefined },
    { method: "shutdown", outcome: timeout, code: "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT", native: null },
    { method: "shutdown", outcome: failure, code: "WYRD_SDK_503_TELEMETRY_FLUSH_FAILED", native: null },
    { method: "shutdown", outcome: undefined, code: undefined, native: undefined },
  ] as const;
  for (const { method, outcome, code, native } of cases) {
    trace.disable();
    const state = offlineState();
    await state.startTelemetry();
    const provider = globalProvider();
    const flush = vi.spyOn(provider, "forceFlush");
    if (outcome === undefined) {
      flush.mockResolvedValue();
    } else {
      flush.mockRejectedValue(outcome);
    }
    const active = trace.getTracer("framework").startSpan("active");
    const settled = state[method]();
    if (code === undefined) {
      await expect(settled).resolves.toBeUndefined();
    } else {
      const error = await settled.then(
        () => undefined,
        (rejected: unknown) => rejected as { code: string; details?: Record<string, unknown>; message: string },
      );
      expect(error?.code, `${method} ${code}`).toBe(code);
      expect(error?.message).not.toContain("SECRET");
      expect(JSON.stringify(error?.details ?? {})).not.toContain("SECRET");
      if (native !== undefined) {
        expect(error?.details).toMatchObject({ stage: "opentelemetry", native_code: native });
      }
    }
    expect(flush).toHaveBeenCalledTimes(1);
    // Left unended: ending it would export to the unreachable server.
    expect(active.isRecording()).toBe(true);
    expect(globalProvider()).toBe(provider);
    await provider.shutdown();
  }
});

test("exporter token freshness", async () => {
  const tokens: (string | undefined)[] = [];
  const server = createServer((request, response) => {
    tokens.push(request.headers["x-wyrd-access-token"] as string | undefined);
    request.resume();
    request.on("end", () => response.writeHead(200, { "content-type": "application/x-protobuf" }).end());
  });
  await new Promise<void>((listening) => server.listen(0, "127.0.0.1", listening));
  try {
    let issued = 0;
    const client = {
      serverUrl: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
      accessToken: async () => `token-${++issued}`,
    } as unknown as WyrdClient;
    const exporter = spanExporter(client);
    const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(exporter)] });
    expect(issued).toBe(0);
    provider.getTracer("framework").startSpan("first").end();
    await provider.forceFlush();
    provider.getTracer("framework").startSpan("second").end();
    await provider.forceFlush();
    await provider.shutdown();
  } finally {
    server.close();
  }
  expect(tokens).toEqual(["Bearer token-1", "Bearer token-2"]);
});

/** A stand-in Agents SDK span carrying only what the processor reads. */
function agentsSpan(spanId: string, spanData: Record<string, unknown>, extra: Record<string, unknown> = {}) {
  return {
    spanId,
    traceId: "trace_1",
    parentId: null,
    spanData,
    startedAt: null,
    endedAt: null,
    error: null,
    ...extra,
  };
}

test("agents callback lifecycle", async () => {
  const state = offlineState();
  const exporter = new InMemorySpanExporter();
  const provider = new BasicTracerProvider({
    spanProcessors: [runCorrelationProcessor(), new SimpleSpanProcessor(exporter)],
  });
  trace.setGlobalTracerProvider(provider);
  const processor = new WyrdAgentsTraceProcessor();
  type Callback = Parameters<WyrdAgentsTraceProcessor["onSpanStart"]>[0];
  const call = (span: ReturnType<typeof agentsSpan>) => span as unknown as Callback;
  const traceHandle = { traceId: "trace_1", name: "workflow" } as unknown as Parameters<
    WyrdAgentsTraceProcessor["onTraceStart"]
  >[0];
  const runA = state.run();
  const runB = state.run();

  const agent = agentsSpan("agent", { type: "agent", name: "assistant" }, { startedAt: "2026-01-01T00:00:00.000Z" });
  const tool = agentsSpan(
    "tool",
    { type: "function", name: "lookup", input: "SECRET-ARGS", output: "SECRET-RESULT" },
    { parentId: "agent", startedAt: "not a timestamp" },
  );
  const generation = agentsSpan(
    "generation",
    { type: "generation", input: [{ content: "SECRET-PROMPT" }], output: [{ content: "SECRET-REPLY" }] },
    { parentId: "missing-parent" },
  );
  const enclosing = trace.getTracer("app").startSpan("enclosing");
  await context.with(trace.setSpan(context.active(), enclosing), () =>
    runA.scope(async () => {
      await processor.onTraceStart(traceHandle);
      await processor.onSpanStart(call(agent));
      await processor.onSpanStart(call(agent));
      await processor.onSpanStart(call(tool));
      await processor.onSpanStart(call(generation));
    }),
  );
  Object.assign(generation.spanData, { model: "local-model", usage: { input_tokens: 3, output_tokens: 1 } });
  await runB.scope(async () => {
    await processor.onSpanEnd(call(generation));
    tool.error = { message: "SECRET-ERROR", data: { secret: "SECRET-DATA" } } as never;
    await processor.onSpanEnd(call(tool));
    await processor.onSpanEnd(call(tool));
    await processor.onSpanEnd(call(agentsSpan("never-started", { type: "custom", name: "x", data: {} })));
  });
  const response = agentsSpan(
    "response",
    { type: "response", _response: { model: "resp-model", usage: { input_tokens: 5, output_tokens: 2 } } },
    { parentId: "agent" },
  );
  await processor.onSpanStart(call(response));
  await processor.onSpanEnd(call(response));
  const custom = agentsSpan(
    "custom",
    { type: "custom", name: "checkpoint", data: { secret: "SECRET" } },
    { parentId: "agent" },
  );
  await processor.onSpanStart(call(custom));
  await processor.onSpanEnd(call(custom));
  agent.endedAt = "2026-01-01T00:00:02.000Z" as never;
  await processor.onSpanEnd(call(agent));
  await processor.onTraceEnd(traceHandle);
  enclosing.end();

  const spans = exporter.getFinishedSpans();
  const names = spans.map((span) => span.name);
  expect(new Set(names).size).toBe(names.length);
  const byName = (name: string) => {
    const span = spans.find((candidate) => candidate.name === name);
    if (span === undefined) {
      throw new Error(`no finished span named ${name}`);
    }
    return span;
  };
  const root = byName("workflow");
  expect(root.parentSpanContext?.spanId).toBe(enclosing.spanContext().spanId);
  expect(new Set(spans.map((span) => span.spanContext().traceId)).size).toBe(1);
  const agentSpan = byName("invoke_agent assistant");
  expect(agentSpan.parentSpanContext?.spanId).toBe(root.spanContext().spanId);
  expect(agentSpan.startTime).toEqual([1_767_225_600, 0]);
  expect(agentSpan.endTime).toEqual([1_767_225_602, 0]);
  expect(agentSpan.attributes["gen_ai.agent.name"]).toBe("assistant");
  const toolSpan = byName("execute_tool lookup");
  expect(toolSpan.parentSpanContext?.spanId).toBe(agentSpan.spanContext().spanId);
  expect(toolSpan.status.code).toBe(SpanStatusCode.ERROR);
  expect(toolSpan.attributes["error.type"]).toBe("_OTHER");
  const chat = byName("chat local-model");
  expect(chat.parentSpanContext?.spanId).toBe(root.spanContext().spanId);
  expect(chat.attributes).toEqual({
    "gen_ai.operation.name": "chat",
    "gen_ai.request.model": "local-model",
    "gen_ai.usage.input_tokens": 3,
    "gen_ai.usage.output_tokens": 1,
    "wyrd.card_uid": expect.any(String),
    "wyrd.run_id": runA.runId,
  });
  expect(byName("chat resp-model").attributes["gen_ai.usage.output_tokens"]).toBe(2);
  expect(Object.keys(byName("checkpoint").attributes).some((key) => key.startsWith("gen_ai"))).toBe(false);
  for (const span of spans) {
    expect(JSON.stringify([span.attributes, span.status, span.events]), span.name).not.toContain("SECRET");
  }
  expect(Object.fromEntries(spans.map((span) => [span.name, span.attributes["wyrd.run_id"] ?? null]))).toEqual({
    enclosing: null,
    workflow: runA.runId,
    "invoke_agent assistant": runA.runId,
    "execute_tool lookup": runA.runId,
    "chat local-model": runA.runId,
    "chat resp-model": null,
    checkpoint: null,
  });
  expect(runA.runId).not.toBe(runB.runId);

  // Shutdown ends unfinished spans with ERROR, is idempotent, and ignores late callbacks.
  exporter.reset();
  const interrupted = { traceId: "trace_2", name: "interrupted" } as unknown as typeof traceHandle;
  await processor.onTraceStart(interrupted);
  await processor.shutdown();
  await processor.shutdown();
  await processor.onSpanStart(call(agentsSpan("late", { type: "custom", name: "late", data: {} })));
  await processor.onTraceEnd(interrupted);
  await processor.forceFlush();
  const [ended, ...rest] = exporter.getFinishedSpans();
  expect(rest).toEqual([]);
  expect(ended?.name).toBe("interrupted");
  expect(ended?.status).toEqual({
    code: SpanStatusCode.ERROR,
    message: "span did not finish before the trace processor shut down",
  });
  await provider.shutdown();
});
