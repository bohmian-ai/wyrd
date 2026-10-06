import { createHash, randomUUID } from "node:crypto";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Metadata } from "@grpc/grpc-js";
import { context, trace } from "@opentelemetry/api";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, Cards, TableConfig, WyrdError, WyrdState } from "@wyrd/sdk";

import { StorageContextManager } from "../support/otel-context.js";

const PROMPT_ARTIFACT = "ts-observe-prompt-artifact";

const SESSION = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f";
const MEDIA = [
  { id: "screenshot", kind: "image", uri: "s3://bucket/shot.png", mediaType: "image/png" },
] as const;
const MEDIA_TEXT =
  '[{"id":"screenshot","kind":"image","uri":"s3://bucket/shot.png","media_type":"image/png"}]';

const DATASET_SCHEMA = {
  type: "object",
  properties: { value: { type: "integer" } },
  required: ["value"],
} as const;

/**
 * Write a Service graph of one Model, one Agent, and its artifact-bearing Prompt.
 *
 * The Model and Agent give the journey two sibling views to switch between; the
 * artifact-bearing Prompt registers alone, so the Service references it by exact
 * identity.
 */
function writeServiceGraph(root: string): string {
  const digest = createHash("sha256").update(PROMPT_ARTIFACT).digest("base64");
  writeFileSync(join(root, "observe-prompt.txt"), PROMPT_ARTIFACT);
  writeFileSync(
    join(root, "observe-prompt.yaml"),
    `apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: ts-observe-prompt
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-4o
  messages: [hello]
artifacts:
  - relative_path: observe-prompt.txt
    sha256: ${digest}
    size_bytes: ${PROMPT_ARTIFACT.length}
    content_type: text/plain
`,
  );
  writeFileSync(
    join(root, "observe-model.yaml"),
    `apiVersion: wyrd/v1
kind: Model
metadata:
  name: ts-observe-model
  version: 1.0.0
  space: default
spec:
  interface:
    kind: Custom
    meta:
      framework_version: 0.1.0
      loader_module: fixture
      loader_class: TinyModel
      extra: {}
  task_type: Other
  signature:
    inputs:
      - name: input
        dtype: float64
    outputs:
      - name: output
        dtype: float64
  card_refs: []
`,
  );
  writeFileSync(
    join(root, "observe-agent.yaml"),
    `apiVersion: wyrd/v1
kind: Agent
metadata:
  name: ts-observe-agent
  version: 1.0.0
  space: default
spec:
  prompt:
    kind: Prompt
    name: ts-observe-prompt
    version: 1.0.0
    space: default
  run_config:
    max_iterations: 2
    timeout_ms: 1000
`,
  );
  const service = join(root, "observe-service.yaml");
  writeFileSync(
    service,
    `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ts-observe-service
  version: 1.0.0
  space: default
spec:
  service_type: agent
  components:
    - alias: model
      ref: ./observe-model.yaml
    - alias: agent
      ref: ./observe-agent.yaml
    - alias: prompt
      ref:
        kind: Prompt
        name: ts-observe-prompt
        version: 1.0.0
        space: default
`,
  );
  return service;
}

/** Await a promise expected to reject with a structured catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/** Lower-case hex of one persisted fixed-width identity, or null. */
function hex(value: unknown): string | null {
  return value == null ? null : Buffer.from(value as Uint8Array).toString("hex");
}

describe("scoped observation journey", () => {
  it("emits Drift, Eval, and generic rows correlated to their subject Cards", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-observe-"));
    const service = writeServiceGraph(root);
    const bundle = join(root, "bundle");
    const server = startTestServer();
    const manager = new StorageContextManager();
    context.setGlobalContextManager(manager);
    try {
      const stamp = Date.now().toString(36);
      const dataset = `vala.datasets.observe_ts_${stamp}_a`;
      const datasetB = `vala.datasets.observe_ts_${stamp}_b`;
      for (const fqn of [dataset, datasetB]) {
        const registrar = await Bifrost.connect({
          table: TableConfig.fromJsonSchema(fqn, DATASET_SCHEMA),
          serverUrl: server.baseUrl,
          credential: server.apiKey,
          grpcUrl: server.grpcUrl,
        });
        expect(await registrar.register()).toBe("created");
        await registrar.shutdown();
      }

      const cards = Cards.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });
      await cards.registerFromPath(join(root, "observe-prompt.yaml"));
      const receipt = await cards.registerFromPath(service);
      await cards.hydrate(receipt.root, bundle);

      // The writer must be the principal registration projected for the
      // Service: only its card-ref scope covers the component Cards below.
      const credential = server.credentialRegisteredService(
        `${receipt.root.space}/Service/${receipt.root.name}@${receipt.root.version}`,
        [],
      );

      const state = WyrdState.fromPath(bundle);
      await state.startBifrost({
        serverUrl: server.baseUrl,
        credential,
        grpcUrl: server.grpcUrl,
      });

      const run = state.run();
      // With no Card argument the run targets the root Service.
      expect(run.alias).toBe("root");
      expect(state.rootRef.uid).toBe(receipt.root.uid);
      const model = run.forCard("model");
      const agent = run.forCard("agent");
      expect(model.runId).toBe(run.runId);
      expect(agent.runId).toBe(run.runId);
      expect([model.alias, agent.alias]).toEqual(["model", "agent"]);
      const modelUid = state.cardRef("model").uid;
      const agentUid = state.cardRef("agent").uid;
      expect(modelUid).toBeTruthy();
      expect(agentUid).not.toBe(modelUid);

      model.observe.drift({ latency_ms: 12.5, tier: "gold" });
      // A real tracer's active span crosses N-API as the Eval row's identity,
      // and its stock OTLP exporter lands the same span in `vala.traces.spans`.
      const metadata = new Metadata();
      metadata.set("x-wyrd-access-token", `Bearer ${server.token}`);
      const tracerProvider = new BasicTracerProvider({
        spanProcessors: [
          new BatchSpanProcessor(new OTLPTraceExporter({ url: server.grpcUrl, metadata })),
        ],
      });
      const span = tracerProvider.getTracer("wyrd.tests.observe").startSpan("observe");
      const active = span.spanContext();
      context.with(trace.setSpan(context.active(), span), () =>
        agent.observe.eval({ answer: "yes" }, { sessionId: SESSION, media: MEDIA }),
      );
      span.end();
      await tracerProvider.forceFlush();
      await tracerProvider.shutdown();
      await agent.observe.record(dataset, { value: 41 });
      await agent.observe.record(dataset, { value: 43 });
      await model.observe.record(datasetB, { value: 42 });

      expect(() =>
        agent.observe.eval({ answer: "orphan" }, { spanId: active.spanId }),
      ).toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }));
      // Malformed trace identity fails visibly rather than dropping correlation.
      for (const identity of [
        { traceId: `zz${active.traceId.slice(2)}`, spanId: active.spanId },
        { traceId: active.traceId.slice(2), spanId: active.spanId },
        { traceId: active.traceId, spanId: `${active.spanId}00` },
      ]) {
        expect(() => agent.observe.eval({ answer: "bad-trace" }, identity), JSON.stringify(identity)).toThrow(
          expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }),
        );
      }
      expect(() =>
        agent.observe.eval(
          { answer: "bad-media" },
          // @ts-expect-error a kind outside the closed set reaches server-owned validation.
          { media: [{ id: "screenshot", kind: "hologram", uri: "s3://bucket/shot.png" }] },
        ),
      ).toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }));

      const reserved = await rejection(
        agent.observe.record("vala.drift.observations", { x: 1 }),
      );
      expect(reserved.code).toBe("WYRD_SDK_400_INVALID_OBSERVATION");
      const absent = await rejection(
        agent.observe.record(`vala.datasets.absent_${randomUUID().replaceAll("-", "")}`, {
          value: 1,
        }),
      );
      expect(absent.code).toBe("WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
      expect(() => run.forCard("missing")).toThrow(
        expect.objectContaining({ code: "WYRD_SDK_404_UNKNOWN_ALIAS" }),
      );
      // The concise single-Card form opens its own invocation on that Card.
      const agentRun = state.run("agent");
      expect(agentRun.alias).toBe("agent");
      expect(agentRun.runId).not.toBe(run.runId);
      expect(agentRun.forCard("model").runId).toBe(agentRun.runId);
      expect(() => state.run("missing")).toThrow(
        expect.objectContaining({ code: "WYRD_SDK_404_UNKNOWN_ALIAS" }),
      );

      await state.shutdown();
      // A closed state stays closed and has nothing left to drain.
      await state.shutdown();
      server.flushBifrost();

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      const drift = (
        await reader.sql(
          `SELECT series, num_value, str_value, card_uid, run_id
             FROM vala.drift.observations WHERE run_id = '${run.runId}' ORDER BY series`,
        )
      )
        .toArrow()
        .toArray()
        .map((row) => row.toJSON());
      expect(drift).toEqual([
        {
          series: "latency_ms",
          num_value: 12.5,
          str_value: "12.5",
          card_uid: modelUid,
          run_id: run.runId,
        },
        {
          series: "tier",
          num_value: null,
          str_value: "gold",
          card_uid: modelUid,
          run_id: run.runId,
        },
      ]);

      const evals = (
        await reader.sql(
          `SELECT context, session_id, trace_id, span_id, media, card_uid, run_id
             FROM vala.eval.observations WHERE run_id = '${run.runId}'`,
        )
      )
        .toArrow()
        .toArray()
        .map((row) => row.toJSON());
      // One row: neither refused Eval was admitted. It carries the authored
      // session/media, the active span's exact identity, and the Agent view's
      // subject rather than the Model's.
      expect(
        evals.map((row) => ({ ...row, trace_id: hex(row.trace_id), span_id: hex(row.span_id) })),
      ).toEqual([
        {
          context: JSON.stringify({ answer: "yes" }),
          session_id: SESSION,
          trace_id: active.traceId,
          span_id: active.spanId,
          media: MEDIA_TEXT,
          card_uid: agentUid,
          run_id: run.runId,
        },
      ]);

      // The typed trace/span identities join the exported span by type.
      const joined = (
        await reader.sql(
          `SELECT s.name, e.run_id FROM vala.eval.observations e
             JOIN vala.traces.spans s ON e.trace_id = s.trace_id AND e.span_id = s.span_id
             WHERE e.run_id = '${run.runId}'`,
        )
      )
        .toArrow()
        .toArray()
        .map((row) => row.toJSON());
      expect(joined).toEqual([{ name: "observe", run_id: run.runId }]);

      // Two tables, two scopes, one invocation: each row keeps the subject of
      // the view that wrote it, so a second table never inherits the first's.
      for (const [table, values, subject] of [
        [dataset, [41n, 43n], agentUid],
        [datasetB, [42n], modelUid],
      ] as const) {
        const rows = (
          await reader.sql(
            `SELECT value, run_id, card_uid FROM ${table}
               WHERE run_id = '${run.runId}' ORDER BY value`,
          )
        )
          .toArrow()
          .toArray()
          .map((row) => row.toJSON());
        expect(rows).toEqual(values.map((value) => ({ value, run_id: run.runId, card_uid: subject })));
      }
    } finally {
      context.disable();
      server.shutdown();
    }
  }, 90_000);
});

const BURST_OBSERVATIONS = 1_000;
const BURST_FEATURES = 9;
const UNSEALABLE_BUDGET = 1024;
const BURST_BUDGET = 16 * 1024 * 1024;

describe("Drift burst under a byte-budget override", () => {
  it("lands every row of a 1,000 x 9 burst exactly once", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-burst-"));
    const service = writeServiceGraph(root);
    const bundle = join(root, "bundle");
    const server = startTestServer();
    try {
      const cards = Cards.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
      await cards.registerFromPath(join(root, "observe-prompt.yaml"));
      const receipt = await cards.registerFromPath(service);
      await cards.hydrate(receipt.root, bundle);
      const credential = server.credentialRegisteredService(
        `${receipt.root.space}/Service/${receipt.root.name}@${receipt.root.version}`,
        [],
      );
      const state = WyrdState.fromPath(bundle);
      const connection = { serverUrl: server.baseUrl, credential, grpcUrl: server.grpcUrl };

      const unsealable = await rejection(
        state.startBifrost({ ...connection, clientByteLimitBytes: UNSEALABLE_BUDGET }),
      );
      expect(unsealable.code).toBe("WYRD_CLIENT_400_CONFIG_INVALID");
      await state.startBifrost({ ...connection, clientByteLimitBytes: BURST_BUDGET });

      const run = state.run();
      const model = run.forCard("model");
      for (let observation = 0; observation < BURST_OBSERVATIONS; observation += 1) {
        const features = Object.fromEntries(
          Array.from({ length: BURST_FEATURES }, (_, feature) => [
            `feature_${feature}`,
            observation + feature / 10,
          ]),
        );
        // Admission is all-or-none per observation, so a refused observation
        // admitted no row and resubmitting it after a flush duplicates nothing.
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

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      const counts = (
        await reader.sql(
          `SELECT COUNT(*) AS n FROM vala.drift.observations
             WHERE run_id = '${run.runId}' GROUP BY record_id`,
        )
      )
        .toArrow()
        .toArray()
        .map((row) => Number(row.toJSON().n));
      expect(counts).toHaveLength(BURST_OBSERVATIONS);
      expect(new Set(counts)).toEqual(new Set([BURST_FEATURES]));
      expect(counts.reduce((total, n) => total + n, 0)).toBe(BURST_OBSERVATIONS * BURST_FEATURES);
      await reader.shutdown();
    } finally {
      server.shutdown();
    }
  }, 120_000);
});
