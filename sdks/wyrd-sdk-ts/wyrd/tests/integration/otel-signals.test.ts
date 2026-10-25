// TypeScript's own OTel signals beyond the shared `otel_export` story: stock
// log and metric exporters, plus the promotion and attribution story Python
// mirrors.
import { Metadata } from "@grpc/grpc-js";
import { SeverityNumber } from "@opentelemetry/api-logs";
import { ValueType, context, trace } from "@opentelemetry/api";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-grpc";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-grpc";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BasicTracerProvider, BatchSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Cards } from "wyrd-ai";

import { declared, fixture, runtimeAccount, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 30_000 });

/** A stored fixed-width trace or span identity, read as lower-case hex. */
const HexId = z.instanceof(Uint8Array).transform((bytes) => Buffer.from(bytes).toString("hex"));

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

/** The shared dimensions and workload every signal row carries. */
const DimensionRow = z.object({
  service_name: z.string().nullable(),
  service_version: z.string().nullable(),
  deployment_environment: z.string().nullable(),
  subject_kind: z.string().nullable(),
  subject_name: z.string().nullable(),
  subject_version: z.string().nullable(),
  workload_card_uid: z.string().nullable(),
});

/** The exception columns a span or log record promotes. */
const ExceptionRow = z.object({
  exception_type: z.string().nullable(),
  exception_message: z.string().nullable(),
  is_exception: z.boolean(),
});

/** The URL and exception columns a span promotes from its attributes and events. */
const SpanPromotionRow = ExceptionRow.extend({
  http_request_method: z.string().nullable(),
  url_full: z.string().nullable(),
  url_path: z.string().nullable(),
  url_query: z.string().nullable(),
});

const DIMENSIONS =
  "service_name, service_version, deployment_environment, subject_kind, subject_name, subject_version, workload_card_uid";

/** The application's own standard resource, with no Wyrd state behind it. */
const OWNED = resourceFromAttributes({
  "service.name": "checkout",
  "service.version": "2.1.0",
  "deployment.environment.name": "staging",
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
       FROM bifrost.logs.records WHERE scope_name = 'wyrd.tests.otel.log'`,
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
       FROM bifrost.metrics.points WHERE scope_name = 'wyrd.tests.otel.metric' ORDER BY metric_name`,
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

test("signal promotions and attribution read back through bifrost", async ({ server, bifrost }) => {
  const cards = await Cards.connect();
  const model = declared(await cards.declareFromPath(fixture("cards/observe_a_run/observed-model.yaml")));
  const root = declared(await cards.declareFromPath(fixture("cards/observe_a_run/observed-service.yaml")));
  const { key } = await runtimeAccount({ kind: "Service", name: "observed-service" });
  const subject = { "wyrd.card_uid": model.uid };

  const spans = new BasicTracerProvider({
    resource: OWNED,
    spanProcessors: [new BatchSpanProcessor(new OTLPTraceExporter())],
  });
  const checkout = spans.getTracer("wyrd.tests.otel.owned").startSpan("checkout", {
    attributes: {
      ...subject,
      "http.request.method": "GET",
      "url.full": "https://shop.test/cart?id=7",
      "url.path": "/cart",
      "url.query": "id=7",
    },
  });
  checkout.recordException(new TypeError("sold out"));
  checkout.end();
  const logs = new LoggerProvider({
    resource: OWNED,
    processors: [new BatchLogRecordProcessor({ exporter: new OTLPLogExporter() })],
  });
  logs.getLogger("wyrd.tests.otel.owned").emit({
    body: "checkout failed",
    severityNumber: SeverityNumber.ERROR,
    attributes: { ...subject, "exception.type": "TypeError", "exception.message": "sold out" },
  });
  const metrics = new MeterProvider({
    resource: OWNED,
    readers: [new PeriodicExportingMetricReader({ exporter: new OTLPMetricExporter(), exportIntervalMillis: 600_000 })],
  });
  metrics.getMeter("wyrd.tests.otel.owned").createCounter("orders.failed").add(1, subject);
  const metadata = new Metadata();
  metadata.set("x-wyrd-api-key", key);
  metadata.set("x-wyrd-workload-uid", root.uid);
  const rooted = new BasicTracerProvider({
    resource: OWNED,
    spanProcessors: [new BatchSpanProcessor(new OTLPTraceExporter({ metadata }))],
  });
  rooted.getTracer("wyrd.tests.otel.rooted").startSpan("checkout", { attributes: subject }).end();
  await Promise.all([spans, logs, metrics, rooted].map((provider) => provider.shutdown()));
  server.flushBifrost();

  const observed = {
    service_name: "checkout",
    service_version: "2.1.0",
    deployment_environment: "staging",
    subject_kind: "Model",
    subject_name: "observed-model",
    subject_version: "1.0.0",
    workload_card_uid: null,
  };
  for (const table of ["bifrost.traces.spans", "bifrost.logs.records", "bifrost.metrics.points"]) {
    const rows = await bifrost.sql(
      `SELECT ${DIMENSIONS} FROM ${table} WHERE scope_name = 'wyrd.tests.otel.owned'`,
      [],
      DimensionRow,
    );
    expect(rows, table).toEqual([observed]);
  }
  expect(
    await bifrost.sql(
      `SELECT ${DIMENSIONS} FROM bifrost.traces.spans WHERE scope_name = 'wyrd.tests.otel.rooted'`,
      [],
      DimensionRow,
    ),
  ).toEqual([{ ...observed, service_name: "observed-service", service_version: "1.0.0", workload_card_uid: root.uid }]);
  expect(
    await bifrost.sql(
      `SELECT http_request_method, url_full, url_path, url_query, exception_type, exception_message, is_exception
         FROM bifrost.traces.spans WHERE scope_name = 'wyrd.tests.otel.owned'`,
      [],
      SpanPromotionRow,
    ),
  ).toEqual([
    {
      http_request_method: "GET",
      url_full: "https://shop.test/cart?id=7",
      url_path: "/cart",
      url_query: "id=7",
      exception_type: "TypeError",
      exception_message: "sold out",
      is_exception: true,
    },
  ]);
  expect(
    await bifrost.sql(
      `SELECT exception_type, exception_message, is_exception
         FROM bifrost.logs.records WHERE scope_name = 'wyrd.tests.otel.owned'`,
      [],
      ExceptionRow,
    ),
  ).toEqual([{ exception_type: "TypeError", exception_message: "sold out", is_exception: true }]);
});
