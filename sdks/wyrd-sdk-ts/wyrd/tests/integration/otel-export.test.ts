import { SeverityNumber } from "@opentelemetry/api-logs";
import { SpanStatusCode, ValueType, context, trace } from "@opentelemetry/api";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-grpc";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 30_000 });

/** A stored fixed-width trace or span identity, read as lower-case hex. */
const HexId = z.instanceof(Uint8Array).transform((bytes) => Buffer.from(bytes).toString("hex"));

const SpanRow = z.object({
  name: z.string(),
  trace_id: HexId,
  span_id: HexId,
  parent_span_id: HexId.nullable(),
  status_code: z.number(),
  status_message: z.string().nullable(),
  gen_ai_request_model: z.string().nullable(),
  gen_ai_usage_input_tokens: z.bigint().nullable(),
});

const LogRow = z.object({
  severity_text: z.string().nullable(),
  severity_number: z.number().nullable(),
  trace_id: HexId.nullable(),
  span_id: HexId.nullable(),
});

const MetricRow = z.object({
  metric_name: z.string(),
  metric_type: z.string(),
  int_value: z.bigint().nullable(),
  double_value: z.number().nullable(),
  histogram_count: z.bigint().nullable(),
  histogram_sum: z.number().nullable(),
});

/**
 * Every stock OTLP exporter configured only by `OTEL_EXPORTER_OTLP_*`, the way
 * a deployment points one at Wyrd's gRPC address and authenticates it with
 * the `x-wyrd-api-key` header, and a reader for what lands.
 */
const test = serverTest().extend<{ otlp: void; bifrost: Bifrost }>({
  otlp: [
    async ({ server, deployment: _ }, use) => {
      vi.stubEnv("OTEL_EXPORTER_OTLP_ENDPOINT", server.grpcUrl);
      vi.stubEnv("OTEL_EXPORTER_OTLP_HEADERS", `x-wyrd-api-key=${server.apiKey}`);
      await use();
    },
    { auto: true },
  ],
  bifrost: [async ({ server: _ }, use) => use(await Bifrost.connect()), { scope: "file" }],
});

test("stock exporter span reads back through bifrost", async ({ server, bifrost }) => {
  const traces = new BasicTracerProvider({ spanProcessors: [new BatchSpanProcessor(new OTLPTraceExporter())] });
  const tracer = traces.getTracer("wyrd.tests.otel.trace", "1.0.0");
  const answer = tracer.startSpan("answer", {
    attributes: { "gen_ai.request.model": "claude-opus-5", "gen_ai.usage.input_tokens": 4096 },
  });
  answer.setStatus({ code: SpanStatusCode.ERROR, message: "refused" });
  tracer.startSpan("retrieve", undefined, trace.setSpan(context.active(), answer)).end();
  answer.end();
  await traces.shutdown();
  server.flushBifrost();

  const rows = await bifrost.sql(
    `SELECT name, trace_id, span_id, parent_span_id, status_code, status_message,
            gen_ai_request_model, gen_ai_usage_input_tokens
       FROM vala.traces.spans WHERE scope_name = 'wyrd.tests.otel.trace' ORDER BY name`,
    [],
    SpanRow,
  );

  const { traceId, spanId } = answer.spanContext();
  expect(rows).toEqual([
    {
      name: "answer",
      trace_id: traceId,
      span_id: spanId,
      parent_span_id: null,
      status_code: 2,
      status_message: "refused",
      gen_ai_request_model: "claude-opus-5",
      gen_ai_usage_input_tokens: 4096n,
    },
    expect.objectContaining({ name: "retrieve", trace_id: traceId, parent_span_id: spanId }),
  ]);
});

test("stock log record reads back through bifrost", async ({ server, bifrost }) => {
  const logs = new LoggerProvider({ processors: [new BatchLogRecordProcessor({ exporter: new OTLPLogExporter() })] });
  const ship = new BasicTracerProvider().getTracer("wyrd.tests.otel.log").startSpan("ship");
  // A record names its context explicitly: without a registered context
  // manager, `context.with` alone would leave it uncorrelated.
  logs.getLogger("wyrd.tests.otel.log").emit({
    body: "order delayed",
    severityNumber: SeverityNumber.WARN,
    severityText: "WARN",
    context: trace.setSpan(context.active(), ship),
  });
  ship.end();
  await logs.shutdown();
  server.flushBifrost();

  const rows = await bifrost.sql(
    `SELECT severity_text, severity_number, trace_id, span_id
       FROM vala.logs.records WHERE scope_name = 'wyrd.tests.otel.log'`,
    [],
    LogRow,
  );

  const { traceId, spanId } = ship.spanContext();
  expect(rows).toEqual([{ severity_text: "WARN", severity_number: 13, trace_id: traceId, span_id: spanId }]);
});

test("stock metric points read back through bifrost", async ({ server, bifrost }) => {
  // Shutdown is the one export: the SDK aggregates cumulatively, so a flush
  // before it would export the same totals twice.
  const metrics = new MeterProvider({
    readers: [new PeriodicExportingMetricReader({ exporter: new OTLPMetricExporter(), exportIntervalMillis: 600_000 })],
  });
  const meter = metrics.getMeter("wyrd.tests.otel.metric", "1.0.0");
  // JavaScript has one number type: a whole count declares `ValueType.INT`
  // or it exports as a double.
  meter.createCounter("orders.created", { valueType: ValueType.INT }).add(7);
  meter.createGauge("queue.depth").record(3.5);
  meter.createHistogram("request.duration", { unit: "ms" }).record(12.5);
  await metrics.shutdown();
  server.flushBifrost();

  const rows = await bifrost.sql(
    `SELECT metric_name, metric_type, int_value, double_value, histogram_count, histogram_sum
       FROM vala.metrics.points WHERE scope_name = 'wyrd.tests.otel.metric' ORDER BY metric_name`,
    [],
    MetricRow,
  );

  const point = { int_value: null, double_value: null, histogram_count: null, histogram_sum: null };
  expect(rows).toEqual([
    { ...point, metric_name: "orders.created", metric_type: "sum", int_value: 7n },
    { ...point, metric_name: "queue.depth", metric_type: "gauge", double_value: 3.5 },
    { ...point, metric_name: "request.duration", metric_type: "histogram", histogram_count: 1n, histogram_sum: 12.5 },
  ]);
});
