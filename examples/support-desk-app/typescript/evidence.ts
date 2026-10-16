/**
 * Read one Run's evidence back through Bifrost. Every row a Run produced
 * carries its `run_id`, so one join links the ticket to the agent's
 * span, its gateway call, and each verdict.
 *
 *   npm run evidence -- <run_id>
 */

import { Bifrost, type WyrdClient } from "wyrd-ai";
import { z } from "zod";

import { TICKETS } from "./wyrd.ts";

const Evidence = z.object({
  run_id: z.string(),
  question: z.string(),
  answer: z.string(),
  span: z.string(),
  trace_id: z.string(),
  call_id: z.string(),
  verdict: z.string(),
  execution_id: z.string(),
});

/** The joined evidence of Run `runId`, read as `client` (the ambient client by default). */
export async function evidence(runId: string, client?: WyrdClient) {
  const bifrost = await Bifrost.connect({ client });
  try {
    return await bifrost.sql(
      `SELECT c.run_id, c.question, c.answer, s.name AS span, encode(s.trace_id, 'hex') AS trace_id, g.call_id, r.verdict, r.execution_id
         FROM ${TICKETS} c
         JOIN vala.traces.spans s ON s.run_id = c.run_id AND s.name LIKE 'invoke_agent %'
         JOIN vala.gateway.calls g ON g.run_id = c.run_id
         JOIN vala.verification.results r ON r.run_id = c.run_id
        WHERE c.run_id = $1`,
      [runId],
      Evidence,
    );
  } finally {
    await bifrost.shutdown();
  }
}

if (import.meta.main) {
  const runId = process.argv[2];
  if (!runId) throw new Error("usage: npm run evidence -- <run_id>");
  console.table(await evidence(runId));
}
