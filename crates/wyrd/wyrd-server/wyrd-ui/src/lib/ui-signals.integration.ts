/**
 * Real OTLP/HTTP JSON evidence for the Observe journeys: one exported trace,
 * log record and pair of metrics per call, sent through the server's stock
 * ingest endpoints. Test-only; it runs inside the integration lane.
 */

/** An OTLP/HTTP JSON attribute list. */
const attrs = (values: Record<string, string>) => Object.entries(values).map(([key, value]) => ({ key, value: { stringValue: value } }));
/** Nanoseconds since the epoch of an instant, as OTLP JSON spells them. */
export const nanos = (iso: string, extra = 0n) => String(BigInt(Date.parse(iso)) * 1_000_000n + extra);

/**
 * Export one trace (a root, a child, an erroring grandchild), one log record
 * on the grandchild, and an integer sum and a double gauge, as `apiKey`, over
 * the stock OTLP/HTTP JSON endpoints, then publish them.
 */
export async function seedSignals(server: { baseUrl: string; flushBifrost(): void }, apiKey: string, ids: { trace: string; service: string; subject?: string }) {
  const resource = { attributes: attrs({ 'service.name': ids.service }) };
  const scope = { name: 'wyrd-ui-journey' };
  const span = (spanId: string, parentSpanId: string, name: string, start: string, ms: number, code: number, attributes: Record<string, string> = {}) => ({
    traceId: ids.trace,
    spanId,
    ...(parentSpanId ? { parentSpanId } : {}),
    name,
    kind: 1,
    startTimeUnixNano: nanos(start, 7n),
    endTimeUnixNano: String(BigInt(nanos(start, 7n)) + BigInt(ms) * 1_000_000n),
    status: { code },
    attributes: attrs(attributes)
  });
  const exports = {
    '/v1/traces': {
      resourceSpans: [
        {
          resource,
          scopeSpans: [
            {
              scope,
              spans: [
                span('00f067aa0ba902b7', '', 'invoke_agent support', '2026-10-19T03:00:00Z', 900, 0, { 'gen_ai.operation.name': 'invoke_agent' }),
                span('53995c3f42cd8ad8', '00f067aa0ba902b7', 'chat ranker', '2026-10-19T03:00:00.100Z', 400, 1, { 'gen_ai.input.messages': '[{"role":"user","parts":[{"type":"text","content":"refund?"}]}]' }),
                span('b9c7c989f97918e1', '53995c3f42cd8ad8', 'execute_tool search', '2026-10-19T03:00:00.200Z', 100, 2, ids.subject ? { 'wyrd.card_uid': ids.subject } : {})
              ]
            }
          ]
        }
      ]
    },
    '/v1/logs': {
      resourceLogs: [
        {
          resource,
          scopeLogs: [
            {
              scope,
              logRecords: [
                {
                  timeUnixNano: nanos('2026-10-19T03:00:00.250Z', 11n),
                  observedTimeUnixNano: nanos('2026-10-19T03:00:00.251Z'),
                  severityNumber: 17,
                  severityText: 'ERROR',
                  body: { stringValue: 'search timed out' },
                  traceId: ids.trace,
                  spanId: 'b9c7c989f97918e1'
                }
              ]
            }
          ]
        }
      ]
    },
    '/v1/metrics': {
      resourceMetrics: [
        {
          resource,
          scopeMetrics: [
            {
              scope,
              metrics: [
                { name: 'ui.requests', unit: '1', sum: { aggregationTemporality: 2, isMonotonic: true, dataPoints: [{ asInt: '7', timeUnixNano: nanos('2026-10-19T03:00:00Z') }] } },
                { name: 'ui.load', unit: '1', gauge: { dataPoints: [{ asDouble: 0.5, timeUnixNano: nanos('2026-10-19T03:00:00Z') }] } }
              ]
            }
          ]
        }
      ]
    }
  };
  for (const [path, body] of Object.entries(exports)) await exportOtlp(server, apiKey, path, body);
  server.flushBifrost();
}

/** POST one OTLP/HTTP JSON export as `apiKey`; anything but 200 throws with the server's answer. */
export async function exportOtlp(server: { baseUrl: string }, apiKey: string, path: string, body: unknown, headers: Record<string, string> = {}) {
  const response = await fetch(new URL(path, server.baseUrl), {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-wyrd-api-key': apiKey, ...headers },
    body: JSON.stringify(body)
  });
  if (response.status !== 200) throw new Error(`${path} answered ${response.status}: ${await response.text()}`);
}
