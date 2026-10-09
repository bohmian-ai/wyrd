/**
 * The support desk: an Agent-backed Service that answers customer questions,
 * records each ticket, and is verified continuously and in real time.
 *
 * {@link deploy} registers the checked-in Cards under `service/`, which
 * ensures the declared `vala.datasets.tickets` table, turns on metadata
 * capture, and checks the Agent's model is deployed. {@link serve} answers
 * every question in its own Agent Run. {@link waitForVerdicts} waits for both
 * Verifiers to judge every answer, and {@link explain} joins one Run's
 * evidence over MCP.
 *
 * Run it against a server: `WYRD_SERVER_URL=... WYRD_API_KEY=... pnpm start`.
 */

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import { trace } from "@opentelemetry/api";
import { Bifrost, Cards, Gateway, type RegistrationReceipt, WyrdClient, WyrdState } from "@wyrd/sdk";

/** How many requests {@link serve} answers; every tenth asks for a refund. */
export const REQUESTS = 100;

/** The checked-in Service Cards. */
export const SERVICE = resolve(import.meta.dirname, "../service");

/** The MCP protocol revision Wyrd serves. */
const MCP_VERSION = "2026-07-28";

/** The deployed support desk: its registration and loaded Card graph. */
export interface Desk {
  readonly receipt: RegistrationReceipt;
  readonly state: WyrdState;
}

/** One answered request. */
export interface Served {
  readonly runId: string;
  /** Whether `no-refund-promise` passed the answer in real time. */
  readonly passed: boolean;
}

/** Verdict counts per Verifier name: `[passed, failed]`. */
export type Verdicts = Record<string, [number, number]>;

/** The registered UID of the Card named `name`. */
export function uid(desk: Desk, name: string): string {
  const ref = desk.receipt.outcomes.find((outcome) => outcome.card_ref.name === name)?.card_ref;
  if (!ref?.uid) {
    throw new Error(`${name} is not registered`);
  }
  return ref.uid;
}

/**
 * Register and hydrate the Service into `bundle`, capture gateway call
 * metadata, and check the Agent's model is deployed.
 *
 * @throws WyrdError - the registration refusal, including
 *   `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for an incompatible existing
 *   `tickets` table.
 * @throws Error - naming the model when no deployment serves it.
 */
export async function deploy(client: WyrdClient, bundle: string): Promise<Desk> {
  const cards = Cards.connect({ client });
  const receipt = await cards.registerFromPath(join(SERVICE, "support-desk.yaml"));
  await cards.hydrate(receipt.root, bundle);
  const state = WyrdState.fromPath(bundle, { client });
  const gateway = Gateway.connect({ client });
  await gateway.putCapturePolicy({ mode: "metadata", payload_fields: [] });
  const prompt = state.card("prompt");
  if (prompt.kind !== "Prompt") {
    throw new Error("the `prompt` component is not a Prompt");
  }
  const model = prompt.spec.model;
  if (!(await gateway.deployments()).some((deployment) => deployment.model.model === model)) {
    throw new Error(`the support Agent's model \`${model}\` is not deployed`);
  }
  return { receipt, state };
}

/** The customer question of request `index`. */
export function question(index: number): string {
  return index % 10 === 0 ? `Can I get a refund for order ${index}?` : `Where is order ${index}?`;
}

/**
 * Answer {@link REQUESTS} questions, each in its own Agent Run inside a
 * `support-desk.request` span: invoke the Agent through the gateway, record
 * the ticket, observe the answer for `answer-quality`, and judge it with
 * `no-refund-promise`. Telemetry and observations are flushed on return.
 */
export async function serve(desk: Desk): Promise<Served[]> {
  const { state } = desk;
  await state.startBifrost();
  await state.startTelemetry();
  const tracer = trace.getTracer("support-desk");
  const served: Served[] = [];
  for (let index = 0; index < REQUESTS; index++) {
    const run = state.run("agent");
    const asked = question(index);
    const passed = await run.scope(() =>
      tracer.startActiveSpan("support-desk.request", async (span) => {
        try {
          const answer = await run.invoke({ question: asked });
          const { observe } = run;
          await observe.record("vala.datasets.tickets", {
            ticket_id: `T-${index}`,
            question: asked,
            answer,
            refund: asked.includes("refund"),
          });
          observe.eval({ answer });
          return (await observe.verify("no-refund-promise", { answer })).passed;
        } finally {
          span.end();
        }
      }),
    );
    served.push({ runId: run.runId, passed });
  }
  await state.shutdown();
  return served;
}

/**
 * Wait until both Verifiers have judged every answer, then return their
 * verdict counts.
 *
 * @throws Error - when the verdicts are not all in within `timeoutMs`.
 */
export async function waitForVerdicts(client: WyrdClient, desk: Desk, timeoutMs: number): Promise<Verdicts> {
  const bifrost = await Bifrost.connect({ client });
  const names = new Map(["answer-quality", "no-refund-promise"].map((name) => [uid(desk, name), name]));
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const rows = await bifrost.sql(
      "SELECT card_uid, verdict, COUNT(*) AS n FROM vala.verification.results " +
        "WHERE subject_card_uid = $1 GROUP BY card_uid, verdict",
      [uid(desk, "support-agent")],
      { parse: (row) => row as { card_uid: string; verdict: string; n: number | bigint } },
    );
    const verdicts: Verdicts = {};
    for (const row of rows) {
      const name = names.get(row.card_uid);
      if (name) {
        const counts = (verdicts[name] ??= [0, 0]);
        counts[row.verdict === "passed" ? 0 : 1] += Number(row.n);
      }
    }
    if ([...names.values()].every((name) => (verdicts[name]?.[0] ?? 0) + (verdicts[name]?.[1] ?? 0) >= REQUESTS)) {
      return verdicts;
    }
    if (Date.now() >= deadline) {
      throw new Error(`verdicts still incomplete after ${timeoutMs} ms: ${JSON.stringify(verdicts)}`);
    }
    await sleep(1_000);
  }
}

/**
 * Join every piece of evidence of Run `runId` — its observation, ticket,
 * request span, gateway call, and both verdicts — through MCP
 * `bifrost.query` with a fresh access token, as column name to value.
 *
 * @throws Error - for a malformed Run ID, when MCP or the tool refuses the
 *   call, or when the Run has no complete joined row.
 */
export async function explain(client: WyrdClient, runId: string): Promise<Record<string, unknown>> {
  if (!/^[0-9a-fA-F-]+$/.test(runId)) {
    throw new Error(`malformed Run ID ${runId}`);
  }
  const sql =
    "SELECT o.run_id, o.trace_id, t.ticket_id, t.answer, s.name AS span, " +
    "g.call_id, g.card_uid AS call_card_uid, c.verdict AS continuous, r.verdict AS realtime " +
    "FROM vala.eval.observations o " +
    "JOIN vala.datasets.tickets t ON t.run_id = o.run_id " +
    "JOIN vala.traces.spans s ON s.trace_id = o.trace_id AND s.run_id = o.run_id " +
    "JOIN vala.gateway.calls g ON g.run_id = o.run_id AND g.card_uid = o.card_uid " +
    "JOIN vala.verification.results c ON c.run_id = o.run_id AND c.binding_id IS NOT NULL " +
    "JOIN vala.verification.results r ON r.run_id = o.run_id AND r.binding_id IS NULL " +
    `WHERE o.run_id = '${runId}' AND s.name = 'support-desk.request'`;
  const response = await fetch(`${client.serverUrl}/mcp`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      "x-wyrd-access-token": `Bearer ${await client.accessToken()}`,
      "mcp-protocol-version": MCP_VERSION,
      "mcp-method": "tools/call",
      "mcp-name": "bifrost.query",
    },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: {
        name: "bifrost.query",
        arguments: { sql },
        _meta: {
          "io.modelcontextprotocol/protocolVersion": MCP_VERSION,
          "io.modelcontextprotocol/clientInfo": { name: "support-desk", version: "1.0.0" },
          "io.modelcontextprotocol/clientCapabilities": {},
        },
      },
    }),
  });
  const reply = await response.text();
  if (!response.ok) {
    throw new Error(`MCP refused the call with ${response.status}: ${reply}`);
  }
  const data = reply.split("\n").find((line) => line.startsWith("data:"));
  if (data === undefined) {
    throw new Error("the MCP reply carries no message");
  }
  const { result } = JSON.parse(data.slice("data:".length));
  const content = result.structuredContent as { columns: { name: string }[]; rows: unknown[][] };
  if (result.isError) {
    throw new Error(`bifrost.query failed: ${JSON.stringify(content)}`);
  }
  const [row] = content.rows;
  if (row === undefined) {
    throw new Error(`Run ${runId} has no joined evidence`);
  }
  return Object.fromEntries(content.columns.map((column, index) => [column.name, row[index]]));
}

/** Deploy, serve, and print the verdicts and one passing and one failing explanation. */
async function main(): Promise<void> {
  const client = WyrdClient.connect();
  const desk = await deploy(client, join(mkdtempSync(join(tmpdir(), "support-desk-")), "bundle"));
  const served = await serve(desk);
  console.log(await waitForVerdicts(client, desk, 300_000));
  for (const passed of [true, false]) {
    const request = served.find((candidate) => candidate.passed === passed);
    if (request) {
      console.log(await explain(client, request.runId));
    }
  }
}

if (import.meta.main) {
  await main();
}
