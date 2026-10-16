/**
 * The support API, on Node's `http`:
 *
 *   POST /ask {"question": "...", "order_id": "..."}     answer, run the two model-free checks now
 *   POST /reviews {"question": "..."}                    the agent writes a refund decision letter; queue `follows-policy` on it
 *   GET /reviews/<execution_id>                           the review's status and, once done, its breakdown
 *
 * Every question is one Wyrd Run of the Service: the agent's spans, its
 * gateway call, the recorded ticket, and every verdict carry its run ID.
 */

import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { json } from "node:stream/consumers";

import type OpenAI from "openai";
import { type Run, type VerificationResult, verify, WyrdClient, WyrdError, WyrdState } from "wyrd-ai";

import { answer, gateway } from "./agent.ts";
import { BUNDLE_DIR, TICKETS } from "./wyrd.ts";

/** What a customer gets instead of an answer that promises a refund. */
export const HANDOFF = "I've passed your question to our billing team. They reply within one business day.";

/** What the API needs: the hydrated Service, the client it runs as, and the gateway. */
export interface Support {
  readonly state: WyrdState;
  readonly client: WyrdClient;
  readonly gateway: OpenAI;
}

/** The verdict and each requirement's outcome, reason, and policy citations. */
export function breakdown(result: VerificationResult) {
  const requirements = result.outcome.requirements.map(({ id, outcome, reason, citations }) => ({
    id,
    outcome,
    reason,
    citations,
  }));
  return { verdict: result.outcome.verdict, requirements };
}

/** Answer `question` in a new Run of the Service, record the ticket, then `check` the answer within the Run. */
function answerInRun<T>(support: Support, question: string, check: (wyrdRun: Run, text: string) => Promise<T>) {
  // ANCHOR: answer
  const wyrdRun = support.state.run();
  // Spans started inside the scope carry the Run's ID.
  return wyrdRun.scope(async () => {
    const text = await answer(support.gateway, support.state, wyrdRun, question);
    await wyrdRun.observe.record(TICKETS, { question, answer: text });
    return check(wyrdRun, text);
  });
  // ANCHOR_END: answer
}

/**
 * Answer and run the two model-free Verifiers now. `no-refund-promise` is
 * bound to the Service, so it runs on the Run itself; `ticket-complete` is
 * bound to the agent, so it runs on the Run's agent view, which shares the
 * run ID. An answer that promises a refund is never sent.
 */
export function ask(support: Support, question: string, orderId?: string) {
  return answerInRun(support, question, async (wyrdRun, text) => {
    // ANCHOR: check
    const ticket = { question, answer: text, order_id: orderId ?? null };
    const [promise, complete] = await Promise.all([
      wyrdRun.observe.verify("no-refund-promise", { answer: text }),
      wyrdRun.forCard("agent").observe.verify("ticket-complete", { ticket }),
    ]);
    // ANCHOR_END: check
    const checks = { "no-refund-promise": breakdown(promise), "ticket-complete": breakdown(complete) };
    const promised = promise.outcome.verdict !== "passed";
    return { run_id: wyrdRun.runId, answer: promised ? HANDOFF : text, checks };
  });
}

/** What `POST /reviews` asks the agent for: a document, not a chat reply. */
export const LETTER = "Write the customer a refund decision letter in Markdown. Their request: ";

/** Have the agent write a letter and queue `follows-policy` on it; the reply carries the execution ID to read it back by. */
export function review(support: Support, question: string) {
  return answerInRun(support, LETTER + question, async (wyrdRun, text) => {
    // ANCHOR: submit
    const letter = verify.File.bytes(new TextEncoder().encode(text), { filename: "letter.md" });
    const inputs = { letter };
    const submitted = await wyrdRun.forCard("agent").observe.submit("follows-policy", inputs);
    if (submitted.kind !== "queued") throw new Error("follows-policy judges Run evidence, so submit queues it");
    // ANCHOR_END: submit
    return { run_id: wyrdRun.runId, letter: text, execution_id: submitted.execution.id };
  });
}

/** Read a review by its execution ID alone, so any process holding the ID can serve it. */
export async function reviewStatus(support: Support, executionId: string) {
  // ANCHOR: status
  const execution = await verify.Execution.connect(executionId, { client: support.client });
  const { status, error, result } = await execution.status();
  // ANCHOR_END: status
  return { execution_id: executionId, status, error, ...(result ? breakdown(result) : {}) };
}

/** The HTTP server over {@link ask}, {@link review}, and {@link reviewStatus}. */
export function supportApi(support: Support) {
  return createServer(async (request, response) => {
    const reply = (status: number, body: unknown) =>
      response.writeHead(status, { "content-type": "application/json" }).end(JSON.stringify(body));
    const [, route, id] = (request.url ?? "").split("/");
    try {
      if (request.method === "GET" && route === "reviews" && id) {
        return reply(200, await reviewStatus(support, id));
      }
      if (request.method !== "POST" || (route !== "ask" && route !== "reviews") || id) {
        return reply(404, { error: "not found" });
      }
      const body = ((await json(request).catch(() => ({}))) ?? {}) as { question?: unknown; order_id?: unknown };
      const { question, order_id } = body;
      if (typeof question !== "string" || question === "" || (order_id !== undefined && typeof order_id !== "string")) {
        return reply(400, { error: 'send {"question": "...", "order_id": "..."}' });
      }
      return route === "ask"
        ? reply(200, await ask(support, question, order_id))
        : reply(202, await review(support, question));
    } catch (error) {
      if (error instanceof WyrdError) return reply(error.status, { code: error.code, detail: error.detail });
      console.error(error);
      return reply(500, { error: "internal error" });
    }
  });
}

if (import.meta.main) {
  // WYRD_SERVER_URL and WYRD_API_KEY say which server to use and who the app is.
  const client = WyrdClient.connect();
  // The registered Service, hydrated by `npm run deploy`.
  const state = WyrdState.fromPath(BUNDLE_DIR, { client });
  await state.startBifrost(); // writes tickets to Bifrost
  await state.startTelemetry(); // exports spans to Wyrd
  const server = supportApi({ state, client, gateway: gateway(client) });
  server.listen(Number(process.env.PORT ?? 3000), "127.0.0.1", () => {
    const { port } = server.address() as AddressInfo;
    console.log(`support-desk listening on http://127.0.0.1:${port}`);
  });

  // Flush every span and row before exiting.
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, async () => {
      server.close();
      await state.shutdown();
      process.exit(0);
    });
  }
}
