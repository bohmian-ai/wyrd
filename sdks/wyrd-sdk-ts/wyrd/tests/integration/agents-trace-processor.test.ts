import {
  Agent,
  type Model,
  type ModelResponse,
  Usage,
  run,
  setTraceProcessors,
  setTracingDisabled,
  withGenerationSpan,
} from "@openai/agents-core";
import { expect, vi } from "vitest";
import { z } from "zod";

import { WyrdAgentsTraceProcessor } from "wyrd-ai";

import { observedServiceTest as test } from "../support/observed-service.js";

vi.setConfig({ testTimeout: 120_000 });

/** A local Model answering `hi` that records its call in a generation span, as a provider Model does. */
const LOCAL_MODEL: Model = {
  getResponse: () =>
    withGenerationSpan(async (span): Promise<ModelResponse> => {
      span.spanData.model = "local-model";
      span.spanData.usage = { input_tokens: 3, output_tokens: 1 };
      return {
        usage: new Usage({ input_tokens: 3, output_tokens: 1, total_tokens: 4 }),
        output: [
          {
            type: "message",
            role: "assistant",
            status: "completed",
            content: [{ type: "output_text", text: "hi" }],
          },
        ],
      };
    }),
  getStreamedResponse: () => {
    throw new Error("the local model does not stream");
  },
};

const SpanRow = z.object({
  name: z.string(),
  trace_id: z.string(),
  gen_ai_request_model: z.string().nullable(),
  gen_ai_usage_input_tokens: z.bigint().nullable(),
});

test("agents sdk run exports spans attributed to the wyrd run", async ({ server, reader, state }) => {
  await state.startTelemetry();
  setTracingDisabled(false);
  setTraceProcessors([new WyrdAgentsTraceProcessor()]);
  const agentRun = state.run("agent");

  const result = await agentRun.scope(() => run(new Agent({ name: "assistant", model: LOCAL_MODEL }), "hello"));
  await state.shutdown();
  server.flushBifrost();

  expect(result.finalOutput).toBe("hi");
  const rows = await reader.sql(
    `SELECT name, encode(trace_id, 'hex') AS trace_id, gen_ai_request_model, gen_ai_usage_input_tokens
       FROM vala.traces.spans WHERE run_id = $1 AND scope_name = 'wyrd.agents'`,
    [agentRun.runId],
    SpanRow,
  );
  expect(rows).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: "assistant", gen_ai_request_model: null }),
      expect.objectContaining({
        name: "generation",
        gen_ai_request_model: "local-model",
        gen_ai_usage_input_tokens: 3n,
      }),
    ]),
  );
  expect(new Set(rows.map((row) => row.trace_id)).size).toBe(1);
});
