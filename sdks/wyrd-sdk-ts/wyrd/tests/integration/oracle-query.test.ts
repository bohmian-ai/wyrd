import { Metadata } from "@grpc/grpc-js";
import { SeverityNumber } from "@opentelemetry/api-logs";
import {
  SpanKind,
  SpanStatusCode,
  TraceFlags,
  ValueType,
  context,
  createTraceState,
  trace,
} from "@opentelemetry/api";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-grpc";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import type { NativeWyrdTestServer } from "@wyrd/testing";
import { startTestServer } from "@wyrd/testing";
import { RecordBatch, Struct, makeBuilder, makeData, tableFromIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import { Bifrost, IncompleteQueryStreamError, TableConfig, WyrdError } from "@wyrd/sdk";

describe("Oracle query journey", () => {
  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      server.seedBifrostRows(server.tableFqn, [11, 22]);
      server.waitForBifrostPublication();
      // The third row stays live in Scribe; one query reads both sources.
      server.seedBifrostRows(server.tableFqn, [33]);
      const expected = [11, 22, 33];
      const stream = await client.stream({
        sql: `SELECT * FROM ${server.tableFqn} ORDER BY value`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(3);
      expect(batches[0]?.schema.fields[0]?.name).toBe("value");
      expect(batches[0]?.schema.fields.length).toBeGreaterThan(1);
      const values = batches.flatMap((batch) =>
        Array.from(batch.getChildAt(0)?.toArray() ?? []),
      );
      expect(values).toEqual(expected.map(BigInt));
      expect(stream.terminal).toBeDefined();
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(3);
      expect(stream.terminal?.warnings).toEqual([]);
      expect(stream.terminal).not.toHaveProperty("freshness");
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("uses schema-once IPC with explicit EOS", async () => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [51, 52, 53]);
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const stream = await client.stream({
        sql: `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.length).toBeGreaterThan(0);
      // One query is one Arrow IPC stream, so every decoded batch carries the
      // query's single schema rather than one re-declared per batch.
      const fields = batches.map((batch) =>
        batch.schema.fields.map((field) => field.name).join(","),
      );
      expect(new Set(fields).size).toBe(1);
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(3);
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(3);
      // The terminal closes that stream explicitly; an empty delta would mean a
      // truncated result rather than a complete one.
      expect(stream.terminal?.arrow_ipc_eos).toEqual([255, 255, 255, 255, 0, 0, 0, 0]);
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("converts a collected result to Arrow and to IPC bytes", async () => {
    const server = startTestServer();
    try {
      const expected = [61, 62, 63];
      server.seedBifrostRows(server.tableFqn, expected);
      server.waitForBifrostPublication();
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const result = await client.sql(
        `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      );

      // Both conversions are views over the batches the result already holds,
      // so each must carry exactly the rows the result reports.
      const table = result.toArrow();
      expect(table.numRows).toBe(result.numRows);
      expect(Array.from(table.getChild("value")?.toArray() ?? [])).toEqual(
        expected.map(BigInt),
      );

      const decoded = tableFromIPC(result.toBytes());
      expect(decoded.numRows).toBe(result.numRows);
      expect(decoded.schema.fields.map((field) => field.name)).toEqual(
        table.schema.fields.map((field) => field.name),
      );
      expect(Array.from(decoded.getChild("value")?.toArray() ?? [])).toEqual(
        expected.map(BigInt),
      );
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it.each([
    ["schema", "failNextQueryAfterSchema"],
    ["batch", "failNextQueryAfterBatch"],
  ] as const)("fails closed on EOF after %s", async (_label, fault) => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [31, 32]);
      server[fault]();
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const stream = await client.stream({
        sql: `SELECT value FROM ${server.tableFqn} ORDER BY value`,
      });
      await expect((async () => {
        for await (const _batch of stream) {
          // Drain until the deterministic fault closes the native stream.
        }
      })()).rejects.toBeInstanceOf(IncompleteQueryStreamError);
      await expect(stream.next()).resolves.toMatchObject({ done: true });
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("rejects an authenticated token before Oracle work", async () => {
    const server = startTestServer();
    try {
      server.seedBifrostRows(server.tableFqn, [41, 42]);
      const before = server.bifrostReadDecisionCount();
      const denied = server.queryDeniedToken();
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: denied,
        grpcUrl: server.grpcUrl,
      });
      await expect(
        client.stream({ sql: `SELECT * FROM ${server.tableFqn}` }),
      ).rejects.toMatchObject({
        code: "WYRD_PERMISSION_403_DENIED_RBAC",
        status: 403,
        title: "Permission denied (RBAC)",
        details: expect.any(Object),
      } satisfies Partial<WyrdError>);
      expect(server.bifrostReadDecisionCount()).toBe(before);
    } finally {
      server.shutdown();
    }
  }, 15_000);

  it("rejects out-of-range deadlines with the shared validation error", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      for (const deadlineMs of [0, 4_294_967_296, 1.5, Number.NaN]) {
        await expect(
          client.stream({ sql: "SELECT 1", deadlineMs }),
        ).rejects.toMatchObject({
          code: "WYRD_VALA_400_QUERY_INVALID_SQL",
          status: 400,
        } satisfies Partial<WyrdError>);
      }
    } finally {
      server.shutdown();
    }
  }, 15_000);
});

const MODEL = "claude-opus-5";
const INPUT_TOKENS = 1280;
const OUTPUT_TOKENS = 320;
const INPUT_MESSAGES =
  '[{"role":"user","parts":[{"type":"text","content":"summarize the incident"}]}]';
const OUTPUT_MESSAGES =
  '[{"role":"assistant","parts":[{"type":"text","content":"the writer stalled"}]}]';
const LOG_BODY = "tool call exhausted its retry budget";
const EVENT_NAME = "gen_ai.choice";
const LINK_TRACE_STATE = "wyrd=fixture";
const COUNTER_VALUE = 7;
const GAUGE_VALUE = 0.75;
const HISTOGRAM_VALUES = [1, 2.5, 3, 6];
const SERVICE = "wyrd.fixture.service";

/** Builds the gRPC metadata a stock OTLP exporter authenticates with. */
function exporterMetadata(server: NativeWyrdTestServer): Metadata {
  const metadata = new Metadata();
  metadata.set("x-wyrd-access-token", `Bearer ${server.token}`);
  return metadata;
}

/**
 * Exports the GenAI chat, its failing tool call, the tool's log, and three
 * metrics through the stock OpenTelemetry SDK and OTLP exporters, so the
 * canonical rows and their Variant payloads come from the server's own ingress.
 */
async function exportCanonicalSignals(
  server: NativeWyrdTestServer,
  scope: string,
): Promise<void> {
  const resource = resourceFromAttributes({ "service.name": SERVICE });
  const traces = new BasicTracerProvider({
    resource,
    spanProcessors: [
      new BatchSpanProcessor(
        new OTLPTraceExporter({ url: server.grpcUrl, metadata: exporterMetadata(server) }),
      ),
    ],
  });
  const logs = new LoggerProvider({
    resource,
    processors: [
      new BatchLogRecordProcessor({
        exporter: new OTLPLogExporter({
          url: server.grpcUrl,
          metadata: exporterMetadata(server),
        }),
      }),
    ],
  });
  const tracer = traces.getTracer(scope, "1.0.0");
  const parent = tracer.startSpan("chat claude-opus-5", {
    kind: SpanKind.CLIENT,
    links: [
      {
        context: {
          traceId: "00000000000000000000000000000001",
          spanId: "0000000000000002",
          traceFlags: TraceFlags.SAMPLED,
          traceState: createTraceState(LINK_TRACE_STATE),
        },
      },
    ],
  });
  parent.setAttributes({
    "gen_ai.operation.name": "chat",
    "gen_ai.request.model": MODEL,
    "gen_ai.usage.input_tokens": INPUT_TOKENS,
    "gen_ai.usage.output_tokens": OUTPUT_TOKENS,
    "gen_ai.input.messages": INPUT_MESSAGES,
    "gen_ai.output.messages": OUTPUT_MESSAGES,
  });
  parent.addEvent(EVENT_NAME, { "gen_ai.finish_reason": "stop" });
  parent.setStatus({ code: SpanStatusCode.OK });
  const child = tracer.startSpan(
    "execute_tool search",
    undefined,
    trace.setSpan(context.active(), parent),
  );
  child.setAttributes({
    "gen_ai.operation.name": "execute_tool",
    "gen_ai.request.model": MODEL,
    "gen_ai.tool.name": "search",
    "gen_ai.usage.input_tokens": 64,
    "gen_ai.usage.output_tokens": 16,
  });
  // `@opentelemetry/api` has no ambient context manager until an application
  // registers one, so the record names the tool span's context explicitly.
  logs.getLogger(scope, "1.0.0").emit({
    body: LOG_BODY,
    severityNumber: SeverityNumber.ERROR,
    severityText: "ERROR",
    attributes: { "gen_ai.tool.name": "search" },
    context: trace.setSpan(context.active(), child),
  });
  child.setStatus({ code: SpanStatusCode.ERROR, message: LOG_BODY });
  child.end();
  parent.end();
  await traces.shutdown();
  await logs.shutdown();

  // Shutdown is the single metric export: the SDK aggregates cumulatively, so
  // a flush followed by shutdown would write each running total twice.
  const metrics = new MeterProvider({
    resource,
    readers: [
      new PeriodicExportingMetricReader({
        exporter: new OTLPMetricExporter({
          url: server.grpcUrl,
          metadata: exporterMetadata(server),
        }),
        exportIntervalMillis: 600_000,
      }),
    ],
  });
  const meter = metrics.getMeter(scope, "1.0.0");
  const attributes = { "gen_ai.request.model": MODEL };
  meter
    .createCounter("wyrd.fixture.requests", { valueType: ValueType.INT })
    .add(COUNTER_VALUE, attributes);
  meter.createGauge("wyrd.fixture.saturation").record(GAUGE_VALUE, attributes);
  const latency = meter.createHistogram("wyrd.fixture.latency");
  for (const value of HISTOGRAM_VALUES) {
    latency.record(value, attributes);
  }
  await metrics.shutdown();
}

describe("Canonical signal journey", () => {
  it("canonical signal Arrow write and SQL read round-trip", async () => {
    const server = startTestServer();
    try {
      const scope = `wyrd.ts.canonical.${Date.now()}`;
      await exportCanonicalSignals(server, scope);
      server.waitForBifrostPublication();
      const transport = {
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      };
      const writer = await Bifrost.connect(transport);

      const hierarchy = (
        await writer.sql(
          `SELECT name, gen_ai_operation_name, status_code, ` +
            `CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) AS is_root ` +
            `FROM vala.traces.spans WHERE scope_name = '${scope}' ` +
            `ORDER BY is_root DESC`,
        )
      ).toArrow();
      expect(
        Array.from(hierarchy.getChild("gen_ai_operation_name")?.toArray() ?? []),
      ).toEqual(["chat", "execute_tool"]);
      expect(Array.from(hierarchy.getChild("is_root")?.toArray() ?? [])).toEqual([
        1n,
        0n,
      ]);
      expect(Array.from(hierarchy.getChild("status_code")?.toArray() ?? [])).toEqual([
        1, 2,
      ]);

      const tokens = (
        await writer.sql(
          `SELECT CAST(SUM(gen_ai_usage_input_tokens) AS BIGINT) AS input_tokens, ` +
            `CAST(SUM(gen_ai_usage_output_tokens) AS BIGINT) AS output_tokens, ` +
            `CAST(COUNT(*) AS BIGINT) AS spans FROM vala.traces.spans ` +
            `WHERE scope_name = '${scope}' AND gen_ai_request_model = '${MODEL}'`,
        )
      ).toArrow();
      expect(tokens.get(0)?.toJSON()).toEqual({
        input_tokens: BigInt(INPUT_TOKENS + 64),
        output_tokens: BigInt(OUTPUT_TOKENS + 16),
        spans: 2n,
      });

      const correlated = await writer.sql(
        `SELECT l.severity_text, l.body, ` +
          `l.attributes ->> 'gen_ai.tool.name' AS tool, s.name AS span_name ` +
          `FROM vala.logs.records l JOIN vala.traces.spans s ` +
          `ON l.trace_id = s.trace_id AND l.span_id = s.span_id ` +
          `WHERE l.scope_name = '${scope}'`,
        { parse: (row) => row as Record<string, unknown> },
      );
      expect(correlated).toEqual([
        {
          severity_text: "ERROR",
          body: LOG_BODY,
          tool: "search",
          span_name: "execute_tool search",
        },
      ]);

      const metrics = (
        await writer.sql(
          `SELECT metric_type, ` +
            `CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, ` +
            `CAST(SUM(COALESCE(double_value, 0.0)) AS DOUBLE) AS doubles, ` +
            `CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations, ` +
            `CAST(SUM(COALESCE(histogram_sum, 0.0)) AS DOUBLE) AS observed ` +
            `FROM vala.metrics.points WHERE scope_name = '${scope}' ` +
            `GROUP BY metric_type ORDER BY metric_type`,
        )
      ).toArrow();
      expect(metrics.toArray().map((row) => row.toJSON())).toEqual([
        {
          metric_type: "gauge",
          ints: 0n,
          doubles: GAUGE_VALUE,
          observations: 0n,
          observed: 0,
        },
        {
          metric_type: "histogram",
          ints: 0n,
          doubles: 0,
          observations: BigInt(HISTOGRAM_VALUES.length),
          observed: HISTOGRAM_VALUES.reduce((sum, value) => sum + value, 0),
        },
        {
          metric_type: "sum",
          ints: BigInt(COUNTER_VALUE),
          doubles: 0,
          observations: 0n,
          observed: 0,
        },
      ]);

      const payloadReader = await Bifrost.connect({
        ...transport,
        credential: server.scopedApiKey("ts_canonical_reader", [
          "bifrost_query:read",
        ]),
      });
      const [payload] = await payloadReader.sql(
        `SELECT CAST(array_length(events) AS BIGINT) AS events, ` +
          `CAST(array_length(links) AS BIGINT) AS links, ` +
          `events[1]['name'] AS event_name, ` +
          `events[1]['attributes'] ->> 'gen_ai.finish_reason' AS finish_reason, ` +
          `links[1]['trace_state'] AS link_state, attributes, ` +
          `resource_attributes ->> 'service.name' AS service ` +
          `FROM vala.traces.spans ` +
          `WHERE scope_name = '${scope}' AND parent_span_id IS NULL`,
        { parse: (row) => row as Record<string, any> },
      );
      expect(payload).toMatchObject({
        events: 1n,
        links: 1n,
        event_name: EVENT_NAME,
        finish_reason: "stop",
        link_state: LINK_TRACE_STATE,
        service: SERVICE,
      });
      expect(payload?.attributes).toMatchObject({
        "gen_ai.input.messages": INPUT_MESSAGES,
        "gen_ai.output.messages": OUTPUT_MESSAGES,
        "gen_ai.usage.input_tokens": INPUT_TOKENS,
      });
    } finally {
      server.shutdown();
    }
  }, 60_000);
});

describe("Variant query journey", () => {
  it("builtin Variant and Struct payloads are queryable", async () => {
    const server = startTestServer();
    try {
      const scope = `wyrd.ts.variant.${Date.now()}`;
      const provider = new BasicTracerProvider({
        resource: resourceFromAttributes({ "service.name": SERVICE }),
        spanProcessors: [
          new BatchSpanProcessor(
            new OTLPTraceExporter({
              url: server.grpcUrl,
              metadata: exporterMetadata(server),
            }),
          ),
        ],
      });
      const span = provider.getTracer(scope, "1.0.0").startSpan("variant-parent");
      span.setAttribute("gen_ai.request.model", MODEL);
      span.setAttribute("wyrd.test.big", 2 ** 60);
      span.setAttribute("wyrd.test.values", [1, 2, 3]);
      span.addEvent(EVENT_NAME, { "gen_ai.finish_reason": "stop" });
      span.end();
      await provider.forceFlush();
      await provider.shutdown();
      server.waitForBifrostPublication();

      const reader = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const where = `FROM vala.traces.spans WHERE scope_name = '${scope}'`;
      const raw = await reader.sql(`SELECT attributes ${where}`);
      expect(
        raw.schema.fields[0]?.metadata.get("ARROW:extension:name"),
        "the Arrow terminal keeps the Variant extension",
      ).toBe("arrow.parquet.variant");

      const rows = await reader.sql(
        `SELECT attributes ->> 'gen_ai.request.model' AS model, ` +
          `resource_attributes ->> 'service.name' AS service, ` +
          `attributes -> 'absent' AS absent, attributes, ` +
          `events[1]['name'] AS event_name, ` +
          `events[1]['attributes'] ->> 'gen_ai.finish_reason' AS finish_reason, ` +
          `parse_json('{"n": 9007199254740993, "u": 18446744073709551615, "a": [1, "x", null]}') AS parsed, ` +
          `try_parse_json('{bad') AS lenient ${where}`,
        { parse: (row) => ({ ...(row as Record<string, unknown>) }) },
      );
      expect(rows).toEqual([
        {
          model: MODEL,
          service: SERVICE,
          absent: null,
          attributes: {
            "gen_ai.request.model": MODEL,
            "wyrd.test.big": 2n ** 60n,
            "wyrd.test.values": [1, 2, 3],
          },
          event_name: EVENT_NAME,
          finish_reason: "stop",
          parsed: { n: 9007199254740993n, u: 2n ** 64n - 1n, a: [1, "x", null] },
          lenient: null,
        },
      ]);

      const early = await reader
        .sql(`SELECT parse_json('{bad') AS v ${where}`)
        .then(
          () => undefined,
          (error: unknown) => error,
        );
      expect(early).toMatchObject({ code: "WYRD_VALA_400_VARIANT_INVALID_JSON" });

      // A failure after a delivered batch keeps the pre-stream problem: one
      // published object streams 8192-row batches in id order, so rows from
      // id 8192 fail only in the second batch. An unrelated late cast failure
      // stays generic, and neither result is returned partially.
      const lateFqn = `vala.datasets.variant_late_${Date.now().toString(36)}`;
      const writer = await Bifrost.connect({
        table: TableConfig.fromJsonSchema(lateFqn, {
          type: "object",
          properties: { id: { type: "integer" } },
          required: ["id"],
        }),
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      expect(await writer.register()).toBe("created");
      const lateSchema = writer.table!.arrowSchema;
      const ids = makeBuilder({ type: lateSchema.fields[0]!.type });
      for (let id = 0n; id < 10_000n; id += 1n) ids.append(id);
      ids.finish();
      await writer.writeBatch(
        lateFqn,
        new RecordBatch(
          lateSchema,
          makeData({
            type: new Struct(lateSchema.fields),
            length: 10_000,
            children: [ids.flush()],
          }),
        ),
      );
      await writer.shutdown();
      server.waitForBifrostPublication();
      const { code, status, detail, details } = early as WyrdError;
      for (const [value, expected] of [
        [
          "parse_json(CASE WHEN id < 8192 THEN '1' ELSE '{bad' END)",
          { code, status, detail, details },
        ],
        [
          "CAST(CASE WHEN id < 8192 THEN '1' ELSE 'x' END AS BIGINT)",
          {
            code: "WYRD_VALA_500_QUERY_EXECUTION_FAILED",
            details: { variant: "query_execution_failed" },
          },
        ],
      ] as const) {
        const sql = `SELECT id, ${value} AS v FROM ${lateFqn}`;
        let delivered = 0;
        const streamed = await (async () => {
          for await (const batch of await reader.stream({ sql })) {
            delivered += batch.numRows;
          }
        })().then(
          () => undefined,
          (error: unknown) => error,
        );
        expect(delivered, "one valid batch preceded the failure").toBe(8192);
        expect(streamed).toBeInstanceOf(WyrdError);
        expect(streamed).toMatchObject(expected);
        await expect(reader.sql(sql)).rejects.toMatchObject(expected);
      }
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
