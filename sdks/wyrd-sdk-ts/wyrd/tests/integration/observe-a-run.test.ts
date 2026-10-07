import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { context, trace } from "@opentelemetry/api";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Cards, TableConfig, WyrdError, WyrdState } from "@wyrd/sdk";

import { StorageContextManager } from "../support/otel-context.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

const SESSION = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f";
const SCREENSHOT = { id: "screenshot", kind: "image", uri: "s3://bucket/shot.png", mediaType: "image/png" } as const;
const ANSWERS = "vala.datasets.observed_answers";
const SCORES = "vala.datasets.observed_scores";
const VALUE_ROW = { type: "object", properties: { value: { type: "integer" } }, required: ["value"] } as const;

/**
 * The observed Service, hydrated once, plus a fresh state per test.
 *
 * `serviceKey` is the key registration projected for the Service: only its
 * Card scope covers the Agent and Model a run observes.
 */
const test = serverTest().extend<{
  bundle: string;
  serviceKey: string;
  reader: Bifrost;
  state: WyrdState;
}>({
  bundle: [
    async ({ server: _ }, use) => {
      const cards = Cards.connect();
      await cards.registerFromPath(fixture("cards/observe_a_run/observed-model.yaml"));
      const { root } = await cards.registerFromPath(fixture("cards/observe_a_run/observed-service.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-observed-"));
      await cards.hydrate(root, bundle);
      await use(bundle);
    },
    { scope: "file" },
  ],
  serviceKey: [
    async ({ server, bundle: _ }, use) =>
      use(server.credentialRegisteredService("default/Service/observed-service@1.0.0", [])),
    { scope: "file" },
  ],
  reader: [
    async ({ server: _ }, use) => {
      const reader = await Bifrost.connect();
      await use(reader);
      await reader.shutdown();
    },
    { scope: "file" },
  ],
  state: async ({ bundle }, use) => use(WyrdState.fromPath(bundle)),
});

/** A stored fixed-width trace or span identity, read as lower-case hex. */
const HexId = z.instanceof(Uint8Array).transform((bytes) => Buffer.from(bytes).toString("hex"));

const DriftRow = z.object({
  series: z.string(),
  num_value: z.number().nullable(),
  str_value: z.string().nullable(),
  session_id: z.string().nullable(),
  card_uid: z.string(),
  run_id: z.string(),
});

const SpannedEvalRow = z.object({
  context: z.string(),
  session_id: z.string().nullable(),
  media: z.string().nullable(),
  trace_id: HexId,
  span_id: HexId,
  card_uid: z.string(),
  span_name: z.string(),
});

const RecordRow = z.object({ value: z.bigint(), card_uid: z.string() });

test("run observations read back by run id", async ({ server, serviceKey, reader, state }) => {
  await state.startBifrost({ credential: serviceKey });
  const run = state.run();

  run.forCard("model").observe.drift({ latency: 12.5, tier: "gold" }, { sessionId: SESSION });
  await state.shutdown();
  server.flushBifrost();

  const rows = await reader.sql(
    `SELECT series, num_value, str_value, session_id, card_uid, run_id
       FROM vala.drift.observations WHERE run_id = $1 ORDER BY series`,
    [run.runId],
    DriftRow,
  );
  const model = state.cardRef("model").uid;
  expect(rows).toEqual([
    { series: "latency", num_value: 12.5, str_value: "12.5", session_id: SESSION, card_uid: model, run_id: run.runId },
    { series: "tier", num_value: null, str_value: "gold", session_id: SESSION, card_uid: model, run_id: run.runId },
  ]);
});

test("run view exposes its alias", ({ state }) => {
  const run = state.run();
  const model = run.forCard("model");
  const agentRun = state.run("agent");

  expect([run.alias, model.alias, agentRun.alias]).toEqual(["root", "model", "agent"]);
  expect(model.runId).toBe(run.runId);
  expect(agentRun.runId).not.toBe(run.runId);
});

test("card scoped key cannot write another cards observations", async ({ state }) => {
  await state.startBifrost();

  state.run("model").observe.drift({ latency: 12.5 });

  await expect(state.shutdown()).rejects.toMatchObject({ code: "WYRD_VALA_403_BIFROST_CARD_SCOPE" });
});

test("eval observation carries its session, media, and active span", async ({ server, serviceKey, reader, state }) => {
  vi.stubEnv("OTEL_EXPORTER_OTLP_ENDPOINT", server.grpcUrl);
  vi.stubEnv("OTEL_EXPORTER_OTLP_HEADERS", `x-wyrd-api-key=${server.apiKey}`);
  const tracing = new BasicTracerProvider({ spanProcessors: [new BatchSpanProcessor(new OTLPTraceExporter())] });
  context.setGlobalContextManager(new StorageContextManager());
  await state.startBifrost({ credential: serviceKey });
  const agent = state.run("agent");

  const span = tracing.getTracer("wyrd.tests.observe").startSpan("answer");
  context.with(trace.setSpan(context.active(), span), () =>
    agent.observe.eval({ answer: "yes" }, { sessionId: SESSION, media: [SCREENSHOT] }),
  );
  span.end();
  await tracing.shutdown();
  context.disable();
  await state.shutdown();
  server.flushBifrost();

  const rows = await reader.sql(
    `SELECT e.context, e.session_id, e.media, e.trace_id, e.span_id, e.card_uid, s.name AS span_name
         FROM vala.eval.observations e
         JOIN vala.traces.spans s ON e.trace_id = s.trace_id AND e.span_id = s.span_id
         WHERE e.run_id = $1`,
    [agent.runId],
    SpannedEvalRow,
  );
  expect(rows).toEqual([
    {
      context: '{"answer":"yes"}',
      session_id: SESSION,
      media: '[{"id":"screenshot","kind":"image","uri":"s3://bucket/shot.png","media_type":"image/png"}]',
      trace_id: span.spanContext().traceId,
      span_id: span.spanContext().spanId,
      card_uid: state.cardRef("agent").uid,
      span_name: "answer",
    },
  ]);
});

test("records land in their tables under the view that wrote them", async ({ server, serviceKey, reader, state }) => {
  for (const table of [ANSWERS, SCORES]) {
    const registrar = await Bifrost.connect({ table: TableConfig.fromJsonSchema(table, VALUE_ROW) });
    await registrar.register();
    await registrar.shutdown();
  }
  await state.startBifrost({ credential: serviceKey });
  const run = state.run();

  await run.forCard("agent").observe.record(ANSWERS, { value: 41 });
  await run.forCard("model").observe.record(SCORES, { value: 42 });
  await state.shutdown();
  server.flushBifrost();

  const read = (table: string) =>
    reader.sql(`SELECT value, card_uid FROM ${table} WHERE run_id = $1`, [run.runId], RecordRow);
  expect(await read(ANSWERS)).toEqual([{ value: 41n, card_uid: state.cardRef("agent").uid }]);
  expect(await read(SCORES)).toEqual([{ value: 42n, card_uid: state.cardRef("model").uid }]);
});

/** One emit a run refuses, and the catalog code it raises. */
type Refusal = readonly [string, (state: WyrdState, bundle: string) => unknown, string];

test.for<Refusal>([
  [
    "emit before Bifrost starts",
    (_, bundle) => WyrdState.fromPath(bundle).run().observe.drift({ latency: 1 }),
    "WYRD_SDK_400_BIFROST_NOT_STARTED",
  ],
  [
    "span id without its trace id",
    (state) => state.run("agent").observe.eval({ answer: "yes" }, { spanId: "00f067aa0ba902b7" }),
    "WYRD_SPEC_400_VALIDATION",
  ],
  [
    "malformed trace id",
    (state) =>
      state
        .run("agent")
        .observe.eval({ answer: "yes" }, { traceId: "zzf92f3577b34da6a3ce929d0e0e4736", spanId: "00f067aa0ba902b7" }),
    "WYRD_SPEC_400_VALIDATION",
  ],
  [
    "media kind outside the closed set",
    (state) =>
      state.run("agent").observe.eval(
        { answer: "yes" },
        // @ts-expect-error a kind outside the closed set reaches server-owned validation.
        { media: [{ ...SCREENSHOT, kind: "hologram" }] },
      ),
    "WYRD_SPEC_400_VALIDATION",
  ],
  [
    "record into a built-in table",
    (state) => state.run("agent").observe.record("vala.drift.observations", { latency: 1 }),
    "WYRD_SDK_400_INVALID_OBSERVATION",
  ],
  [
    "record into an unregistered table",
    (state) => state.run("agent").observe.record("vala.datasets.never_registered", { value: 1 }),
    "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
  ],
  ["view of an unknown alias", (state) => state.run().forCard("missing"), "WYRD_SDK_404_UNKNOWN_ALIAS"],
])("refused emit raises its catalog code: %s", async ([, emit, code], { bundle, serviceKey, state }) => {
  await state.startBifrost({ credential: serviceKey });

  await expect(Promise.resolve().then(() => emit(state, bundle))).rejects.toMatchObject({ code });

  await state.shutdown();
});

test("drift burst lands every row exactly once", async ({ server, serviceKey, reader, state }) => {
  await state.startBifrost({ credential: serviceKey, clientByteLimitBytes: 16 * 1024 * 1024 });
  const model = state.run("model");

  for (let observation = 0; observation < 1_000; observation += 1) {
    const features = Object.fromEntries(
      Array.from({ length: 9 }, (_, feature) => [`feature_${feature}`, observation + feature / 10]),
    );
    // Admission is all-or-none per observation: a refused one admitted no
    // row, so resubmitting it after a flush duplicates nothing.
    for (;;) {
      try {
        model.observe.drift(features);
        break;
      } catch (error) {
        if (!(error instanceof WyrdError) || error.code !== "WYRD_CLIENT_429_QUEUE_FULL") {
          throw error;
        }
        await state.flush();
      }
    }
  }
  await state.shutdown();
  server.flushBifrost();

  const perRecord = await reader.sql(
    `SELECT COUNT(*) AS features FROM vala.drift.observations WHERE run_id = $1 GROUP BY record_id`,
    [model.runId],
    z.object({ features: z.bigint() }),
  );
  expect(perRecord).toHaveLength(1_000);
  expect(new Set(perRecord.map((row) => row.features))).toEqual(new Set([9n]));
});

test("unsealable byte budget is refused at connect", async ({ serviceKey, state }) => {
  await expect(state.startBifrost({ credential: serviceKey, clientByteLimitBytes: 1024 })).rejects.toMatchObject({
    code: "WYRD_CLIENT_400_CONFIG_INVALID",
  });
});
