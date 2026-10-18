import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import {
  Bifrost,
  Cards,
  OperatorConnections,
  Principals,
  type VerifierInputs,
  WyrdClient,
  type WyrdError,
  WyrdState,
  verify,
} from "wyrd-ai";
import { spanExporter } from "wyrd-ai/otel";
import { cli } from "@wyrd/testing";

import {
  AGREES,
  type Assistant,
  assistantAs,
  assistantTest,
  judgeCalls,
  judgeReply,
  type JudgeCall,
  toolCall,
  toolTurn,
} from "../support/assistant.js";
import { type ReceivedRequest, Receiver, type Reply } from "../support/http-receiver.js";
import { ancestry, binding, fixture, groupExecution, registered, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** How long a test waits for a queued execution or a scheduled result. */
const WAIT_MS = 90_000;

/** The trace identity of a caller's W3C `traceparent`. */
const CALLER_TRACE = "0af7651916cd43dd8448eb211c80319c";
/** The caller's span identity within {@link CALLER_TRACE}. */
const CALLER_SPAN = "b7ad6b7169203331";

/** The stable refusal for a caller no grant authorizes. */
const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** The `coding-review` statement every request of its `spans-reviewed` check carries. */
const LIVE_REVIEW = "spans were reviewed live";

/** What a Chat Completions request carrying tool results contains. */
const TOOL_TURN = /"role"\s*:\s*"tool"/;

/** The coding judge's Verdict once it read the spans: passed, citing the first trace ID. */
const REVIEWED =
  '{"outcome":"passed","reason":"the spans were read","citations":[{"slot":"traces","locator":{"json_pointer":"/0"}}]}';

/**
 * The local judge of the coding session: the `spans-reviewed` check first
 * reads the span names of the trace its answer names live through
 * `bifrost.query`, then passes with {@link REVIEWED}; every other judge agrees.
 */
function codingJudge(request: ReceivedRequest): Reply {
  if (!request.body.includes(LIVE_REVIEW)) {
    return judgeReply(AGREES);
  }
  if (TOOL_TURN.test(request.body)) {
    return judgeReply(REVIEWED);
  }
  const [traceId] = request.body.match(/[0-9a-f]{32}/) ?? [];
  const sql = `SELECT name FROM vala.traces.spans WHERE trace_id = decode('${traceId}', 'hex') ORDER BY name`;
  return toolTurn([toolCall("call-spans", "bifrost.query", { sql, max_rows: 10 })]);
}

/** A fresh deployment per test, as each Rust journey starts its own, judged as {@link codingJudge} answers. */
const test = assistantTest({ perTest: true, provider: codingJudge });

/**
 * The process-global tracer provider, installed once by the first traced
 * test. Later tests only need its active span context, which the SDK sends
 * with each request whichever state installed the provider.
 */
let telemetry: Promise<unknown> | undefined;

/** Install `state`'s telemetry unless an earlier test in this file already did. */
function startTelemetry(state: WyrdState): Promise<unknown> {
  telemetry ??= state.startTelemetry();
  return telemetry;
}

/** One verification result beside one judge call joined to it on the subject Card. */
const ResultJudgeCall = z.object({
  result_subject: z.string(),
  result_actor: z.string(),
  result_run: z.string().nullable(),
  call_subject: z.string(),
  call_actor: z.string(),
  call_run: z.string().nullable(),
});

/**
 * Assert the result `executionId` wrote joins exactly one judge call on its
 * verified Card, both for `principal` on `run`: the result is performed by
 * `principal` itself, and the judge call inside it by SYSTEM.
 */
async function assertCalledAsSystem(executionId: string, principal: string, run: string | null): Promise<void> {
  const joined = await (await Bifrost.connect()).sql(
    `SELECT r.sub_principal_id AS result_subject, r.act_principal_id AS result_actor, r.run_id AS result_run,
            c.sub_principal_id AS call_subject, c.act_principal_id AS call_actor, c.run_id AS call_run
       FROM vala.verification.results r
       JOIN vala.gateway.calls c ON c.card_uid = r.card_uid
      WHERE r.execution_id = $1`,
    [executionId],
    ResultJudgeCall,
  );
  expect(joined).toEqual([
    {
      result_subject: principal,
      result_actor: principal,
      result_run: run,
      call_subject: principal,
      call_actor: expect.not.stringMatching(`^${principal}$`),
      call_run: run,
    },
  ]);
}

/**
 * Judge the answer `yes` with `answer-judged` over plain HTTP the way an
 * agent does, as the assistant Service with `traceparent` and no Run, and
 * return the result's `execution_id`.
 */
async function verifyOverHttp(baseUrl: string, assistant: Assistant, traceparent: string): Promise<string> {
  const token = await WyrdClient.connect({ credential: assistant.key }).accessToken();
  const response = await fetch(`${baseUrl}/v1/verification/execute`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-wyrd-access-token": `Bearer ${token}`, traceparent },
    body: JSON.stringify({
      verifier_card_uid: assistant.answerJudged.uid,
      card_uid: assistant.agent.uid,
      inputs: { answer: { kind: "json", value: "yes" } },
    }),
  });
  const result = (await response.json()) as { execution_id: string; outcome: { verdict: string } };
  expect(response.status, JSON.stringify(result)).toBe(200);
  expect(result.outcome.verdict).toBe("passed");
  return result.execution_id;
}

/** The gateway span of `call` in `spans`, joined on its call id. */
function spanOf(call: z.infer<typeof JudgeCall>, spans: Awaited<ReturnType<typeof timeline>>) {
  return spans.find((span) => span.attributes.gateway_call_id === call.call_id);
}

test("agent answer passes its verifier", async ({ state }) => {
  const run = state.run("agent");

  const result = await run.observe.verify("answer-check", { answer: "yes" });

  expect(result.outcome.verdict).toBe("passed");
  expect(result.verifier.name).toBe("answer-check");
  expect(result.subject).toEqual(state.cardRef("agent"));
  expect(result.run_id).toBe(run.runId);
  expect(result.outcome.requirements.map((each) => [each.id, each.outcome])).toEqual([["says-yes", "passed"]]);
});

test("agent answer fails its verifier", async ({ state }) => {
  const result = await state.run("agent").observe.verify("answer-check", { answer: "no" });

  expect(result.outcome.verdict).toBe("failed");
  expect(result.outcome.requirements.map((each) => [each.outcome, each.actual])).toEqual([["failed", "no"]]);
});

test("positional object is the one answer, not its slots", async ({ state }) => {
  const observe = state.run("agent").observe;

  const positional = await observe.verify("answer-check", { answer: "yes" }, { positional: true });
  const bare = await observe.verify("answer-check", "yes");

  expect(positional.outcome.requirements.map((each) => [each.outcome, each.actual])).toEqual([
    ["failed", { answer: "yes" }],
  ]);
  expect(positional.outcome.verdict).toBe("failed");
  expect(bare.outcome.verdict).toBe("passed");
});

test("judged answer passes the agent check", async ({ provider, state }) => {
  const result = await state.run("agent").observe.verify("answer-judged", { answer: "yes" });

  expect(result.outcome.verdict).toBe("passed");
  expect(result.outcome.requirements).toHaveLength(1);
  const [requirement] = result.outcome.requirements;
  expect(requirement?.outcome).toBe("passed");
  expect(requirement?.citations).toEqual([{ slot: "answer", locator: { json_pointer: "" } }]);
  expect(requirement?.provenance).toMatchObject({ kind: "agent", agent: { name: "judge-agent" } });
  const judged = provider.received.filter((request) => request.path === "/v1/chat/completions");
  expect(judged).toHaveLength(1);
  expect(judged[0]?.body).toContain("Claim: The answer agrees with the customer");
});

test("judge call is attributed to the run and card", async ({ server, assistant, state }) => {
  const run = state.run("agent");

  const result = await run.observe.verify("answer-judged", { answer: "yes" });

  const calls = await judgeCalls(server);
  expect(
    calls.every((call) => call.act_principal_id !== assistant.principal),
    JSON.stringify(calls),
  ).toBe(true);
  expect(calls.map((call) => [call.sub_principal_id, call.run_id, call.card_uid])).toEqual([
    [assistant.principal, run.runId, result.subject.uid],
  ]);
  await assertCalledAsSystem(result.execution_id, assistant.principal, run.runId);
});

test("judge call without a run joins the callers trace attributed to the card", async ({ server, assistant }) => {
  const execution = await verifyOverHttp(server.baseUrl, assistant, `00-${CALLER_TRACE}-${CALLER_SPAN}-01`);

  const calls = await judgeCalls(server);
  expect(calls.map((call) => [call.sub_principal_id, call.run_id, call.card_uid])).toEqual([
    [assistant.principal, null, assistant.agent.uid],
  ]);
  const spans = await timeline(server, CALLER_TRACE);
  const span = calls[0] && spanOf(calls[0], spans);
  expect(span, "the judge call joins the caller's trace").toBeDefined();
  expect(span && ancestry(spans, span).at(-1)?.parent_span_id).toBe(CALLER_SPAN);
  await assertCalledAsSystem(execution, assistant.principal, null);
});

test("traced sdk verify joins the callers trace", async ({ server, state }) => {
  await startTelemetry(state);
  const caller = await trace.getTracer("journey").startActiveSpan("caller", async (span) => {
    await state.run("agent").observe.verify("answer-judged", { answer: "yes" });
    span.end();
    return span.spanContext();
  });

  const calls = await judgeCalls(server);
  const spans = await timeline(server, caller.traceId);
  expect(calls).toHaveLength(1);
  expect(calls[0] && spanOf(calls[0], spans), JSON.stringify(calls)).toBeDefined();
});

test("traced sdk submit links its attempt to the callers trace", async ({ server, state }) => {
  await startTelemetry(state);
  const caller = await trace.getTracer("journey").startActiveSpan("caller", async (span) => {
    const submitted = await state.run("agent").observe.submit("answer-judged", { answer: "yes" });
    span.end();
    if (submitted.kind !== "queued") {
      throw new Error(`Run evidence queues an execution: ${JSON.stringify(submitted)}`);
    }
    await submitted.execution.wait({ timeoutMs: WAIT_MS });
    return span.spanContext();
  });

  const calls = await judgeCalls(server);
  const spans = await timeline(server, caller.traceId);
  const attempts = spans.filter((span) => span.name === "verification.attempt");
  expect(attempts, JSON.stringify(spans)).toHaveLength(1);
  const [attempt] = attempts;
  expect(attempt?.trace_id).not.toBe(caller.traceId);
  expect(attempt?.links.map((link) => link.trace_id)).toEqual([caller.traceId]);
  expect(calls).toHaveLength(1);
  const judged = calls[0] && spanOf(calls[0], spans);
  expect(judged, "the judge call is in the caller's timeline").toBeDefined();
  expect(judged && ancestry(spans, judged).at(-1)?.span_id).toBe(attempt?.span_id);
});

test("malformed traceparent is ignored by the judge call", async ({ server, assistant }) => {
  await verifyOverHttp(server.baseUrl, assistant, `00-${CALLER_TRACE}-${CALLER_SPAN}`);

  expect(await judgeCalls(server)).toHaveLength(1);
  expect(await timeline(server, CALLER_TRACE), "nothing joined the caller's trace").toEqual([]);
});

test("submitted answer is judged in the background and resumes by id", async ({ assistant, state }) => {
  const run = state.run("agent");

  const submitted = await run.observe.submit("answer-check", { answer: "no" });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  const waited = await submitted.execution.wait({ timeoutMs: WAIT_MS });
  const resumed = await (
    await verify.Execution.connect(submitted.execution.id, {
      client: WyrdClient.connect({ credential: assistant.key }),
    })
  ).wait({ timeoutMs: WAIT_MS });

  expect(waited.outcome.verdict).toBe("failed");
  expect(waited.execution_id).toBe(submitted.execution.id);
  expect(waited.run_id).toBe(run.runId);
  expect(resumed).toEqual(waited);
});

test("another tenant cannot resume an execution", async ({ server, state }) => {
  const submitted = await state.run("agent").observe.submit("answer-check", { answer: "yes" });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await submitted.execution.wait({ timeoutMs: WAIT_MS });
  const foreign = WyrdClient.connect({
    credential: server.bootstrapServiceInTenant(server.seedTenant("other-tenant"), ["admin"], "other_tenant_admin"),
  });

  const resumed = await verify.Execution.connect(submitted.execution.id, { client: foreign });

  await expect(resumed.wait({ timeoutMs: 2_000 })).rejects.toMatchObject({
    code: "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT",
  });
});

test("window verifier records samples and refuses verify", async ({ state }) => {
  await state.startBifrost();
  const observe = state.run("model").observe;

  await expect(observe.verify("latency-drift", { latency: 12.5 })).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED",
  });
  expect(await observe.submit("latency-drift", { latency: 12.5 })).toEqual({ kind: "recorded" });
});

test("unfitted baseline is skipped on schedule", async ({ server }) => {
  const cards = await Cards.connect();
  await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  await cards.registerFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
  const { root } = await cards.registerFromPath(fixture("cards/verify_in_real_time/unfitted-assistant.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-unfitted-"));
  await cards.hydrate(root, bundle);
  const key = await cli.issueKey({ ...root, space: root.space ?? "default" });
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key.key }) });
  await state.startBifrost();
  const observe = state.run("model").observe;
  for (let tier = 0; tier < 5; tier += 1) {
    await observe.submit("tier-drift", { tier: "gold" });
  }
  await state.shutdown();
  const id = await binding(cards, root);
  server.makeBindingDue(id);
  const token = await WyrdClient.connect().accessToken();
  const status = async (): Promise<{ next_run_at?: string; readiness: string; last_execution_id?: string }> => {
    const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${id}`, {
      headers: { "x-wyrd-access-token": `Bearer ${token}` },
    });
    expect(response.status).toBe(200);
    return (await response.json()) as { next_run_at?: string; readiness: string; last_execution_id?: string };
  };
  const due = (await status()).next_run_at;

  const deadline = Date.now() + WAIT_MS;
  let skipped = await status();
  while (skipped.next_run_at === due) {
    expect(Date.now() < deadline, "the due occurrence was not scheduled").toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 500));
    skipped = await status();
  }

  expect(skipped.readiness).toBe("baseline_not_ready");
  expect(skipped.last_execution_id ?? null).toBeNull();
});

test("unbound verifier fails locally", async ({ state }) => {
  await expect(state.run("agent").observe.verify("not-bound-here", { answer: "yes" })).rejects.toMatchObject({
    code: "WYRD_SDK_404_UNKNOWN_VERIFIER",
  });
});

test("input that does not fit its slot fails locally", async ({ state }) => {
  const observe = state.run("agent").observe;

  for (const inputs of [
    { reply: "yes" },
    { answer: verify.File.bytes(new TextEncoder().encode("yes")) },
  ] as VerifierInputs[]) {
    await expect(observe.verify("answer-check", inputs)).rejects.toMatchObject({
      code: "WYRD_VERIFICATION_400_INPUT_INVALID",
    });
  }
});

test("caller without the verifier grant is refused", async ({ server, assistant }) => {
  const keys = [
    server.scopedApiKey("operators_only", ["operators:invoke"]),
    server.scopedApiKey("other_verifier", [
      { resource: "verifier", action: "run", scope: { verifier: assistant.answerJudged.uid } },
    ]),
  ];
  for (const credential of keys) {
    const run = assistantAs(assistant, WyrdClient.connect({ credential })).run("agent");

    await expect(run.observe.verify("answer-check", { answer: "yes" })).rejects.toMatchObject(DENIED);
  }
});

test("retired verification contracts are refused", async ({ server, assistant }) => {
  for (const action of ["run", "invoke"]) {
    const credential = server.scopedApiKey(`evals_${action}`, [{ resource: "evals", action, scope: "all" }]);
    const run = assistantAs(assistant, WyrdClient.connect({ credential })).run("agent");

    await expect(run.observe.verify("answer-check", { answer: "yes" })).rejects.toMatchObject({
      code: "WYRD_SPEC_500_INTERNAL",
    });
  }

  const bifrost = await Bifrost.connect();
  expect((await bifrost.listTables()).every((table) => table.namespace !== "vala.eval")).toBe(true);
  const refusals = async (namespace: string, name: string) =>
    Promise.all(
      [
        bifrost.sql(`SELECT * FROM ${namespace}.${name}`),
        bifrost.describeTable(namespace, name),
        (async () => (await Bifrost.connect()).useTableByName(`${namespace}.${name}`))(),
      ].map((call) =>
        call.then(
          () => "accepted",
          (error: { code: string }) => error.code,
        ),
      ),
    );
  for (const name of ["result_items", "observations"]) {
    const retired = await refusals("vala.eval", name);
    expect(retired).not.toContain("accepted");
    expect(retired).toEqual(await refusals("vala.absent", name));
  }
  await bifrost.sql("SELECT COUNT(*) AS n FROM vala.verification.results");
});

test("exact verifier grant verifies only that verifier", async ({ server, assistant }) => {
  const credential = server.scopedApiKey("exact_verifier", [
    { resource: "verifier", action: "run", scope: { verifier: assistant.answerCheck.uid } },
  ]);
  const run = assistantAs(assistant, WyrdClient.connect({ credential })).run("agent");

  expect((await run.observe.verify("answer-check", { answer: "yes" })).outcome.verdict).toBe("passed");
  await expect(run.observe.verify("answer-judged", { answer: "yes" })).rejects.toMatchObject(DENIED);
});

test("another tenant cannot verify the assistant", async ({ server, assistant }) => {
  const foreign = WyrdClient.connect({
    credential: server.bootstrapServiceInTenant(server.seedTenant("other-tenant"), ["admin"], "other_tenant_admin"),
  });
  const run = assistantAs(assistant, foreign).run("agent");

  for (const verifier of ["answer-check", "answer-judged"]) {
    await expect(run.observe.verify(verifier, { answer: "yes" })).rejects.toMatchObject({
      code: "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
    });
  }
});

/** Enqueue `body` over plain HTTP as `key` with `idempotencyKey`, returning the accepted execution. */
async function enqueueOverHttp(baseUrl: string, key: string, idempotencyKey: string, body: object): Promise<string> {
  const token = await WyrdClient.connect({ credential: key }).accessToken();
  const response = await fetch(`${baseUrl}/v1/verification/enqueue`, {
    method: "POST",
    headers: {
      "x-wyrd-access-token": `Bearer ${token}`,
      "idempotency-key": idempotencyKey,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  const accepted = (await response.json()) as { execution_id: string };
  expect(response.status, JSON.stringify(accepted)).toBe(202);
  return accepted.execution_id;
}

/**
 * Register and hydrate the Service fixture at `path`, issue the Service's own
 * key and principal, start its state with that key, and find the one binding
 * whose status pins `group_end` settings.
 */
async function grouped(baseUrl: string, path: string) {
  const cards = await Cards.connect();
  const receipt = await cards.registerFromPath(fixture(path));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-grouped-"));
  await cards.hydrate(receipt.root, bundle);
  const issued = await cli.issueKey({ ...receipt.root, space: receipt.root.space ?? "default" });
  const token = await WyrdClient.connect().accessToken();
  const bindingIds: string[] = [];
  for (const id of (await cards.get(receipt.root)).status?.verification?.binding_ids ?? []) {
    const response = await fetch(`${baseUrl}/v1/verification/bindings/${id}`, {
      headers: { "x-wyrd-access-token": `Bearer ${token}` },
    });
    if (((await response.json()) as { group_end?: unknown }).group_end) {
      bindingIds.push(id);
    }
  }
  expect(bindingIds).toHaveLength(1);
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: issued.key }) });
  await state.startBifrost();
  return { cards, receipt, bundle, key: issued.key, principal: issued.principal_id, state, bindingId: bindingIds[0] };
}

test("grouped support resumes", async ({ server, assistant }) => {
  const { cards, receipt, bundle, key, state, bindingId } = await grouped(
    server.baseUrl,
    "cards/verify_in_real_time/support.yaml",
  );
  const group = "conv-1";
  const turns = [1, 2, 3, 4, 5, 6].map((turn) => ({ turn, answer: `answer ${turn}` }));
  const grouping = { positional: true, group_id: group };
  const first = state.run("agent");
  for (const turn of turns.slice(0, 3)) {
    await first.observe.verify("turn-check", turn, grouping);
  }
  const resumed = state.run("agent");
  await resumed.observe.verify("turn-check", turns[3], grouping);
  const submitted = await resumed.observe.submit("turn-check", turns[4], grouping);
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await submitted.execution.wait({ timeoutMs: WAIT_MS });
  const body = (turn: object) => ({
    verifier_card_uid: registered(receipt, "turn-check").uid,
    card_uid: registered(receipt, "assistant-agent").uid,
    group_id: group,
    inputs: { turn: { kind: "json", value: turn } },
  });
  const replayed = await enqueueOverHttp(server.baseUrl, key, "support-turn-0006", body(turns[5]));
  expect(await enqueueOverHttp(server.baseUrl, key, "support-turn-0006", body(turns[5]))).toBe(replayed);
  const conflict = await enqueueOverHttp(server.baseUrl, key, "support-turn-0006", body({ answer: "" }));
  await expect((await verify.Execution.connect(conflict)).wait({ timeoutMs: WAIT_MS })).rejects.toMatchObject({
    code: "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT",
  });
  await (await verify.Execution.connect(replayed)).wait({ timeoutMs: WAIT_MS });
  await WyrdState.fromPath(bundle, { client: WyrdClient.connect() })
    .run("agent")
    .observe.verify("turn-check", { answer: "" }, grouping);
  const other = server.scopedApiKey("other_verifier", [
    { resource: "verifier", action: "run", scope: { verifier: assistant.answerCheck.uid } },
  ]);
  await expect(
    WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: other }) })
      .run("agent")
      .observe.verify("turn-check", { answer: "" }, grouping),
  ).rejects.toMatchObject(DENIED);
  await state.shutdown();

  const judged = await groupExecution(server, bindingId, WAIT_MS);

  expect([judged.group_id, judged.owner_card_uid]).toEqual([group, registered(receipt).uid]);
  expect(judged.group?.members, "six inputs and six results").toBe(12);
  expect(judged.group?.sources.turns.execution_ids).toHaveLength(6);
  expect(judged.group?.sources.turns.execution_ids[5]).toBe(replayed);
  expect(judged.result?.outcome.evidence.turns).toEqual({ kind: "json", value: turns });
  expect(judged.result?.outcome.verdict).toBe("passed");
  await expect(
    cards.registerFromPath(fixture("cards/verify_in_real_time/support-unsourced.yaml")),
  ).rejects.toMatchObject({
    code: "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH",
    message: expect.stringContaining("verified_by[0].runs_on"),
  });
});

test("grouped refund changes verdict", async ({ server }) => {
  const hooks = await Receiver.start(() => ({ status: 200 }));
  await (await OperatorConnections.connect()).create({
    provider: "http",
    name: "refund-hooks",
    origin: hooks.url,
    auth: { scheme: "bearer", token: "refund-hook-token" },
  });
  const { cards, state, bindingId } = await grouped(server.baseUrl, "cards/verify_in_real_time/refund.yaml");
  const group = "order-1";
  const opened = Date.now();
  const turn = (overdue: boolean) =>
    state.run("agent").observe.verify("refund-turn", { order: "A-1", overdue }, { positional: true, group_id: group });

  await turn(false);
  await turn(false);
  const promised = await groupExecution(server, bindingId, WAIT_MS);
  for (const overdue of [false, true, false]) {
    await turn(overdue);
  }
  const corrected = await groupExecution(server, bindingId, WAIT_MS, promised.execution_id);
  await turn(false);
  const unchanged = await groupExecution(server, bindingId, WAIT_MS, corrected.execution_id);

  expect(
    [promised, corrected, unchanged].map((review) => [
      review.group?.sources.facts.execution_ids.length,
      review.result?.outcome.verdict,
      review.dispatches.length,
    ]),
    "only the verdict change dispatches",
  ).toEqual([
    [2, "passed", 0],
    [5, "failed", 1],
    [6, "failed", 0],
  ]);
  const history = await (await Bifrost.connect()).sql(
    "SELECT execution_id, result_id, verdict FROM vala.verification.results WHERE binding_id = $1 ORDER BY ended_at",
    [bindingId],
    z.object({ execution_id: z.string(), result_id: z.string(), verdict: z.string() }),
  );
  expect(
    history.map((row) => [row.execution_id, row.verdict]),
    "every review stays retained",
  ).toEqual([
    [promised.execution_id, "passed"],
    [corrected.execution_id, "failed"],
    [unchanged.execution_id, "failed"],
  ]);
  const [alert] = await hooks.requests(1);
  expect(alert?.path).toBe("/hooks/refund-review");
  expect(JSON.parse(alert?.body ?? "null")).toEqual({
    execution_id: corrected.execution_id,
    result_id: history[1]?.result_id,
    verdict: "failed",
    group_id: group,
    previous_result_id: history[0]?.result_id,
    previous_verdict: "passed",
  });

  // The fixture's 40 s finality, counted from the group's first turn, must
  // pass before the late turn; waiting for it is the scenario.
  await new Promise((resolve) => setTimeout(resolve, Math.max(0, opened + 41_000 - Date.now())));
  await turn(true);
  await state.shutdown();
  const token = await WyrdClient.connect().accessToken();
  const quiet = Date.now() + 8_000;
  while (Date.now() < quiet) {
    server.flushBifrost();
    const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${bindingId}`, {
      headers: { "x-wyrd-access-token": `Bearer ${token}` },
    });
    expect(
      ((await response.json()) as { last_execution_id: string | null }).last_execution_id,
      "a turn after finality opens no review",
    ).toBe(unchanged.execution_id);
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  expect(hooks.received).toHaveLength(1);
  await hooks.close();
  await expect(
    cards.registerFromPath(fixture("cards/verify_in_real_time/refund-on-failure.yaml")),
  ).rejects.toMatchObject({
    code: "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH",
    message: expect.stringContaining("verified_by[1].on_failure"),
  });
}, 180_000);

test("grouped coding uses declared sources", async ({ server, provider, assistant: _ }) => {
  const { cards, principal, state, bindingId, key } = await grouped(
    server.baseUrl,
    "cards/verify_in_real_time/coding.yaml",
  );
  server.scopedApiKey("ts_live_reader", ["tool:invoke", "bifrost_query:read"]);
  const principals = await Principals.connect();
  await principals.grantRole(principal, "ts_live_reader");
  const spans = new BasicTracerProvider({
    spanProcessors: [new SimpleSpanProcessor(spanExporter(WyrdClient.connect({ credential: key })))],
  });
  const tracer = spans.getTracer("journey.coding");
  const session = tracer.startSpan("coding-session");
  tracer.startSpan("edit-file", {}, trace.setSpan(context.active(), session)).end();
  const traceId = session.spanContext().traceId;
  const calls = [
    { call: "prompt", tests: "passed" },
    { call: "edit", tests: "failed" },
    { call: "test", tests: "passed" },
  ];
  const run = state.run("agent");
  const verdicts = [];
  for (const [index, call] of calls.entries()) {
    const inputs: VerifierInputs = index === 0 ? { call, trace: traceId } : { call };
    verdicts.push((await run.observe.verify("coding-turn", inputs, { group_id: "session-1" })).outcome.verdict);
  }
  expect(verdicts).toEqual(["passed", "failed", "passed"]);
  const empty = await run.observe.verify("empty-each", [], { positional: true });
  expect(empty.outcome.verdict).toBe("inconclusive");
  expect(
    empty.outcome.requirements.map((requirement) => [requirement.id, requirement.outcome, requirement.reason]),
  ).toEqual([
    ["listed-ok", "inconclusive", "empty_selection"],
    ["items-ok", "inconclusive", "empty_selection"],
  ]);
  tracer.startSpan("late-test-run", {}, trace.setSpan(context.active(), session)).end();
  session.end();
  await spans.shutdown();

  const judged = await groupExecution(server, bindingId, WAIT_MS);

  const outcome = judged.result?.outcome;
  expect(outcome?.evidence.calls).toEqual({ kind: "json", value: calls });
  expect(outcome?.evidence.traces).toEqual({ kind: "json", value: [traceId, null, null] });
  expect(outcome?.evidence.tests).toEqual({ kind: "json", value: ["passed", "failed", "passed"] });
  expect(
    outcome?.requirements.map((requirement) => [requirement.id, requirement.outcome]),
    JSON.stringify(outcome?.requirements),
  ).toEqual([
    ["every-call-tested", "failed"],
    ["spans-reviewed", "passed"],
  ]);
  expect(outcome?.verdict).toBe("failed");
  const reviews = () => provider.received.map((request) => request.body).filter((body) => body.includes(LIVE_REVIEW));
  const [ask, ...moreAsks] = reviews().filter((body) => !TOOL_TURN.test(body));
  expect(moreAsks).toEqual([]);
  expect(ask, "the trace slot carries its ID only").toContain(traceId);
  expect(ask).not.toContain("edit-file");
  const [read, ...moreReads] = reviews().filter((body) => TOOL_TURN.test(body));
  expect(moreReads).toEqual([]);
  expect(read, "the check reads the spans live").toContain("edit-file");
  expect(read).toContain("late-test-run");

  await principals.revokeRole(principal, "ts_live_reader");
  await run.observe.verify("coding-turn", { call: calls[0], trace: traceId }, { group_id: "session-2" });
  const revoked = await groupExecution(server, bindingId, WAIT_MS, judged.execution_id);
  expect(revoked.group_id).toBe("session-2");
  const denied = reviews().filter((body) => TOOL_TURN.test(body) && body.includes("WYRD_PERMISSION_403_DENIED_RBAC"));
  expect(denied).toHaveLength(1);
  expect(denied[0], "a denied read returns nothing").not.toContain("edit-file");
  await state.shutdown();

  for (const [relative, code, reason] of [
    ["coding-child.yaml", "WYRD_LOADER_400_INVALID_ENVELOPE", "child fields"],
    ["coding-unbound.yaml", "WYRD_SPEC_400_VALIDATION", "matches 0 bindings"],
    ["coding-cycle.yaml", "WYRD_SPEC_400_VALIDATION", "forms a source cycle"],
  ] as const) {
    const refused = await cards.registerFromPath(fixture(`cards/verify_in_real_time/${relative}`)).then(
      () => undefined,
      (error: unknown) => error as WyrdError,
    );
    expect(refused?.code, relative).toBe(code);
    expect(`${refused?.message} ${JSON.stringify(refused?.details)}`, relative).toContain(reason);
  }
}, 180_000);
