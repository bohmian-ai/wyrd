import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { trace } from "@opentelemetry/api";
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Cards, type VerifierInputs, WyrdClient, WyrdState, verify } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { type Assistant, assistantAs, assistantTest, judgeCalls, type JudgeCall } from "../support/assistant.js";
import { ancestry, binding, fixture, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** How long a test waits for a queued execution or a scheduled result. */
const WAIT_MS = 90_000;

/** The trace identity of a caller's W3C `traceparent`. */
const CALLER_TRACE = "0af7651916cd43dd8448eb211c80319c";
/** The caller's span identity within {@link CALLER_TRACE}. */
const CALLER_SPAN = "b7ad6b7169203331";

/** The stable refusal for a caller no grant authorizes. */
const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** A fresh deployment per test, as each Rust journey starts its own. */
const test = assistantTest({ perTest: true });

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
  await state.startTelemetry();
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
    server.scopedApiKey("evals_only", ["evals:run"]),
    server.scopedApiKey("other_verifier", [
      { resource: "verifier", action: "run", scope: { verifier: assistant.answerJudged.uid } },
    ]),
  ];
  for (const credential of keys) {
    const run = assistantAs(assistant, WyrdClient.connect({ credential })).run("agent");

    await expect(run.observe.verify("answer-check", { answer: "yes" })).rejects.toMatchObject(DENIED);
  }
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
