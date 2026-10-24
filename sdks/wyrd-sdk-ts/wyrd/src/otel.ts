/**
 * Stock OpenTelemetry OTLP/HTTP protobuf exporters that authenticate as a
 * {@link WyrdClient}, and the Run-correlation span processor for
 * application-owned tracer providers.
 *
 * Each factory builds the stock exporter for one signal, pointed at the
 * client's server, whose headers ask the client for its current access token
 * on every export and send it as `x-wyrd-access-token`, beside the client's
 * workload root, when it has one, as `x-wyrd-workload-uid`. The token comes from
 * the client's one shared refresh path, so a long-lived exporter keeps working
 * after any single access token expires; this module holds no token or cache
 * of its own. Needs the optional `@opentelemetry/exporter-*-otlp-proto` peers.
 */

import { type Context, context, createContextKey, propagation, trace } from "@opentelemetry/api";
import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import { W3CTraceContextPropagator } from "@opentelemetry/core";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-proto";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-proto";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-proto";
import { BasicTracerProvider, BatchSpanProcessor, type Span, type SpanProcessor } from "@opentelemetry/sdk-trace-base";

import type { WyrdClient } from "./index.js";

/**
 * The workload option every exporter factory takes: `workloadUid`, the exact
 * Service Card UID exports run as; omitted, the client's own root, if
 * any. A `workloadUid` that is not a UUIDv7 throws `WYRD_SPEC_400_VALIDATION`.
 */
export type ExporterOptions = { readonly workloadUid?: string };

/** The signal's OTLP/HTTP URL on `client`'s server and per-export auth headers. */
function config(client: WyrdClient, signal: "traces" | "logs" | "metrics", options: ExporterOptions) {
  const rooted = options.workloadUid === undefined ? client : client.forWorkload(options.workloadUid);
  const workloadUid = rooted.workloadUid;
  const environment = rooted.deploymentEnvironment;
  return {
    url: `${rooted.serverUrl.replace(/\/+$/, "")}/v1/${signal}`,
    headers: async (): Promise<Record<string, string>> => ({
      "x-wyrd-access-token": `Bearer ${await rooted.accessToken()}`,
      ...(workloadUid === undefined ? {} : { "x-wyrd-workload-uid": workloadUid }),
      ...(environment === undefined ? {} : { "x-wyrd-deployment-environment": environment }),
    }),
  };
}

/** Build a span exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function spanExporter(client: WyrdClient, options: ExporterOptions = {}): OTLPTraceExporter {
  return new OTLPTraceExporter(config(client, "traces", options));
}

/** Build a log exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function logExporter(client: WyrdClient, options: ExporterOptions = {}): OTLPLogExporter {
  return new OTLPLogExporter(config(client, "logs", options));
}

/** Build a metric exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function metricExporter(client: WyrdClient, options: ExporterOptions = {}): OTLPMetricExporter {
  return new OTLPMetricExporter(config(client, "metrics", options));
}

/**
 * The context key `Run.scope` sets to `[cardUid, runId]`. `createContextKey`
 * is `Symbol.for`, so the key `Run.scope` creates without importing this
 * module is the same one.
 */
const RUN_SCOPE = createContextKey("wyrd.run_scope");

/** Copies the innermost `Run.scope` onto every span started inside it. */
const runCorrelation: SpanProcessor = {
  onStart(span: Span, parentContext: Context): void {
    const scope = parentContext.getValue(RUN_SCOPE) as readonly [string, string] | undefined;
    if (scope !== undefined) {
      span.setAttributes({ "wyrd.card_uid": scope[0], "wyrd.run_id": scope[1] });
    }
  },
  onEnd(): void {},
  forceFlush: async () => {},
  shutdown: async () => {},
};

/**
 * The span processor that stamps `wyrd.card_uid` and `wyrd.run_id` from the
 * innermost {@link Run.scope} onto every span started inside it.
 *
 * Put it once in an application-owned provider's construction-time
 * `spanProcessors`, beside {@link spanExporter} or any other exporter; the
 * provider keeps its own sampler, resource, and lifecycle. It holds no state
 * and creates no provider or exporter; `WyrdState.startTelemetry` installs the
 * same processor on its managed provider.
 *
 * ```ts
 * const provider = new NodeTracerProvider({
 *   spanProcessors: [runCorrelationProcessor(), new BatchSpanProcessor(spanExporter(client))],
 * });
 * ```
 */
export function runCorrelationProcessor(): SpanProcessor {
  return runCorrelation;
}

/**
 * @internal `WyrdState.startTelemetry`: install the global tracer provider
 * exporting through {@link spanExporter} and stamping `Run.scope`, Node's
 * `AsyncLocalStorage` context manager, and the W3C trace-context propagator,
 * unless the application registered a tracer provider.
 *
 * @returns The installed provider, or `undefined` when a global tracer
 * provider is already registered.
 */
export function startTelemetry(client: WyrdClient): BasicTracerProvider | undefined {
  const provider = new BasicTracerProvider({
    spanProcessors: [runCorrelation, new BatchSpanProcessor(spanExporter(client))],
  });
  if (!trace.setGlobalTracerProvider(provider)) {
    // Refused: release this provider's processors and leave the registered one untouched.
    void provider.shutdown().catch(() => {});
    return undefined;
  }
  context.setGlobalContextManager(new AsyncLocalStorageContextManager().enable());
  propagation.setGlobalPropagator(new W3CTraceContextPropagator());
  return provider;
}
