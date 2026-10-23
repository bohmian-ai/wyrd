import { type ProxyTracerProvider, context, propagation, trace } from "@opentelemetry/api";
import OpenAI from "openai";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Gateway, type SpanRecord, TableConfig, WyrdClient, type WyrdState } from "wyrd-ai";

import { assistantTest } from "../support/assistant.js";
import { ancestry, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** The trace identity of a caller's W3C `traceparent`. */
const CALLER_TRACE = "5bf92f3577b34da6a3ce929d0e0e4736";
/** The caller's span identity within {@link CALLER_TRACE}. */
const CALLER_SPAN = "10f067aa0ba902b7";

/** One assistant deployment per file, since the tracer provider is process-global. */
const test = assistantTest();

/** The process-global tracer provider, installed once by the first traced test. */
let telemetry: Promise<unknown> | undefined;

/** Install `state`'s telemetry unless an earlier test in this file already did. */
function startTelemetry(state: WyrdState): Promise<unknown> {
  telemetry ??= state.startTelemetry();
  return telemetry;
}

/** Export every ended span the process-global tracer provider still holds. */
async function flushTelemetry(): Promise<void> {
  const provider = (trace.getTracerProvider() as ProxyTracerProvider).getDelegate();
  await (provider as unknown as { forceFlush(): Promise<void> }).forceFlush();
}

/** One `scope kind name` line per span, for a readable assertion message. */
function outline(timeline: readonly SpanRecord[]): string {
  return timeline.map((span) => `${span.scope.name} ${span.kind} ${span.name}`).join("\n");
}

/** Whether `names` holds every one of `steps` in order, gaps allowed. */
function holdsInOrder(names: readonly string[], steps: readonly ((name: string) => boolean)[]): boolean {
  let at = 0;
  return steps.every((step) => {
    while (at < names.length && !step(names[at])) {
      at += 1;
    }
    at += 1;
    return at <= names.length;
  });
}

test("sdk requests carry the active trace", async ({ server, state }) => {
  await startTelemetry(state);
  const gateway = await Gateway.connect();
  const caller = await trace.getTracer("journey").startActiveSpan("caller", async (span) => {
    await gateway.putCapturePolicy({ mode: "metadata", payload_fields: [] });
    span.end();
    return span.spanContext();
  });

  const spans = await timeline(server, caller.traceId);
  expect(spans.filter((span) => span.parent_span_id === caller.spanId).map((span) => span.name)).toEqual(["request"]);
});

test("service action timeline is one connected tree", async ({ server, assistant, state }) => {
  await startTelemetry(state);
  await state.startBifrost();
  const actions = "vala.datasets.actions";
  const admin = await Bifrost.connect({ table: TableConfig.fromJsonSchema(actions, z.object({ action: z.string() })) });
  await admin.register();
  await admin.shutdown();
  const service = WyrdClient.connect({ credential: assistant.key }).forWorkload(assistant.service.uid);
  const writer = await Bifrost.connect({ client: service });
  await writer.useTableByName(actions);
  const tracer = trace.getTracer("journey.service");

  const action = await tracer.startActiveSpan("service.action", async (span) => {
    tracer.startSpan("answer").end();
    expect(await state.run("model").observe.submit("latency-drift", { latency: 12.5 })).toEqual({ kind: "recorded" });
    writer.insert({ action: "answered" });
    await tracer.startActiveSpan("model.ask", async (ask) => {
      const headers: Record<string, string> = {};
      propagation.inject(context.active(), headers);
      await new OpenAI({
        baseURL: `${service.serverUrl}/v1`,
        apiKey: await service.accessToken(),
        maxRetries: 0,
      }).chat.completions.create({ model: "openai/gpt-4o", messages: [{ role: "user", content: "hi" }] }, { headers });
      ask.end();
    });
    const result = await state.run("agent").observe.verify("answer-judged", { answer: "yes" });
    expect(result.outcome.verdict).toBe("passed");
    span.end();
    return span.spanContext();
  });
  await writer.shutdown();
  await state.shutdown();
  await flushTelemetry();

  const read = await timeline(server, action.traceId);
  const starts = read.map((span) => span.start_time);
  expect(starts).toEqual([...starts].sort());
  const spans = read.filter((span) => span.trace_id === action.traceId);
  const linked = read.filter((span) => span.trace_id !== action.traceId);
  const roots = spans.filter((span) => span.parent_span_id == null);
  expect(
    roots.map((root) => [root.name, root.scope.name]),
    outline(spans),
  ).toEqual([["service.action", "journey.service"]]);
  for (const span of spans) {
    expect(ancestry(spans, span).at(-1), span.name).toBe(roots[0]);
  }
  expect(spans.some((span) => span.name === "answer")).toBe(true);
  const producers = spans.filter((span) => span.kind === "producer" && span.name === "bifrost.enqueue");
  expect(producers.length, outline(spans)).toBeGreaterThanOrEqual(2);
  for (const producer of producers) {
    expect(
      linked.some(
        (consumer) =>
          consumer.name === "bifrost.gate.write" &&
          consumer.kind === "consumer" &&
          (consumer.links ?? []).some(
            (link) => link.trace_id === producer.trace_id && link.span_id === producer.span_id,
          ),
      ),
      outline(linked),
    ).toBe(true);
  }
  const attempts = spans
    .filter((span) => span.scope.name === "wyrd.gateway")
    .map((span) => ancestry(spans, span).map((each) => each.name));
  expect(attempts).toHaveLength(2);
  expect(
    attempts.some((names) =>
      holdsInOrder(names, [
        (name) => name === "gateway.call",
        (name) => name === "request",
        (name) => name === "model.ask",
      ]),
    ),
    JSON.stringify(attempts),
  ).toBe(true);
  expect(
    attempts.some((names) =>
      holdsInOrder(names, [
        (name) => name === "gateway.call",
        (name) => name.startsWith("verification."),
        (name) => name === "request",
        (name) => name === "service.action",
      ]),
    ),
    JSON.stringify(attempts),
  ).toBe(true);
  const stamped = await (await Bifrost.connect()).sql(
    `SELECT encode(span_id, 'hex') AS span_id, sub_principal_id, act_principal_id FROM vala.traces.spans WHERE trace_id = X'${action.traceId}'`,
    [],
    z.object({ span_id: z.string(), sub_principal_id: z.string(), act_principal_id: z.string() }),
  );
  const pairs = (selected: typeof spans) => [
    ...new Set(
      selected.map((span) => {
        const row = stamped.find((each) => each.span_id === span.span_id);
        return `${row?.sub_principal_id}/${row?.act_principal_id}`;
      }),
    ),
  ];
  const requests = pairs(spans.filter((span) => span.name === "request"));
  const [subject] = (requests[0] ?? "").split("/");
  expect(requests).toEqual([`${subject}/${subject}`]);
  const judge = pairs(
    spans.filter(
      (span) =>
        span.scope.name === "wyrd.gateway" &&
        ancestry(spans, span).some((each) => each.name.startsWith("verification.")),
    ),
  );
  expect(judge).toEqual([expect.stringMatching(new RegExp(`^${subject}/(?!${subject}$)[0-9a-f-]{36}$`))]);
});

test("another tenant cannot read the timeline", async ({ server }) => {
  const token = await WyrdClient.connect().accessToken();
  const listed = await fetch(`${server.baseUrl}/v1/bifrost/tables`, {
    headers: { "x-wyrd-access-token": `Bearer ${token}`, traceparent: `00-${CALLER_TRACE}-${CALLER_SPAN}-01` },
  });
  expect(listed.status).toBe(200);
  expect(Array.isArray(await listed.json())).toBe(true);
  expect(await timeline(server, CALLER_TRACE), "the owning tenant reads its request").not.toEqual([]);

  const foreign = WyrdClient.connect({
    credential: server.bootstrapServiceInTenant(server.seedTenant("other-tenant"), ["admin"], "other_tenant_admin"),
  });
  const read = await (await Bifrost.connect({ client: foreign })).traceTimeline(CALLER_TRACE);
  expect(read, outline(read)).toEqual([]);
});
