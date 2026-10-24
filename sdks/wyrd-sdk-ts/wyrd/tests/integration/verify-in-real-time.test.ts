import { mkdtempSync, writeFileSync } from "node:fs";
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
  type VerificationResult,
  type VerifierInputs,
  WyrdClient,
  type WyrdError,
  WyrdState,
  verify,
} from "wyrd-ai";
import { spanExporter } from "wyrd-ai/otel";

import {
  AGREES,
  type Assistant,
  assistantAs,
  assistantTest,
  judgeCalls,
  judgeReply,
  type JudgeCall,
  registerAssistant,
  toolCall,
  toolTurn,
} from "../support/assistant.js";
import { type ReceivedRequest, Receiver, type Reply } from "../support/http-receiver.js";
import { ancestry, binding, fixture, groupExecution, declared, runtimeAccount, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** How long a test waits for a queued execution or a scheduled result. */
const WAIT_MS = 90_000;

/** The trace identity of a caller's W3C `traceparent`. */
const CALLER_TRACE = "0af7651916cd43dd8448eb211c80319c";
/** The caller's span identity within {@link CALLER_TRACE}. */
const CALLER_SPAN = "b7ad6b7169203331";

/** The stable refusal for a caller no grant authorizes. */
const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** The `coding-verifier` statement every request of its `spans-reviewed` check carries. */
const LIVE_REVIEW = "spans were reviewed live";

/** What a Chat Completions request carrying tool results contains. */
const TOOL_TURN = /"role"\s*:\s*"tool"/;

/** The coding judge's Verdict once it read the spans: passed, citing the first trace ID. */
const REVIEWED =
  '{"outcome":"passed","reason":"the spans were read","citations":[{"slot":"traces","locator":{"json_pointer":"/0"}}]}';

/** Text every `frame-verifier` judge request carries. */
const FRAME_CLAIM = "Cite the frame.";

/** The nine-byte label of the one frame whose person wears no badge. */
const UNBADGED = "no badge!";

/** {@link UNBADGED} in base64, as the judge's image data URL carries it. */
const UNBADGED_BASE64 = "bm8gYmFkZ2Uh";

/** The frame judge's Verdict for a frame showing a badge: passed, citing the frame as its one page. */
const BADGED = '{"outcome":"passed","reason":"a badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}';

/** The frame judge's Verdict for the frame showing no badge. */
const NO_BADGE =
  '{"outcome":"failed","reason":"no badge is visible","citations":[{"slot":"frame","locator":{"page":1}}]}';

/**
 * The file's local judge: the frame judge fails the {@link UNBADGED} frame
 * and passes every other; the coding session's `spans-reviewed` check first
 * reads the span names of the trace its answer names live through
 * `bifrost.query`, then passes with {@link REVIEWED}; every other judge agrees.
 */
function localJudge(request: ReceivedRequest): Reply {
  if (request.body.includes(FRAME_CLAIM)) {
    return judgeReply(request.body.includes(UNBADGED_BASE64) ? NO_BADGE : BADGED);
  }
  if (!request.body.includes(LIVE_REVIEW)) {
    return judgeReply(AGREES);
  }
  if (TOOL_TURN.test(request.body)) {
    return judgeReply(REVIEWED);
  }
  const [traceId] = request.body.match(/[0-9a-f]{32}/) ?? [];
  const sql = `SELECT name FROM bifrost.traces.spans WHERE trace_id = decode('${traceId}', 'hex') ORDER BY name`;
  return toolTurn([toolCall("call-spans", "bifrost.query", { sql, max_rows: 10 })]);
}

/** A fresh deployment per test, as each Rust journey starts its own, judged as {@link localJudge} answers. */
const test = assistantTest({ perTest: true, judge: localJudge });

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
       FROM bifrost.verification.results r
       JOIN bifrost.gateway.calls c ON c.subject_card_uid = r.subject_card_uid
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
  const token = await WyrdClient.connect({ credential: assistant.key })
    .forWorkload(assistant.service.uid)
    .accessToken();
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
  expect(calls.map((call) => [call.sub_principal_id, call.run_id, call.subject_card_uid])).toEqual([
    [assistant.principal, run.runId, result.subject.uid],
  ]);
  await assertCalledAsSystem(result.execution_id, assistant.principal, run.runId);
});

test("judge call without a run joins the callers trace attributed to the card", async ({ server, assistant }) => {
  const execution = await verifyOverHttp(server.baseUrl, assistant, `00-${CALLER_TRACE}-${CALLER_SPAN}-01`);

  const calls = await judgeCalls(server);
  expect(calls.map((call) => [call.sub_principal_id, call.run_id, call.subject_card_uid])).toEqual([
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
      client: WyrdClient.connect({ credential: assistant.key }).forWorkload(assistant.service.uid),
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
  await cards.declareFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  await cards.declareFromPath(fixture("cards/verify_in_real_time/latency-model.yaml"));
  const { root } = await cards.declareFromPath(fixture("cards/verify_in_real_time/unfitted-assistant.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-unfitted-"));
  await cards.pull(root, bundle);
  const { key } = await runtimeAccount({ kind: "Service", name: root.name });
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
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
      code: "WYRD_PERMISSION_500_ROLE_CORRUPT",
    });
  }

  const bifrost = await Bifrost.connect();
  expect((await bifrost.listTables()).every((table) => table.namespace !== "bifrost.eval")).toBe(true);
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
    const retired = await refusals("bifrost.eval", name);
    expect(retired).not.toContain("accepted");
    expect(retired).toEqual(await refusals("bifrost.absent", name));
  }
  await bifrost.sql("SELECT COUNT(*) AS n FROM bifrost.verification.results");
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
      code: "WYRD_AUTH_401_WORKLOAD_UID_REFUSED",
    });
  }
});

/** Enqueue `body` over plain HTTP as `client` with `idempotencyKey`, returning the accepted execution. */
async function enqueueOverHttp(
  baseUrl: string,
  client: WyrdClient,
  idempotencyKey: string,
  body: object,
): Promise<string> {
  const token = await client.accessToken();
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
 * Register and hydrate the Service fixture at `path`, issue a key of the
 * Service's runtime account, start its state with that key, and find the one
 * binding whose status pins `group_end` settings. `client` runs as the
 * Service for raw HTTP and span export.
 */
async function grouped(baseUrl: string, path: string) {
  const cards = await Cards.connect();
  const receipt = await cards.declareFromPath(fixture(path));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-grouped-"));
  await cards.pull(receipt.root, bundle);
  const { principalId, key } = await runtimeAccount({ kind: "Service", name: receipt.root.name });
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
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: key }) });
  await state.startBifrost();
  const client = WyrdClient.connect({ credential: key }).forWorkload(declared(receipt).uid);
  return { cards, receipt, bundle, client, principal: principalId, state, bindingId: bindingIds[0] };
}

test("grouped support resumes", async ({ server, assistant }) => {
  const { cards, receipt, bundle, client, state, bindingId } = await grouped(
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
    verifier_card_uid: declared(receipt, "turn-check").uid,
    card_uid: declared(receipt, "assistant-agent").uid,
    group_id: group,
    inputs: { turn: { kind: "json", value: turn } },
  });
  const replayed = await enqueueOverHttp(server.baseUrl, client, "support-turn-0006", body(turns[5]));
  expect(await enqueueOverHttp(server.baseUrl, client, "support-turn-0006", body(turns[5]))).toBe(replayed);
  // An ungrouped conflicting body: a staged input the queue later refuses
  // would otherwise join the group.
  const { group_id: _, ...ungrouped } = body({ answer: "" });
  const conflict = await enqueueOverHttp(server.baseUrl, client, "support-turn-0006", ungrouped);
  await expect((await verify.Execution.connect(conflict)).wait({ timeoutMs: WAIT_MS })).rejects.toMatchObject({
    code: "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT",
  });
  await (await verify.Execution.connect(replayed)).wait({ timeoutMs: WAIT_MS });
  // A WyrdState would carry its Service root for the administrator too, so
  // the ownerless turn is the administrator's rootless enqueue.
  const ownerless = await enqueueOverHttp(
    server.baseUrl,
    WyrdClient.connect(),
    "support-turn-ownerless",
    body({ answer: "" }),
  );
  await (await verify.Execution.connect(ownerless)).wait({ timeoutMs: WAIT_MS });
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

  expect([judged.group_id, judged.owner_card_uid]).toEqual([group, declared(receipt).uid]);
  expect(judged.group?.sources.turns.execution_ids).toHaveLength(6);
  expect(judged.group?.sources.turns.execution_ids[5]).toBe(replayed);
  expect(judged.result?.outcome.evidence.turns).toEqual({ kind: "json", value: turns });
  expect(judged.result?.outcome.verdict).toBe("passed");
  await expect(
    cards.declareFromPath(fixture("cards/verify_in_real_time/support-unsourced.yaml")),
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
  const turn = (overdue: boolean, paid?: "in_full") =>
    state
      .run("agent")
      .observe.verify(
        "refund-turn",
        { order: "A-1", overdue, ...(paid ? { paid } : {}) },
        { positional: true, group_id: group },
      );

  await turn(false);
  await turn(false);
  const promised = await groupExecution(server, bindingId, WAIT_MS);
  for (const overdue of [false, true]) {
    await turn(overdue);
  }
  const corrected = await groupExecution(server, bindingId, WAIT_MS, promised.execution_id);
  await turn(true);
  const unchanged = await groupExecution(server, bindingId, WAIT_MS, corrected.execution_id);
  await turn(false, "in_full");
  const recovered = await groupExecution(server, bindingId, WAIT_MS, unchanged.execution_id);

  expect(
    [promised, corrected, unchanged, recovered].map((execution) => [
      execution.group?.sources.facts.execution_ids.length,
      execution.result?.outcome.verdict,
      execution.result?.outcome.requirements.map((requirement) => requirement.id),
      execution.dispatches.length,
    ]),
    "only verdict changes dispatch",
  ).toEqual([
    [2, "passed", ["latest-not-overdue"], 0],
    [4, "failed", ["latest-not-overdue"], 1],
    [5, "failed", ["latest-not-overdue"], 0],
    [6, "passed", ["latest-not-overdue"], 1],
  ]);
  const history = await (await Bifrost.connect()).sql(
    "SELECT execution_id, result_id, verdict FROM bifrost.verification.results WHERE binding_id = $1 ORDER BY ended_at",
    [bindingId],
    z.object({ execution_id: z.string(), result_id: z.string(), verdict: z.string() }),
  );
  expect(
    history.map((row) => [row.execution_id, row.verdict]),
    "every execution stays retained",
  ).toEqual([
    [promised.execution_id, "passed"],
    [corrected.execution_id, "failed"],
    [unchanged.execution_id, "failed"],
    [recovered.execution_id, "passed"],
  ]);
  const alerts = await hooks.requests(2);
  expect(alerts.map((alert) => alert.path)).toEqual(["/hooks/refund-verification", "/hooks/refund-verification"]);
  expect(alerts.map((alert) => JSON.parse(alert.body))).toEqual([
    {
      execution_id: corrected.execution_id,
      result_id: history[1]?.result_id,
      verdict: "failed",
      group_id: group,
      previous_result_id: history[0]?.result_id,
      previous_verdict: "passed",
    },
    {
      execution_id: recovered.execution_id,
      result_id: history[3]?.result_id,
      verdict: "passed",
      group_id: group,
      previous_result_id: history[2]?.result_id,
      previous_verdict: "failed",
    },
  ]);

  await state.shutdown();
  expect(hooks.received).toHaveLength(2);
  await hooks.close();
  await expect(
    cards.declareFromPath(fixture("cards/verify_in_real_time/refund-on-failure.yaml")),
  ).rejects.toMatchObject({
    code: "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH",
    message: expect.stringContaining("verified_by[1].on_failure"),
  });
}, 180_000);

test("grouped coding uses declared sources", async ({ server, provider, assistant: _ }) => {
  const { cards, principal, state, bindingId, client } = await grouped(
    server.baseUrl,
    "cards/verify_in_real_time/coding.yaml",
  );
  server.scopedApiKey("ts_live_reader", ["tool:invoke", "bifrost_query:read"]);
  const principals = await Principals.connect();
  await principals.grantRole(principal, "ts_live_reader");
  const spans = new BasicTracerProvider({
    spanProcessors: [new SimpleSpanProcessor(spanExporter(client))],
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

  for (const [relative, kind, code, reason] of [
    ["invalid/coding-child", "Verifier", "WYRD_LOADER_400_INVALID_ENVELOPE", "child fields"],
    ["cards/verify_in_real_time/coding-unbound", "Service", "WYRD_SPEC_400_VALIDATION", "matches 0 bindings"],
    ["cards/verify_in_real_time/support-ambiguous", "Service", "WYRD_SPEC_400_VALIDATION", "matches 2 bindings"],
    ["cards/verify_in_real_time/support-undeclared", "Service", "WYRD_SPEC_400_VALIDATION", "does not declare"],
    ["cards/verify_in_real_time/coding-cycle", "Service", "WYRD_SPEC_400_VALIDATION", "forms a source cycle"],
    ["invalid/reserved-typed", "Verifier", "WYRD_LOADER_400_INVALID_ENVELOPE", "inputs.group_id"],
    ["invalid/reserved-source", "Verifier", "WYRD_LOADER_400_INVALID_ENVELOPE", "inputs.group_id"],
  ] as const) {
    const name = relative.split("/").at(-1) ?? relative;
    const refused = await cards.declareFromPath(fixture(`${relative}.yaml`)).then(
      () => undefined,
      (error: unknown) => error as WyrdError,
    );
    expect(refused?.code, name).toBe(code);
    expect(`${refused?.message} ${JSON.stringify(refused?.details)}`, name).toContain(reason);
    await expect(cards.resolveLatest(kind, name), name).rejects.toMatchObject({
      code: "WYRD_REGISTRY_404_CARD_NOT_FOUND",
    });
  }
  await cards.declareFromPath(fixture("cards/verify_in_real_time/coding-external.yaml"));
}, 180_000);

/**
 * A PNG frame in a fresh directory carrying the nine-byte `label` after its
 * signature and a newline, which starts the label on a base64 boundary, so
 * its base64 appears verbatim in the image data URL the judge receives.
 */
function frame(label: string): string {
  expect(label, "a frame label is nine bytes").toHaveLength(9);
  const path = join(mkdtempSync(join(tmpdir(), "wyrd-ts-frame-")), `${label}.png`);
  writeFileSync(path, Buffer.concat([Buffer.from("89504e470d0a1a0a0a", "hex"), Buffer.from(label)]));
  return path;
}

/** Submit each frame at `paths` to `frame-verifier` in one Run of `state` under `group`. */
async function submitFrames(state: WyrdState, group: string, paths: string[]): Promise<verify.Execution[]> {
  const run = state.run("agent");
  const executions = [];
  for (const path of paths) {
    const submitted = await run.observe.submit("frame-verifier", verify.File.path(path), { group_id: group });
    if (submitted.kind !== "queued") {
      throw new Error("a frame submission queues an execution");
    }
    executions.push(submitted.execution);
  }
  return executions;
}

test("grouped sampled frames use visible outcomes", async ({ server, provider, assistant }) => {
  const { cards, bundle, state, bindingId } = await grouped(server.baseUrl, "cards/verify_in_real_time/frames.yaml");
  const other = await cards.declareFromPath(fixture("cards/verify_in_real_time/frames-other.yaml"));
  const otherBundle = mkdtempSync(join(tmpdir(), "wyrd-ts-grouped-"));
  await cards.pull(other.root, otherBundle);
  const otherKey = await runtimeAccount({ kind: "Service", name: other.root.name });
  const otherState = WyrdState.fromPath(otherBundle, { client: WyrdClient.connect({ credential: otherKey.key }) });
  await otherState.startBifrost();
  const reported = (event: string) =>
    state
      .run("agent")
      .observe.verify("event-fact", { event_id: event, detected: true }, { positional: true, group_id: event });
  const badged = (judged: Awaited<ReturnType<typeof groupExecution>>) => [
    judged.result?.outcome.verdict,
    judged.result?.outcome.requirements[0]?.reason,
  ];
  const waited = (executions: verify.Execution[]) =>
    Promise.all(executions.map((execution) => execution.wait({ timeoutMs: WAIT_MS })));
  const ids = (executions: verify.Execution[]) => executions.map((execution) => execution.id);

  await reported("door-44");
  let judged = await groupExecution(server, bindingId, WAIT_MS);
  expect(judged.group_id).toBe("door-44");
  expect(badged(judged)).toEqual(["inconclusive", "empty_selection"]);
  expect(judged.result?.outcome.evidence.outcomes).toEqual({ kind: "json", value: [] });

  const failing = Array.from({ length: 19 }, (_, index) => frame(`fail-${String(index).padStart(4, "0")}`));
  failing.splice(7, 0, frame(UNBADGED));
  const executions = await submitFrames(state, "door-43", failing);
  await waited(await submitFrames(otherState, "door-43", [frame("other-001")]));
  const unauthorized = server.scopedApiKey("frames_denied", [
    { resource: "verifier", action: "run", scope: { verifier: assistant.answerCheck.uid } },
  ]);
  await expect(
    WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: unauthorized }) })
      .run("agent")
      .observe.submit("frame-verifier", verify.File.path(frame("denied-01")), { group_id: "door-43" }),
  ).rejects.toMatchObject(DENIED);
  await waited(executions);
  await reported("door-43");
  judged = await reportedGroup(server, bindingId, judged.execution_id);
  expect(judged.group_id).toBe("door-43");
  // Frames ingested in one batch share its ingest time, so their list
  // order is by execution ID rather than by submission.
  const listed = judged.group?.sources.outcomes.execution_ids ?? [];
  expect([...listed].sort(), "only the Service's own frames").toEqual(ids(executions).sort());
  expect(judged.group?.sources.event.execution_ids).toHaveLength(1);
  const unbadged = ids(executions)[7];
  const expected = listed.map((id) => (id === unbadged ? "failed" : "passed"));
  expect(judged.result?.outcome.evidence.outcomes).toEqual({ kind: "json", value: expected });
  expect(judged.result?.outcome.verdict).toBe("failed");

  for (const [group, count] of [
    ["door-45", 21],
    ["door-42", 20],
  ] as const) {
    const door = await submitFrames(
      state,
      group,
      Array.from({ length: count }, (_, index) => frame(`${group.slice(5)}-${String(index).padStart(6, "0")}`)),
    );
    await waited(door);
    await reported(group);
    judged = await reportedGroup(server, bindingId, judged.execution_id);
    expect(judged.group_id).toBe(group);
    expect([...(judged.group?.sources.outcomes.execution_ids ?? [])].sort()).toEqual(ids(door).sort());
    expect(judged.result?.outcome.evidence.outcomes).toEqual({ kind: "json", value: Array(count).fill("passed") });
    expect(judged.result?.outcome.verdict).toBe("passed");
  }
  expect(
    provider.received.filter((request) => request.body.includes(FRAME_CLAIM)),
    "every frame is judged once",
  ).toHaveLength(20 + 1 + 21 + 20);
  await state.shutdown();
  await otherState.shutdown();
}, 180_000);

/**
 * The next completed group execution of `bindingId` after `after` that pins
 * the reported event and a visible outcome for every listed frame. Frame
 * results acknowledged before the event may schedule earlier snapshots;
 * those are skipped.
 */
async function reportedGroup(
  server: { flushBifrost(): void; readonly baseUrl: string },
  bindingId: string,
  after: string,
): Promise<Awaited<ReturnType<typeof groupExecution>>> {
  for (;;) {
    const judged = await groupExecution(server, bindingId, WAIT_MS, after);
    const outcomes = judged.result?.outcome.evidence.outcomes;
    if (judged.group?.sources.event.execution_ids.length === 1 && !JSON.stringify(outcomes).includes("null")) {
      return judged;
    }
    after = judged.execution_id;
  }
}

/** One retained result's execution identity. */
const Retained = z.object({ execution_id: z.string() });

/** The executions of `bindingId`'s retained group executions, oldest first, after publishing every captured row. */
async function groupExecutions(server: { flushBifrost(): void }, bindingId: string): Promise<string[]> {
  server.flushBifrost();
  const rows = await (await Bifrost.connect()).sql(
    "SELECT execution_id FROM bifrost.verification.results WHERE binding_id = $1 ORDER BY ended_at",
    [bindingId],
    Retained,
  );
  return rows.map((row) => row.execution_id);
}

test("grouped conversations coexist with drift", async ({ server, state: drift }) => {
  const { receipt, client, state, bindingId } = await grouped(server.baseUrl, "cards/verify_in_real_time/support.yaml");
  const turn = (answer: string) => ({ answer });
  const said = (group: string, answer: string) =>
    state.run("agent").observe.verify("turn-check", turn(answer), { positional: true, group_id: group });

  await drift.startBifrost();
  expect(await drift.run("model").observe.submit("latency-drift", { latency: 12.5 })).toEqual({ kind: "recorded" });
  await said("conv-a", "a1");
  await said("conv-b", "b1");
  const queued = await state
    .run("agent")
    .observe.submit("turn-check", turn("a2"), { positional: true, group_id: "conv-a" });
  if (queued.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await said("conv-b", "");
  const solo = await state.run("agent").observe.verify("turn-check", turn("solo"), { positional: true });
  await queued.execution.wait({ timeoutMs: WAIT_MS });
  const cards = await Cards.connect();
  const newer = await cards.declareFromPath(fixture("cards/verify_in_real_time/turn-check-newer.yaml"));
  const late = await enqueueOverHttp(server.baseUrl, client, "support-newer-turn", {
    verifier_card_uid: declared(newer).uid,
    card_uid: declared(receipt, "assistant-agent").uid,
    group_id: "conv-a",
    inputs: { turn: { kind: "json", value: turn("newer") } },
  });
  await (await verify.Execution.connect(late)).wait({ timeoutMs: WAIT_MS });
  const tenantB = WyrdClient.connect({
    credential: server.bootstrapServiceInTenant(server.seedTenant("tenant-b"), ["admin"], "tenant_b_admin"),
  });
  const bCards = await Cards.connect({ client: tenantB });
  const bReceipt = await bCards.declareFromPath(fixture("cards/verify_in_real_time/support.yaml"));
  const bBundle = mkdtempSync(join(tmpdir(), "wyrd-ts-grouped-"));
  await bCards.pull(bReceipt.root, bBundle);
  const bKey = await runtimeAccount({ kind: "Service", name: bReceipt.root.name }, tenantB);
  const bState = WyrdState.fromPath(bBundle, { client: WyrdClient.connect({ credential: bKey.key }) });
  await bState.startBifrost();
  await bState.run("agent").observe.verify("turn-check", turn("tenant b"), { positional: true, group_id: "conv-a" });
  for (const each of [state, drift, bState]) {
    await each.shutdown();
  }

  const deadline = Date.now() + WAIT_MS;
  let executions = await groupExecutions(server, bindingId);
  while (executions.length < 2) {
    expect(Date.now(), "both conversations are verified").toBeLessThan(deadline);
    await new Promise((resolve) => setTimeout(resolve, 500));
    executions = await groupExecutions(server, bindingId);
  }
  const judged: Record<string, unknown> = {};
  for (const id of executions) {
    const execution = await (await verify.Execution.connect(id)).status();
    judged[execution.group_id ?? ""] = [
      execution.group?.sources.turns.execution_ids.length,
      execution.result?.outcome.evidence.turns,
      execution.result?.outcome.verdict,
    ];
  }
  expect(judged, "each conversation judges only its own pinned turns").toEqual({
    "conv-a": [2, { kind: "json", value: [turn("a1"), turn("a2")] }, "passed"],
    "conv-b": [2, { kind: "json", value: [turn("b1"), turn("")] }, "failed"],
  });
  const ungrouped = await (await Bifrost.connect()).sql(
    "SELECT execution_id FROM bifrost.verification.results WHERE execution_id = $1 AND group_id IS NULL",
    [solo.execution_id],
    Retained,
  );
  expect(ungrouped, "the ungrouped turn joins no group").toHaveLength(1);
  const quiet = Date.now() + 8_000;
  while (Date.now() < quiet) {
    expect(await groupExecutions(server, bindingId), "without new activity no group is verified again").toEqual(
      executions,
    );
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
});

/** Scribe outbox capacity of the staging-pressure story: its reserved three quarters, four writes, is what two direct judgments reserve. */
const STAGING_CAPACITY = 6;

/** How long the staging-pressure story's judge holds each call. */
const JUDGE_DELAY_MS = 15_000;

/** Judge calls the staging-pressure story's judge has received, counted on arrival. */
let judgeArrivals = 0;

/** A fresh deployment whose judge holds every call and whose staging is small. */
const stagingTest = assistantTest({
  perTest: true,
  scribeOutboxCapacity: STAGING_CAPACITY,
  judge: () => {
    judgeArrivals += 1;
    return { ...judgeReply(AGREES), delayMs: JUDGE_DELAY_MS };
  },
});

/** Resolve once the judge has received `calls` calls. */
async function awaitJudgeCalls(calls: number): Promise<void> {
  await vi.waitFor(() => expect(judgeArrivals).toBeGreaterThanOrEqual(calls), { timeout: WAIT_MS, interval: 20 });
}

stagingTest("verification_staging_refuses_and_recovers", async ({ server, state }) => {
  judgeArrivals = 0;
  const neighborClient = WyrdClient.connect({
    credential: server.bootstrapServiceInTenant(server.seedTenant("burst-neighbor"), ["admin"], "burst-admin"),
  });
  const registeredNeighbor = await registerAssistant(neighborClient);
  const neighbor = assistantAs(registeredNeighbor, WyrdClient.connect({ credential: registeredNeighbor.key }));
  const support = await grouped(server.baseUrl, "cards/verify_in_real_time/support.yaml");
  const pressured = { answer: "pressured" };
  const grouping = { positional: true, group_id: "pressured" };
  server.flushBifrost();

  const burst: Promise<VerificationResult>[] = [];
  const finished: boolean[] = [];
  let offered = 0;
  for (const inFlight of [1, 2]) {
    const judging = state.run("agent").observe.verify("answer-judged", { answer: "yes" });
    void judging.then(
      () => (finished[inFlight - 1] = true),
      () => (finished[inFlight - 1] = true),
    );
    burst.push(judging);
    await awaitJudgeCalls(inFlight);
    if (inFlight === 1) {
      const accepted = await neighbor.run("agent").observe.submit("answer-check", { answer: "yes" });
      if (accepted.kind !== "queued") {
        throw new Error("Run evidence queues an execution");
      }
      let neighborDone = false;
      const neighborWaits = accepted.execution.wait({ timeoutMs: WAIT_MS }).finally(() => (neighborDone = true));
      do {
        const normal = await state.run("agent").observe.submit("answer-check", { answer: "yes" });
        if (normal.kind !== "queued") {
          throw new Error("Run evidence queues an execution");
        }
        expect((await normal.execution.wait({ timeoutMs: WAIT_MS })).outcome.verdict).toBe("passed");
        offered += 1;
      } while (!neighborDone);
      const judged = await neighborWaits;
      expect(judged.outcome.verdict).toBe("passed");
      expect(finished[0], "the burst's judgment is still in flight").toBeUndefined();
      server.flushBifrost();
    }
  }

  const refusedRun = state.run("agent");
  const refusal = {
    code: "WYRD_VERIFICATION_429_STAGING_FULL",
    status: 429,
    details: { retry_after_seconds: 1 },
  };
  const groupedRun = support.state.run("agent");
  await expect(refusedRun.observe.verify("answer-check", { answer: "yes" })).rejects.toMatchObject(refusal);
  await expect(refusedRun.observe.submit("answer-check", { answer: "yes" })).rejects.toMatchObject(refusal);
  await expect(groupedRun.observe.verify("turn-check", pressured, grouping)).rejects.toMatchObject(refusal);
  await expect(groupedRun.observe.submit("turn-check", pressured, grouping)).rejects.toMatchObject(refusal);
  expect(finished, "the burst's judgments are still in flight").toEqual([]);
  for (const judged of await Promise.all(burst)) {
    expect(judged.outcome.verdict).toBe("passed");
  }

  server.flushBifrost();
  const recovered = await refusedRun.observe.submit("answer-check", { answer: "yes" });
  if (recovered.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  const judged = await recovered.execution.wait({ timeoutMs: WAIT_MS });
  expect(judged.outcome.verdict).toBe("passed");
  expect(judged.run_id).toBe(refusedRun.runId);
  const turn = await groupedRun.observe.submit("turn-check", pressured, grouping);
  if (turn.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await turn.execution.wait({ timeoutMs: WAIT_MS });
  await support.state.shutdown();
  const group = await groupExecution(server, support.bindingId, WAIT_MS);
  expect(
    [group.group_id, group.group?.sources.turns.execution_ids],
    "the refused grouped turns staged nothing",
  ).toEqual(["pressured", [turn.execution.id]]);
  expect(group.result?.outcome.evidence.turns).toEqual({ kind: "json", value: [pressured] });

  server.flushBifrost();
  const rows = await (await Bifrost.connect()).sql(
    "SELECT count(*) AS n FROM bifrost.verification.execution_inputs",
    [],
    z.object({ n: z.coerce.number() }),
  );
  expect(rows).toEqual([{ n: 4 + offered }]);
});
