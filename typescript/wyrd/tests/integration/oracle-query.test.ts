import {
  Data,
  DataType,
  Field,
  Int as Int_,
  RecordBatch,
  Schema,
  Struct,
  Type,
  makeBuilder,
  makeData,
  tableFromIPC,
} from "apache-arrow";
import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import {
  Bifrost,
  IncompleteQueryStreamError,
  TableConfig,
  WyrdError,
} from "@wyrd/sdk";

describe("Oracle query journey", () => {
  it("uses the public SDK against an in-process Wyrd server", async () => {
    const server = startTestServer();
    try {
      const client = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.token,
        grpcUrl: server.grpcUrl,
      });
      const expected = [11, 22];
      server.seedBifrostRows(server.tableFqn, expected);
      server.waitForBifrostPublication();
      const stream = await client.stream({
        sql: `SELECT * FROM ${server.tableFqn}`,
      });
      const batches = [];
      for await (const batch of stream) {
        batches.push(batch);
      }
      expect(batches.reduce((rows, batch) => rows + batch.numRows, 0)).toBe(2);
      expect(batches[0]?.schema.fields[0]?.name).toBe("value");
      expect(batches[0]?.schema.fields.length).toBeGreaterThan(1);
      const values = batches.flatMap((batch) =>
        Array.from(batch.getChildAt(0)?.toArray() ?? []),
      );
      expect(values).toEqual(expected.map(BigInt));
      expect(stream.terminal).toBeDefined();
      expect(stream.terminal?.outcome).toBe("success");
      expect(stream.terminal?.row_count).toBe(2);
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
  });

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
  });
});

/** Hex-decoded canonical trace and span identifiers the fixture writes. */
const TRACE_ID = Uint8Array.from(
  Buffer.from("c1a0112233445566778899aabbccddee", "hex"),
);
const PARENT_SPAN_ID = Uint8Array.from(Buffer.from("a1a2a3a4a5a6a7a8", "hex"));
const CHILD_SPAN_ID = Uint8Array.from(Buffer.from("b1b2b3b4b5b6b7b8", "hex"));
const MODEL = "claude-opus-5";
const INPUT_TOKENS = 1280n;
const OUTPUT_TOKENS = 320n;
const INPUT_MESSAGES =
  '[{"role":"user","parts":[{"type":"text","content":"summarize the incident"}]}]';
const OUTPUT_MESSAGES =
  '[{"role":"assistant","parts":[{"type":"text","content":"the writer stalled"}]}]';
const LOG_BODY = "tool call exhausted its retry budget";
const EVENT_NAME = "gen_ai.choice";
const LINK_TRACE_STATE = "wyrd=fixture";
const LINKED_SPAN_ID = Uint8Array.from(Buffer.from("c1c2c3c4c5c6c7c8", "hex"));
const COUNTER_VALUE = 7n;
const GAUGE_VALUE = 0.75;
const HISTOGRAM_COUNT = 4n;
const HISTOGRAM_SUM = 12.5;
const SERVICE = "wyrd.fixture.service";

/** Concatenate protobuf fragments into one message body. */
function concat(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

/** Encode one base-128 varint, the protobuf tag and length primitive. */
function varint(value: number): Uint8Array {
  const bytes: number[] = [];
  let rest = value;
  do {
    const byte = rest & 0x7f;
    rest >>>= 7;
    bytes.push(rest > 0 ? byte | 0x80 : byte);
  } while (rest > 0);
  return Uint8Array.from(bytes);
}

/** Encode one length-delimited protobuf field. */
function delimited(number: number, payload: Uint8Array): Uint8Array {
  return concat(varint((number << 3) | 2), varint(payload.length), payload);
}

/** Encode one string `AnyValue`, which a log body carries verbatim. */
function anyValue(text: string): Uint8Array {
  return delimited(1, new TextEncoder().encode(text));
}

/**
 * Encode string attributes as the canonical `KeyValueList` bytes.
 *
 * Ingress decodes and re-encodes every canonical binary payload, so a fixture
 * cannot substitute a JSON blob here.
 */
function attributes(pairs: Record<string, string>): Uint8Array {
  return concat(
    ...Object.entries(pairs).map(([key, value]) =>
      delimited(
        1,
        concat(
          delimited(1, new TextEncoder().encode(key)),
          delimited(2, anyValue(value)),
        ),
      ),
    ),
  );
}

/** The value an unnamed column takes, decided by its described type. */
function defaultFor(field: Field): unknown {
  if (field.nullable) {
    return null;
  }
  switch (field.typeId) {
    case Type.Utf8:
      return "";
    case Type.Bool:
      return false;
    case Type.Float:
      return 0;
    case Type.Int:
      return (field.type as Int_).bitWidth === 64 ? 0n : 0;
    case Type.Binary:
    case Type.FixedSizeBinary:
      return new Uint8Array(0);
    case Type.List:
      return [];
    default:
      throw new Error(`column ${field.name} has unsupported type ${field.type}`);
  }
}

/**
 * Build one described column across every fixture row.
 *
 * A nested struct is the one shape a builder cannot express here: the ledger
 * declares its children non-nullable, so an absent struct is a null parent
 * over present children rather than nulls all the way down.
 */
function columnData(field: Field, rows: Row[]): Data {
  if (DataType.isStruct(field.type)) {
    return makeData({
      type: field.type,
      length: rows.length,
      nullCount: rows.length,
      nullBitmap: new Uint8Array(Math.ceil(rows.length / 8) + 8),
      children: field.type.children.map((child) =>
        columnData(child, rows.map(() => ({}))),
      ),
    });
  }
  const builder = makeBuilder({ type: field.type, nullValues: [null] });
  for (const row of rows) {
    builder.append(field.name in row ? row[field.name] : defaultFor(field));
  }
  builder.finish();
  return builder.flush();
}

/** A fixture row naming only the columns its assertions depend on. */
type Row = Record<string, unknown>;

/**
 * Build one Arrow batch over `schema` from rows that name only some columns.
 *
 * The schema is the server's own published description, so the fixture never
 * restates a second copy of the canonical ledger.
 */
function batch(schema: Schema, rows: Row[]): RecordBatch {
  return new RecordBatch(
    schema,
    makeData({
      type: new Struct(schema.fields),
      length: rows.length,
      children: schema.fields.map((field) => columnData(field, rows)),
    }),
  );
}

/** The parent GenAI chat span and the failing tool span it made. */
function spans(schema: Schema, scope: string, anchor: bigint): RecordBatch {
  const envelope: Row = {
    resource_present: true,
    resource_attributes: attributes({ "service.name": SERVICE }),
    scope_present: true,
    scope_name: scope,
    scope_version: "1.0.0",
    service_name: SERVICE,
    gen_ai_provider_name: "anthropic",
    gen_ai_request_model: MODEL,
    gen_ai_conversation_id: "conversation-fixture",
    trace_id: TRACE_ID,
    status_present: true,
  };
  return batch(schema, [
    {
      ...envelope,
      span_id: PARENT_SPAN_ID,
      name: "chat claude-opus-5",
      kind: 3,
      start_time_unix_nano: anchor,
      end_time_unix_nano: anchor + 2_000_000n,
      duration_nano: 2_000_000n,
      status_code: 1,
      status_message: "ok",
      attributes: attributes({
        "gen_ai.input.messages": INPUT_MESSAGES,
        "gen_ai.output.messages": OUTPUT_MESSAGES,
      }),
      gen_ai_operation_name: "chat",
      gen_ai_usage_input_tokens: INPUT_TOKENS,
      gen_ai_usage_output_tokens: OUTPUT_TOKENS,
      events: [
        {
          time_unix_nano: anchor + 1_000_000n,
          name: EVENT_NAME,
          attributes: attributes({ "gen_ai.finish_reason": "stop" }),
          dropped_attributes_count: 0n,
        },
      ],
      links: [
        {
          trace_id: TRACE_ID,
          span_id: LINKED_SPAN_ID,
          trace_state: LINK_TRACE_STATE,
          flags: 1n,
          attributes: attributes({ "link.kind": "follows_from" }),
          dropped_attributes_count: 0n,
        },
      ],
    },
    {
      ...envelope,
      span_id: CHILD_SPAN_ID,
      parent_span_id: PARENT_SPAN_ID,
      name: "execute_tool search",
      kind: 1,
      start_time_unix_nano: anchor + 100_000n,
      end_time_unix_nano: anchor + 900_000n,
      duration_nano: 800_000n,
      status_code: 2,
      status_message: LOG_BODY,
      attributes: attributes({ "gen_ai.tool.name": "search" }),
      gen_ai_operation_name: "execute_tool",
      gen_ai_usage_input_tokens: 64n,
      gen_ai_usage_output_tokens: 16n,
    },
  ]);
}

/** The error log correlated to the tool span that failed. */
function logs(schema: Schema, scope: string, anchor: bigint): RecordBatch {
  return batch(schema, [
    {
      time_unix_nano: anchor + 800_000n,
      observed_time_unix_nano: anchor + 850_000n,
      severity_number: 17,
      severity_text: "ERROR",
      event_name: "tool.retry.exhausted",
      body: anyValue(LOG_BODY),
      trace_id: TRACE_ID,
      span_id: CHILD_SPAN_ID,
      attributes: attributes({ "gen_ai.tool.name": "search" }),
      resource_present: true,
      resource_attributes: attributes({ "service.name": SERVICE }),
      scope_present: true,
      scope_name: scope,
      scope_version: "1.0.0",
    },
  ]);
}

/** One counter, one gauge and one histogram point. */
function points(schema: Schema, scope: string, anchor: bigint): RecordBatch {
  const common = (name: string, kind: string): Row => ({
    metric_name: name,
    description: `fixture ${kind}`,
    unit: "1",
    metric_type: kind,
    time_unix_nano: anchor,
    start_time_unix_nano: anchor,
    attributes: attributes({ "gen_ai.request.model": MODEL }),
    resource_present: true,
    resource_attributes: attributes({ "service.name": SERVICE }),
    scope_present: true,
    scope_name: scope,
    scope_version: "1.0.0",
  });
  return batch(schema, [
    {
      ...common("wyrd.fixture.requests", "sum"),
      int_value: COUNTER_VALUE,
      aggregation_temporality: 2,
      is_monotonic: true,
    },
    { ...common("wyrd.fixture.saturation", "gauge"), double_value: GAUGE_VALUE },
    {
      ...common("wyrd.fixture.latency", "histogram"),
      aggregation_temporality: 2,
      histogram_count: HISTOGRAM_COUNT,
      histogram_sum: HISTOGRAM_SUM,
      histogram_min: 1.0,
      histogram_max: 6.0,
    },
  ]);
}

describe("Canonical signal journey", () => {
  it("canonical signal Arrow write and SQL read round-trip", async () => {
    const server = startTestServer();
    try {
      for (const [namespace, name] of [
        ["traces", "spans"],
        ["logs", "records"],
        ["metrics", "points"],
      ] as const) {
        server.ensureBuiltinTable(namespace, name);
      }
      const transport = {
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      };
      const writer = await Bifrost.connect(transport);
      const scope = `wyrd.ts.canonical.${Date.now()}`;
      const anchor = 1_760_000_000_000_000_000n;

      for (const [fqn, build] of [
        ["vala.traces.spans", spans],
        ["vala.logs.records", logs],
        ["vala.metrics.points", points],
      ] as const) {
        const described = await TableConfig.describe(fqn, transport);
        await writer.writeBatch(fqn, build(described.arrowSchema, scope, anchor));
      }
      server.flushBifrost();

      const hierarchy = (
        await writer.sql(
          `SELECT name, gen_ai_operation_name, status_code, ` +
            `CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) AS is_root ` +
            `FROM vala.traces.spans WHERE scope_name = '${scope}' ` +
            `ORDER BY start_time_unix_nano`,
        )
      ).toArrow();
      expect(
        Array.from(hierarchy.getChild("gen_ai_operation_name")?.toArray() ?? []),
      ).toEqual(["chat", "execute_tool"]);
      expect(Array.from(hierarchy.getChild("is_root")?.toArray() ?? [])).toEqual([
        1n,
        0n,
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
        input_tokens: INPUT_TOKENS + 64n,
        output_tokens: OUTPUT_TOKENS + 16n,
        spans: 2n,
      });

      const correlated = (
        await writer.sql(
          `SELECT l.severity_text, l.event_name, s.name AS span_name ` +
            `FROM vala.logs.records l JOIN vala.traces.spans s ` +
            `ON l.trace_id = s.trace_id AND l.span_id = s.span_id ` +
            `WHERE l.scope_name = '${scope}'`,
        )
      ).toArrow();
      expect(correlated.numRows).toBe(1);
      expect(correlated.get(0)?.toJSON()).toEqual({
        severity_text: "ERROR",
        event_name: "tool.retry.exhausted",
        span_name: "execute_tool search",
      });

      const metrics = (
        await writer.sql(
          `SELECT metric_type, ` +
            `CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, ` +
            `CAST(SUM(COALESCE(double_value, 0.0)) AS DOUBLE) AS doubles, ` +
            `CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations ` +
            `FROM vala.metrics.points WHERE scope_name = '${scope}' ` +
            `GROUP BY metric_type ORDER BY metric_type`,
        )
      ).toArrow();
      expect(metrics.toArray().map((row) => row.toJSON())).toEqual([
        { metric_type: "gauge", ints: 0n, doubles: GAUGE_VALUE, observations: 0n },
        {
          metric_type: "histogram",
          ints: 0n,
          doubles: 0,
          observations: HISTOGRAM_COUNT,
        },
        {
          metric_type: "sum",
          ints: COUNTER_VALUE,
          doubles: 0,
          observations: 0n,
        },
      ]);
      expect(HISTOGRAM_SUM).toBe(12.5);

      const payloadReader = await Bifrost.connect({
        ...transport,
        credential: server.scopedApiKey("ts_canonical_reader", [
          "bifrost_query:read",
        ]),
      });
      const nested = (
        await payloadReader.sql(
          `SELECT CAST(array_length(events) AS BIGINT) AS events, ` +
            `CAST(array_length(links) AS BIGINT) AS links, ` +
            `events[1]['name'] AS event_name, links[1]['trace_state'] AS link_state ` +
            `FROM vala.traces.spans ` +
            `WHERE scope_name = '${scope}' AND parent_span_id IS NULL`,
        )
      ).toArrow();
      expect(nested.get(0)?.toJSON()).toEqual({
        events: 1n,
        links: 1n,
        event_name: EVENT_NAME,
        link_state: LINK_TRACE_STATE,
      });

      const messages = (
        await payloadReader.sql(
          `SELECT attributes FROM vala.traces.spans ` +
            `WHERE scope_name = '${scope}' AND parent_span_id IS NULL`,
        )
      ).toArrow();
      expect(messages.numRows).toBe(1);
      const payload = new TextDecoder().decode(
        messages.getChild("attributes")?.get(0) as Uint8Array,
      );
      expect(payload).toContain(INPUT_MESSAGES);
      expect(payload).toContain(OUTPUT_MESSAGES);
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
