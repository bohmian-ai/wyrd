/**
 * Stock OpenTelemetry OTLP/HTTP protobuf exporters that authenticate as a
 * {@link WyrdClient}.
 *
 * Each factory builds the stock exporter for one signal, pointed at the
 * client's server, whose headers ask the client for its current access token
 * on every export and send it as `x-wyrd-access-token`. The token comes from
 * the client's one shared refresh path, so a long-lived exporter keeps working
 * after any single access token expires; this module holds no token or cache
 * of its own. Needs the optional `@opentelemetry/exporter-*-otlp-proto` peers.
 */

import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-proto";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-proto";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-proto";

import type { WyrdClient } from "./index.js";

/** The signal's OTLP/HTTP URL on `client`'s server and per-export auth headers. */
function config(client: WyrdClient, signal: "traces" | "logs" | "metrics") {
  return {
    url: `${client.serverUrl.replace(/\/+$/, "")}/v1/${signal}`,
    headers: async () => ({ "x-wyrd-access-token": `Bearer ${await client.accessToken()}` }),
  };
}

/** Build a span exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function spanExporter(client: WyrdClient): OTLPTraceExporter {
  return new OTLPTraceExporter(config(client, "traces"));
}

/** Build a log exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function logExporter(client: WyrdClient): OTLPLogExporter {
  return new OTLPLogExporter(config(client, "logs"));
}

/** Build a metric exporter that sends OTLP/HTTP protobuf to `client`'s server. */
export function metricExporter(client: WyrdClient): OTLPMetricExporter {
  return new OTLPMetricExporter(config(client, "metrics"));
}
