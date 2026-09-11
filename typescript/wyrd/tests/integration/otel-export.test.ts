import { Metadata } from "@grpc/grpc-js";
import { SeverityNumber } from "@opentelemetry/api-logs";
import { SpanStatusCode, TraceFlags, ValueType, context, trace } from "@opentelemetry/api";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-grpc";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import type { NativeWyrdTestServer } from "@wyrd/testing";
import { startTestServer } from "@wyrd/testing";
import type { Table } from "apache-arrow";
import { describe, expect, it } from "vitest";

import { Bifrost } from "@wyrd/sdk";

/** `service.name` every stock exporter in this file declares. */
const SERVICE_NAME = "wyrd-typescript-journey";
/** Instrumentation scope version every stock exporter declares. */
const SCOPE_VERSION = "1.2.3";
/** Instrumentation scope the stock tracer records under. */
const TRACE_SCOPE = "wyrd.tests.stock.ts.trace";
/** Instrumentation scope the stock logger records under. */
const LOG_SCOPE = "wyrd.tests.stock.ts.log";
/** Instrumentation scope the stock meter records under. */
const METRIC_SCOPE = "wyrd.tests.stock.ts.metric";

/** The GenAI values every canonical write path in this change converges on. */
const GEN_AI = {
  operationName: "chat",
  providerName: "anthropic",
  requestModel: "claude-opus-5",
  conversationId: "conv-canonical-0001",
  inputTokens: 4096,
  outputTokens: 512,
  inputMessages:
    '[{"role":"user","parts":[{"type":"text","content":"summarize the canonical ledger"}]}]',
  outputMessages:
    '[{"role":"assistant","parts":[{"type":"text","content":"the ledger is canonical"}],"finish_reason":"stop"}]',
} as const;

/** Builds the gRPC metadata a stock OTLP exporter authenticates with. */
function exporterMetadata(server: NativeWyrdTestServer): Metadata {
  const metadata = new Metadata();
  metadata.set("x-wyrd-access-token", `Bearer ${server.token}`);
  return metadata;
}

/** The resource every stock exporter in this file is configured with. */
function journeyResource() {
  return resourceFromAttributes({ "service.name": SERVICE_NAME });
}

/** Publishes the acknowledged rows, then reads one canonical query back. */
async function readCanonical(
  server: NativeWyrdTestServer,
  sql: string,
): Promise<Table> {
  server.waitForBifrostPublication();
  const client = await Bifrost.connect({
    serverUrl: server.baseUrl,
    credential: server.token,
    grpcUrl: server.grpcUrl,
  });
  const result = await client.sql(sql);
  expect(result.terminal?.outcome).toBe("success");
  return result.toArrow();
}

/** Reads one column's row values out of a queried canonical table. */
function values<T>(table: Table, column: string): T[] {
  const child = table.getChild(column);
  expect(child, `the result carries a \`${column}\` column`).not.toBeNull();
  return Array.from(child?.toArray() ?? []) as T[];
}

/** Renders one stored identifier column value as lowercase hex. */
function hex(value: unknown): string {
  return Buffer.from(value as Uint8Array).toString("hex");
}

/**
 * Reads a stored attribute blob as text.
 *
 * The canonical `KeyValueList` encoding embeds every string value literally,
 * and the TypeScript SDK ships no protobuf decoder, so a payload assertion
 * looks for the exact JSON the application emitted rather than adding a
 * decoder to the test tree. Exact structural fidelity of the same blob is
 * proven by the raw OTLP suite in Rust.
 */
function blobText(value: unknown): string {
  return Buffer.from(value as Uint8Array).toString("utf8");
}

describe("stock OpenTelemetry exporters", () => {
  it("stock OpenTelemetry tracer exports GenAI span to Bifrost", async () => {
    const server = startTestServer();
    try {
      const provider = new BasicTracerProvider({
        resource: journeyResource(),
        spanProcessors: [
          new BatchSpanProcessor(
            new OTLPTraceExporter({
              url: server.grpcUrl,
              metadata: exporterMetadata(server),
            }),
          ),
        ],
      });
      const tracer = provider.getTracer(TRACE_SCOPE, SCOPE_VERSION);

      const parent = tracer.startSpan("stock-ts-parent", {
        links: [
          {
            context: {
              traceId: "0102030405060708090a0b0c0d0e0f10",
              spanId: "1112131415161718",
              traceFlags: TraceFlags.SAMPLED,
            },
          },
        ],
      });
      parent.setAttribute("wyrd.test.marker", "typescript-trace");
      parent.setAttribute("gen_ai.operation.name", GEN_AI.operationName);
      parent.setAttribute("gen_ai.provider.name", GEN_AI.providerName);
      parent.setAttribute("gen_ai.request.model", GEN_AI.requestModel);
      parent.setAttribute("gen_ai.conversation.id", GEN_AI.conversationId);
      parent.setAttribute("gen_ai.usage.input_tokens", GEN_AI.inputTokens);
      parent.setAttribute("gen_ai.usage.output_tokens", GEN_AI.outputTokens);
      parent.setAttribute("gen_ai.input.messages", GEN_AI.inputMessages);
      parent.setAttribute("gen_ai.output.messages", GEN_AI.outputMessages);
      parent.addEvent("checkpoint", { step: 1 });
      parent.setStatus({
        code: SpanStatusCode.ERROR,
        message: "expected test status",
      });
      const child = tracer.startSpan(
        "stock-ts-child",
        undefined,
        trace.setSpan(context.active(), parent),
      );
      child.setAttribute("answer", 42);
      child.end();
      parent.end();
      const emitted = {
        traceId: parent.spanContext().traceId,
        parentSpanId: parent.spanContext().spanId,
        childSpanId: child.spanContext().spanId,
      };
      await provider.forceFlush();
      await provider.shutdown();

      const table = await readCanonical(
        server,
        `SELECT * FROM vala.traces.spans WHERE scope_name = '${TRACE_SCOPE}'`,
      );
      expect(table.numRows).toBe(2);
      const names = values<string>(table, "name");
      const parentRow = names.indexOf("stock-ts-parent");
      const childRow = names.indexOf("stock-ts-child");
      expect(parentRow).toBeGreaterThanOrEqual(0);
      expect(childRow).toBeGreaterThanOrEqual(0);

      const traceIds = values(table, "trace_id");
      expect(hex(traceIds[parentRow])).toBe(emitted.traceId);
      expect(hex(traceIds[childRow])).toBe(emitted.traceId);
      expect(hex(values(table, "span_id")[parentRow])).toBe(emitted.parentSpanId);
      expect(hex(values(table, "parent_span_id")[childRow])).toBe(
        emitted.parentSpanId,
      );
      expect(hex(values(table, "span_id")[childRow])).toBe(emitted.childSpanId);

      expect(values<string>(table, "service_name")[parentRow]).toBe(SERVICE_NAME);
      expect(values<string>(table, "scope_version")[parentRow]).toBe(SCOPE_VERSION);
      expect(values(table, "status_code")[parentRow]).toBe(2);
      expect(values<string>(table, "status_message")[parentRow]).toBe(
        "expected test status",
      );
      expect(values<string>(table, "gen_ai_operation_name")[parentRow]).toBe(
        GEN_AI.operationName,
      );
      expect(values<string>(table, "gen_ai_provider_name")[parentRow]).toBe(
        GEN_AI.providerName,
      );
      expect(values<string>(table, "gen_ai_request_model")[parentRow]).toBe(
        GEN_AI.requestModel,
      );
      expect(values<string>(table, "gen_ai_conversation_id")[parentRow]).toBe(
        GEN_AI.conversationId,
      );
      expect(
        Number(values(table, "gen_ai_usage_input_tokens")[parentRow]),
      ).toBe(GEN_AI.inputTokens);
      expect(
        Number(values(table, "gen_ai_usage_output_tokens")[parentRow]),
      ).toBe(GEN_AI.outputTokens);

      const attributes = blobText(values(table, "attributes")[parentRow]);
      expect(attributes).toContain("typescript-trace");
      expect(attributes).toContain(GEN_AI.inputMessages);
      expect(attributes).toContain(GEN_AI.outputMessages);
      expect(blobText(values(table, "resource_attributes")[parentRow])).toContain(
        SERVICE_NAME,
      );

      const events = table.getChild("events")?.get(parentRow);
      expect(events?.length).toBe(1);
      const links = table.getChild("links")?.get(parentRow);
      expect(links?.length).toBe(1);
    } finally {
      server.shutdown();
    }
  }, 30_000);

  it("stock OpenTelemetry logger exports correlated log to Bifrost", async () => {
    const server = startTestServer();
    try {
      const logs = new LoggerProvider({
        resource: journeyResource(),
        processors: [
          new BatchLogRecordProcessor({
            exporter: new OTLPLogExporter({
              url: server.grpcUrl,
              metadata: exporterMetadata(server),
            }),
          }),
        ],
      });
      const logger = logs.getLogger(LOG_SCOPE, SCOPE_VERSION);

      // The correlation this case proves is the SDK's, so the span it
      // correlates to comes from an ordinary tracer with no exporter: this
      // case owns log transport, and a second exporting trace pipeline would
      // write rows nothing here asserts.
      const contextTraces = new BasicTracerProvider({ resource: journeyResource() });
      const correlated = contextTraces
        .getTracer("wyrd.tests.stock.ts.log-context")
        .startSpan("stock-ts-log-context");
      // The record names its context explicitly rather than relying on an
      // ambient one: `@opentelemetry/api` installs a no-op context manager
      // until an application registers `AsyncLocalStorageContextManager`, so
      // `context.with` alone would leave the record uncorrelated. Passing the
      // context is the upstream API for exactly this case.
      logger.emit({
        body: "order delayed",
        severityNumber: SeverityNumber.ERROR,
        severityText: "ERROR",
        attributes: { "wyrd.test.marker": "typescript-log" },
        context: trace.setSpan(context.active(), correlated),
      });
      correlated.end();
      await logs.forceFlush();
      await logs.shutdown();

      const table = await readCanonical(
        server,
        `SELECT * FROM vala.logs.records WHERE scope_name = '${LOG_SCOPE}'`,
      );
      expect(table.numRows).toBe(1);
      expect(blobText(values(table, "body")[0])).toContain("order delayed");
      expect(values(table, "severity_number")[0]).toBe(SeverityNumber.ERROR);
      expect(values<string>(table, "severity_text")[0]).toBe("ERROR");
      expect(blobText(values(table, "attributes")[0])).toContain("typescript-log");
      expect(blobText(values(table, "resource_attributes")[0])).toContain(
        SERVICE_NAME,
      );
      expect(hex(values(table, "trace_id")[0])).toBe(
        correlated.spanContext().traceId,
      );
      expect(hex(values(table, "span_id")[0])).toBe(
        correlated.spanContext().spanId,
      );
    } finally {
      server.shutdown();
    }
  }, 30_000);

  it("stock OpenTelemetry meter exports representative metrics to Bifrost", async () => {
    const server = startTestServer();
    try {
      // Shutdown is the single export boundary here rather than a flush
      // followed by a shutdown: the SDK aggregates cumulatively, so both
      // would export the same running totals and the table would carry two
      // indistinguishable points per instrument.
      const metrics = new MeterProvider({
        resource: journeyResource(),
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
      const meter = metrics.getMeter(METRIC_SCOPE, SCOPE_VERSION);
      const attributes = { "wyrd.test.marker": "typescript-metric" };
      // JavaScript has one number type, so the instruments that carry whole
      // counts declare `ValueType.INT` explicitly. Without it the SDK exports
      // them as doubles and the canonical integer column stays null.
      meter
        .createCounter("orders.created", { valueType: ValueType.INT })
        .add(7, attributes);
      meter
        .createUpDownCounter("orders.active", { valueType: ValueType.INT })
        .add(-2, attributes);
      meter.createGauge("queue.depth").record(3.5, attributes);
      meter
        .createHistogram("request.duration", { unit: "ms" })
        .record(12.5, attributes);
      await metrics.shutdown();

      const table = await readCanonical(
        server,
        `SELECT * FROM vala.metrics.points WHERE scope_name = '${METRIC_SCOPE}'`,
      );
      const names = values<string>(table, "metric_name");
      expect(new Set(names)).toEqual(
        new Set([
          "orders.created",
          "orders.active",
          "queue.depth",
          "request.duration",
        ]),
      );
      const types = values<string>(table, "metric_type");
      const intValues = values(table, "int_value");
      const doubleValues = values(table, "double_value");
      const monotonic = values(table, "is_monotonic");

      const created = names.indexOf("orders.created");
      expect(types[created]).toBe("sum");
      expect(Number(intValues[created])).toBe(7);
      expect(monotonic[created]).toBe(true);

      const active = names.indexOf("orders.active");
      expect(types[active]).toBe("sum");
      expect(Number(intValues[active])).toBe(-2);
      expect(monotonic[active]).toBe(false);

      const depth = names.indexOf("queue.depth");
      expect(types[depth]).toBe("gauge");
      expect(doubleValues[depth]).toBeCloseTo(3.5);

      const duration = names.indexOf("request.duration");
      expect(types[duration]).toBe("histogram");
      expect(values<string>(table, "unit")[duration]).toBe("ms");
      expect(Number(values(table, "histogram_count")[duration])).toBe(1);
      expect(values(table, "histogram_sum")[duration]).toBeCloseTo(12.5);
      const bounds = table.getChild("explicit_bounds")?.get(duration);
      const counts = table.getChild("bucket_counts")?.get(duration);
      expect(counts?.length).toBe((bounds?.length ?? 0) + 1);
      expect(
        Array.from(counts ?? []).reduce(
          (total: number, count) => total + Number(count),
          0,
        ),
      ).toBe(1);

      for (const index of [created, active, depth, duration]) {
        expect(blobText(values(table, "attributes")[index])).toContain(
          "typescript-metric",
        );
      }
    } finally {
      server.shutdown();
    }
  }, 30_000);
});
