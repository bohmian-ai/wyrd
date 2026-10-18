/**
 * The support desk: an Agent-backed Service that answers customer questions,
 * records each ticket, and checks every answer against its refund policy
 * with the `support/policy-check` Verifier.
 *
 * {@link deploy} registers the checked-in Cards under `service/`, which
 * ensures the declared `vala.datasets.tickets` table, turns on metadata
 * capture, and checks the Agent's model is deployed. {@link serve} answers
 * every question in its own Agent Run with {@link answer}, which calls the
 * model through the Wyrd gateway, and submits each answer for a background
 * policy check. {@link waitForVerdicts} resumes each check from
 * its execution ID alone and waits for its result, {@link check} judges one
 * answer immediately, and {@link explain} joins one Run's evidence over MCP.
 *
 * Run it against a server: `WYRD_SERVER_URL=... WYRD_API_KEY=... pnpm start`.
 */

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { trace } from "@opentelemetry/api";
import {
  Cards,
  Gateway,
  type DeclarationReceipt,
  type Run,
  type VerificationResult,
  type VerifierInputs,
  verify,
  WyrdClient,
  WyrdState,
} from "wyrd-ai";

/** How many requests {@link serve} answers; every tenth asks for a refund. */
export const REQUESTS = 100;

/** The checked-in Service Cards. */
export const SERVICE = resolve(import.meta.dirname, "../service");

/** The MCP protocol revision Wyrd serves. */
const MCP_VERSION = "2026-07-28";

/** The deployed support desk: its registration and loaded Card graph. */
export interface Desk {
  readonly receipt: DeclarationReceipt;
  readonly state: WyrdState;
  /** The client the desk's model calls go through the gateway as. */
  readonly client: WyrdClient;
}

/** One answered request. */
export interface Served {
  readonly runId: string;
  /** The background policy check of the request's answer. */
  readonly executionId: string;
}

/** The `policy-check` inputs for `answer`: the answer text and the checked-in refund policy PDF. */
function policyCheck(answer: string): VerifierInputs {
  return { answer, policy: verify.File.path(join(SERVICE, "refund-policy.pdf")) };
}

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
 * @throws Error - naming the exact gateway deployment to configure when the
 *   Agent's gateway route names a deployment that does not exist.
 */
export async function deploy(client: WyrdClient, bundle: string): Promise<Desk> {
  const cards = await Cards.connect({ client });
  const receipt = await cards.declareFromPath(join(SERVICE, "support-desk.yaml"));
  await cards.hydrate(receipt.root, bundle);
  const state = WyrdState.fromPath(bundle, { client });
  const gateway = await Gateway.connect({ client });
  await gateway.putCapturePolicy({ mode: "metadata", payload_fields: [] });
  const prompt = state.card("prompt");
  if (prompt.kind !== "Prompt") {
    throw new Error("the `prompt` component is not a Prompt");
  }
  const agent = state.card("agent");
  if (agent.kind !== "Agent") {
    throw new Error("the `agent` component is not an Agent");
  }
  const { execution_route: executionRoute } = agent.spec;
  if (executionRoute.kind === "native") {
    return { receipt, state, client };
  }
  const route = `${executionRoute.deployment}/${prompt.spec.model}`;
  const deployed = await gateway.deployments();
  if (!deployed.some(({ name }) => name === executionRoute.deployment)) {
    throw new Error(
      `the support Agent's model ${route} is not deployed; configure a gateway provider deployment ` +
        `for ${route} (PUT /v1/admin/gateway/provider-deployments/{name})`,
    );
  }
  return { receipt, state, client };
}

/** The total budget of one gateway model call, through the last reply byte. */
const ANSWER_TIMEOUT_MS = 10 * 60_000;

/**
 * Answer `asked` as the support Agent of `run` through the Wyrd gateway: send
 * its Prompt, with `{{question}}` bound, to `<deployment>/<model>` on the
 * OpenAI-compatible ingress as `client`, tagged with the Run and Agent Card
 * so the gateway's capture, accounting, and audit join the Run's evidence.
 * `asked` is inserted literally, and the whole exchange, through reading the
 * reply body, is bounded by {@link ANSWER_TIMEOUT_MS}.
 *
 * @throws Error - carrying the gateway's stable code, such as
 *   `WYRD_PERMISSION_403_DENIED_RBAC`, when it refuses the call.
 * @throws DOMException - `TimeoutError` when the budget runs out; the gateway
 *   may still have completed the call, and nothing retries it.
 */
export async function answer(client: WyrdClient, state: WyrdState, run: Run, asked: string): Promise<string> {
  const agent = state.agent("agent");
  const route = agent.spec.execution_route;
  if (route.kind !== "wyrd_gateway") {
    throw new Error("the support Agent must route through the Wyrd gateway");
  }
  const prompt = agent.prompt().spec;
  const reply = await client.httpAuth(`${client.serverUrl}/v1/chat/completions`, {
    method: "POST",
    signal: AbortSignal.timeout(ANSWER_TIMEOUT_MS),
    headers: {
      "content-type": "application/json",
      "wyrd-run-id": run.runId,
      "wyrd-card-uid": state.cardRef("agent").uid ?? "",
    },
    body: JSON.stringify({
      model: `${route.deployment}/${prompt.model}`,
      messages: (prompt.messages ?? []).map((message) => ({
        role: message.role,
        content: String(message.content).replaceAll("{{question}}", () => asked),
      })),
    }),
  });
  const body = (await reply.json()) as {
    error?: { code: string; message: string };
    choices?: { message: { content: string } }[];
  };
  if (!reply.ok) {
    throw new Error(`the gateway refused the call: ${body.error?.code}: ${body.error?.message}`);
  }
  return body.choices?.[0]?.message.content ?? "";
}

/** The customer question of request `index`. */
export function question(index: number): string {
  return index % 10 === 0 ? `Can I get a refund for order ${index}?` : `Where is order ${index}?`;
}

/**
 * Answer {@link REQUESTS} questions, each in its own Agent Run inside a
 * `support-desk.request` span: {@link answer} through the gateway, record
 * the ticket, and submit the answer for a background `policy-check` without
 * waiting for it. Telemetry and tickets are flushed on return.
 */
export async function serve(desk: Desk): Promise<Served[]> {
  const { state, client } = desk;
  await state.startBifrost();
  await state.startTelemetry();
  const tracer = trace.getTracer("support-desk");
  const served: Served[] = [];
  for (let index = 0; index < REQUESTS; index++) {
    const run = state.run("agent");
    const asked = question(index);
    const executionId = await run.scope(() =>
      tracer.startActiveSpan("support-desk.request", async (span) => {
        try {
          const reply = await answer(client, state, run, asked);
          const { observe } = run;
          await observe.record("vala.datasets.tickets", {
            ticket_id: `T-${index}`,
            question: asked,
            answer: reply,
            refund: asked.includes("refund"),
          });
          const submitted = await observe.submit("policy-check", policyCheck(reply));
          if (submitted.kind !== "queued") {
            throw new Error("policy-check judges Run evidence, not a window");
          }
          return submitted.execution.id;
        } finally {
          span.end();
        }
      }),
    );
    served.push({ runId: run.runId, executionId });
  }
  await state.shutdown();
  return served;
}

/** Judge `answer` with `policy-check` now, in a fresh Agent Run, with the verdict and each requirement's outcome. */
export function check(desk: Desk, answer: string): Promise<VerificationResult> {
  return desk.state.run("agent").observe.verify("policy-check", policyCheck(answer));
}

/**
 * Resume every served request's background check from its execution ID
 * alone, as another process would, and wait for its result; results are in
 * `served` order.
 *
 * @throws WyrdError - the execution's own error, or
 *   `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` when the results are not all in
 *   within `timeoutMs`.
 */
export async function waitForVerdicts(
  client: WyrdClient,
  served: readonly Served[],
  timeoutMs: number,
): Promise<VerificationResult[]> {
  const deadline = Date.now() + timeoutMs;
  const results: VerificationResult[] = [];
  for (const request of served) {
    const execution = await verify.Execution.connect(request.executionId, { client });
    results.push(await execution.wait({ timeoutMs: Math.max(0, deadline - Date.now()) }));
  }
  return results;
}

/**
 * Join every piece of evidence of Run `runId` — its ticket, request span,
 * gateway call, and policy-check verdict — through MCP
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
    "SELECT t.run_id, s.trace_id, t.ticket_id, t.answer, s.name AS span, " +
    "g.call_id, g.card_uid AS call_card_uid, r.verdict " +
    "FROM vala.datasets.tickets t " +
    "JOIN vala.traces.spans s ON s.run_id = t.run_id " +
    "JOIN vala.gateway.calls g ON g.run_id = t.run_id " +
    "JOIN vala.verification.results r ON r.run_id = t.run_id " +
    `WHERE t.run_id = '${runId}' AND s.name = 'support-desk.request' ` +
    "ORDER BY g.call_id LIMIT 1";
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

/**
 * Deploy, judge one refund promise immediately, serve every request, wait
 * for every background policy check, and print the requirement breakdown of
 * a failing answer and the joined evidence of one passing and one failing
 * request.
 */
async function main(): Promise<void> {
  const client = WyrdClient.connect();
  const desk = await deploy(client, join(mkdtempSync(join(tmpdir(), "support-desk-")), "bundle"));
  console.log(`deployed support-agent ${uid(desk, "support-agent")}`);
  const promise = await check(desk, "You have a guaranteed refund.");
  console.log(`immediate check: ${promise.outcome.verdict}`);
  const served = await serve(desk);
  const results = await waitForVerdicts(client, served, 300_000);
  const failed = served.find((_, index) => results[index]?.outcome.verdict === "failed");
  if (failed) {
    // Any process can resume a check from its ID with the ambient client.
    const resumed = await (await verify.Execution.connect(failed.executionId)).wait({ timeoutMs: 30_000 });
    for (const requirement of resumed.outcome.requirements) {
      console.log(`${requirement.id}: ${requirement.outcome} (${requirement.reason})`);
    }
  }
  for (const verdict of ["passed", "failed"]) {
    const request = served.find((_, index) => results[index]?.outcome.verdict === verdict);
    if (request) {
      console.log(await explain(client, request.runId));
    }
  }
}

if (import.meta.main) {
  await main();
}
