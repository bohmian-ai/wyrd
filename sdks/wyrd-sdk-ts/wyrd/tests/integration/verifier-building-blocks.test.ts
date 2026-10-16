// Verifier building blocks, end to end: one `policy-review` Verifier runs
// every action kind (`extract`, `agent`, `code`) and every Run check kind
// (`assert`, `trace`, `agent`, `code`) over an answer, its refund policy PDF,
// and the trace that produced it; `document-formats` extracts each of the nine
// extractable formats into its locked output.
//
// The local upstream stands in for the model provider: the refund reader first
// calls `fs.read` on its workspace copy of the policy and then answers a
// 100-dollar limit, the judge passes every answer with a citation unless the
// answer names a scripted case, and the `tool-review` investigator calls the
// Card, Bifrost, and code tools at once and then finishes. Scripted answer
// texts also fail the reader's provider, schema, and deadline and the code step
// and check; scoped and delegated principals exercise every tool grant.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import {
  Bifrost,
  Cards,
  type CardTypes,
  Principals,
  verify,
  type VerificationResult,
  type VerifierInputs,
  WyrdClient,
  WyrdState,
} from "wyrd-ai";
import { spanExporter } from "wyrd-ai/otel";
import { cli } from "@wyrd/testing";

import { judgeReply } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { type RegisteredRef, fixture, registered, serverTest, timeline } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** How long a test waits for a queued execution. */
const WAIT_MS = 90_000;

/** The judge's Verdict for an answer naming no scripted case: passed, citing the answer text. */
const FOLLOWS =
  '{"outcome":"passed","reason":"within the policy","citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}';

/** The judge's Verdict for an answer containing `REFUSE-POLICY`: failed, citing the answer text. */
const BREAKS =
  '{"outcome":"failed","reason":"contradicts the policy","citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}';

/** The judge's Verdict for an answer containing `UNCITED`: a pass with no citation, which must not count as one. */
const UNCITED = '{"outcome":"passed","reason":"trust me","citations":[]}';

/** The judge's Verdict for an answer containing `JUDGE-FOREIGN-CITE`: a pass citing a JSON pointer the answer does not have. */
const FOREIGN_CITE =
  '{"outcome":"passed","reason":"see the note","citations":[{"slot":"answer","locator":{"json_pointer":"/note"}}]}';

/** The reader's reply to an answer containing each scripted failure marker, ahead of its tool turn. */
const READER_FAILURES: ReadonlyArray<readonly [string, Reply]> = [
  ["PROVIDER-DOWN", { status: 500 }],
  ["OFF-SCHEMA", judgeReply('{"wrong":1}')],
  ["SLOW-READER", { ...judgeReply('{"max_refund":100}'), delayMs: 15_000 }],
];

/** The three built-ins the investigator calls, in call order. */
const TOOLS = ["cards.get_by_ref", "bifrost.query", "code.run"] as const;

/** The stable code of a permission denial, carried by every denied tool call's error. */
const DENIED = "WYRD_PERMISSION_403_DENIED_RBAC";

/** The permissions every `tool-review` principal holds: run any Verifier and invoke the gateway its investigator calls the model through. */
const VERIFIES = ["verifier:run", "gateway:invoke"];

/** One assistant tool call of `tool` with JSON `args`, as the model returns it. */
function toolCall(id: string, tool: string, args: unknown): unknown {
  return { id, type: "function", function: { name: tool, arguments: JSON.stringify(args) } };
}

/** The investigator's one tool turn: one call of each of {@link TOOLS}, in order. */
const INVESTIGATION = [
  toolCall("call-card", TOOLS[0], {
    card_ref: { kind: "Service", name: "policy-desk", version: "1.0.0", space: "default" },
  }),
  toolCall("call-query", TOOLS[1], { sql: "SELECT execution_id FROM vala.verification.results", max_rows: 1 }),
  toolCall("call-code", TOOLS[2], { code: "print(6 * 7)" }),
];

/** The policy every review reads: the support desk's checked-in refund policy. */
const POLICY = resolve(import.meta.dirname, "../../../../../examples/support-desk/service/refund-policy.pdf");

/** A Chat Completions answer whose one assistant turn calls `calls`. */
function toolTurn(calls: readonly unknown[]): Reply {
  const reply = judgeReply("");
  const message = { role: "assistant", content: null, tool_calls: calls };
  return {
    ...reply,
    body: { ...(reply.body as object), choices: [{ index: 0, finish_reason: "tool_calls", message }] },
  };
}

/**
 * The local model upstream. The refund reader fails by the scripted failure
 * its answer names; otherwise its first turn calls `fs.read` on the workspace
 * policy and its turn with the tool result answers the limit. The investigator
 * calls every tool at once and finishes once it holds their results; the judge
 * answers by the scripted case its claim names.
 */
function upstream(request: ReceivedRequest): Reply {
  const body = request.body;
  const toolResults = /"role"\s*:\s*"tool"/.test(body);
  if (body.includes("REFUND RULES")) {
    const failure = READER_FAILURES.find(([marker]) => body.includes(marker));
    if (failure !== undefined) {
      return failure[1];
    }
    if (toolResults) {
      return judgeReply('{"max_refund":100}');
    }
    return toolTurn([toolCall("call-read-policy", "fs.read", { path: "/inputs/policy.pdf" })]);
  }
  if (body.includes("INVESTIGATE")) {
    return toolResults ? judgeReply('{"done":true}') : toolTurn(INVESTIGATION);
  }
  for (const [marker, verdict] of [
    ["REFUSE-POLICY", BREAKS],
    ["UNCITED", UNCITED],
    ["JUDGE-MALFORMED", "not a verdict"],
    ["JUDGE-FOREIGN-CITE", FOREIGN_CITE],
    ["Claim:", FOLLOWS],
  ] as const) {
    if (body.includes(marker)) {
      return judgeReply(verdict);
    }
  }
  return { status: 404 };
}

/** The `policy-review` inputs: an answer with `text` offering `refund` dollars, the policy, and its trace when there is one. */
function review(text: string, refund: number, trace?: string): VerifierInputs {
  return { answer: { text, refund }, policy: verify.File.path(POLICY), ...(trace === undefined ? {} : { trace }) };
}

/** The citation of the answer's text, which every cited Verdict of the story names. */
const ANSWER_TEXT = [{ slot: "answer", locator: { json_pointer: "/text" } }];

/** The requirement `id` of `result`. */
function requirement(result: VerificationResult, id: string): CardTypes.RequirementResult {
  const found = result.outcome.requirements.find((each) => each.id === id);
  if (found === undefined) {
    throw new Error(`requirement ${id}: ${JSON.stringify(result)}`);
  }
  return found;
}

/** The step `id` of `result`. */
function step(result: VerificationResult, id: string): CardTypes.StepResult {
  const found = result.outcome.steps.find((each) => each.id === id);
  if (found === undefined) {
    throw new Error(`step ${id}: ${JSON.stringify(result)}`);
  }
  return found;
}

/** The registered `policy-desk` Service, its hydrated bundle, its own client, and its hydrated state with Bifrost started. */
interface PolicyDesk {
  readonly bundle: string;
  readonly root: RegisteredRef;
  readonly client: WyrdClient;
  readonly state: WyrdState;
}

/**
 * `test` on a fresh verification-capable server per test whose gateway
 * deploys `gpt-4o` on the local upstream, with `policy-desk` registered and
 * hydrated and acting as its own Card-scoped key.
 */
const test = serverTest({ provider: upstream, verificationRuntime: true, perTest: true }).extend<{
  desk: PolicyDesk;
}>({
  desk: async ({ server: _ }, use) => {
    await configureGateway(WyrdClient.connect());
    const cards = await Cards.connect();
    const receipt = await cards.registerFromPath(fixture("cards/verifier_building_blocks/policy-desk.yaml"));
    const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-policy-desk-"));
    await cards.hydrate(receipt.root, bundle);
    const root = registered(receipt);
    const client = WyrdClient.connect({
      credential: (await cli.issueKey({ ...root, space: root.space ?? "default" })).key,
    });
    const state = WyrdState.fromPath(bundle, { client });
    await state.startBifrost();
    await use({ bundle, root, client, state });
    await state.shutdown();
  },
});

/** Verify `inputs` with `policy-review` now. */
function reviewNow(desk: PolicyDesk, inputs: VerifierInputs): Promise<VerificationResult> {
  return desk.state.run().observe.verify("policy-review", inputs);
}

/**
 * Export a span named `outer`, carrying `policyId` as `policy_id`, with one
 * child named `inner` started inside it, as the Service; wait until both are
 * stored and return the trace ID. Stored spans order by start time, so `outer`
 * is the trace document's `spans[0]` and `inner` its `spans[1]`.
 */
async function recordTrace(
  server: { flushBifrost(): void },
  desk: PolicyDesk,
  [outer, inner]: readonly [string, string],
  policyId: string,
): Promise<string> {
  const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(spanExporter(desk.client))] });
  const tracer = provider.getTracer("journey.service");
  const span = tracer.startSpan(outer, { attributes: { policy_id: policyId } });
  tracer.startSpan(inner, {}, trace.setSpan(context.active(), span)).end();
  span.end();
  await provider.shutdown();
  const traceId = span.spanContext().traceId;
  expect(await timeline(server, traceId), "both spans are stored").toHaveLength(2);
  return traceId;
}

/** Ask the investigator `question` with `tool-review` now, as `client`, on the hydrated Service with Bifrost not started. */
function investigate(desk: PolicyDesk, client: WyrdClient, question: string): Promise<VerificationResult> {
  return WyrdState.fromPath(desk.bundle, { client }).run().observe.verify("tool-review", question);
}

/**
 * Assert the investigation of `question` finished after calling each of
 * {@link TOOLS} once, in order, where exactly the `allowed` calls returned
 * output and every other call was denied with {@link DENIED} and returned
 * nothing. The model's tool turn shows the policy desk's Card only when the
 * Card read was allowed.
 */
function assertTools(
  provider: { received: readonly ReceivedRequest[] },
  result: VerificationResult,
  question: string,
  allowed: readonly string[],
): void {
  const investigated = step(result, "investigate");
  expect(investigated.status, question).toBe("succeeded");
  const calls = investigated.tool_calls ?? [];
  expect(
    calls.map((call) => call.tool),
    question,
  ).toEqual([...TOOLS]);
  for (const call of calls) {
    if (allowed.includes(call.tool)) {
      expect(call.error ?? null, `${question} ${call.tool}`).toBeNull();
      expect(call.output_sha256, `${question} ${call.tool}`).toBeTruthy();
      expect(call.returned_bytes, `${question} ${call.tool}`).toBeGreaterThan(0);
    } else {
      expect(call.error ?? "", `${question} ${call.tool}`).toContain(DENIED);
      expect([call.output_sha256 ?? null, call.returned_bytes], `${question} ${call.tool}`).toEqual([null, 0]);
    }
  }
  const turn = provider.received.find(
    (request) => request.body.includes(question) && /"role"\s*:\s*"tool"/.test(request.body),
  );
  expect(turn, `${question}: the model received the tool results`).toBeDefined();
  expect(
    turn?.body.includes("document-formats"),
    `${question}: the Card reaches the model only when its read is allowed`,
  ).toBe(allowed.includes(TOOLS[0]));
}

/**
 * The grants of a principal that verifies, invokes the gateway, reads Cards
 * and Bifrost, and may call the tools `toolScope` covers: `"all"` or one exact
 * `{ tool: name }`.
 */
function grants(toolScope: unknown): unknown[] {
  return [
    { resource: "verifier", action: "run", scope: "all" },
    { resource: "gateway", action: "invoke", scope: "all" },
    { resource: "tool", action: "invoke", scope: toolScope },
    { resource: "cards", action: "read", scope: "all" },
    { resource: "bifrost_query", action: "read", scope: "all" },
  ];
}

test("policy review runs every action and check kind", async ({ server, provider, desk }) => {
  const traceId = await recordTrace(server, desk, ["lookup-policy", "draft-answer"], "refunds");

  const result = await reviewNow(desk, review("Refunds follow our policy.", 50, traceId));

  expect(result.outcome.verdict, JSON.stringify(result)).toBe("passed");
  expect(result.subject).toEqual(desk.root);
  const text = step(result, "policy-text");
  expect(text.status).toBe("succeeded");
  expect(text.output).toMatchObject({ format: "pdf" });
  expect(text.provenance?.kind).toBe("extract");
  const rules = step(result, "refund-rules");
  expect([rules.status, rules.output]).toEqual(["succeeded", { max_refund: 100 }]);
  expect(rules.tool_calls?.map((call) => call.tool)).toEqual(["fs.read"]);
  const facts = step(result, "answer-facts");
  expect([facts.status, facts.output]).toEqual(["succeeded", { within_limit: true }]);
  expect(facts.provenance?.kind).toBe("code");
  for (const id of [
    "policy-has-text",
    "limit-found",
    "refund-within-limit",
    "policy-looked-up",
    "answer-drafted-after-lookup",
    "lookup-read-refunds",
    "follows-policy",
    "names-the-policy",
  ]) {
    expect(requirement(result, id).outcome, id).toBe("passed");
  }
  expect(requirement(result, "follows-policy").citations).toEqual(ANSWER_TEXT);
  expect(requirement(result, "names-the-policy").citations).toEqual(ANSWER_TEXT);
  const judge = provider.received.find((request) => request.body.includes("Claim:"));
  expect(judge, "the judge was asked").toBeDefined();
  expect(judge?.body, "the judge is offered no tools").not.toContain('"tools"');
});

test("refund over the limit fails the assertion", async ({ desk }) => {
  const result = await reviewNow(desk, review("Refunds follow our policy.", 500));

  expect(result.outcome.verdict).toBe("failed");
  const over = requirement(result, "refund-within-limit");
  expect([over.outcome, over.actual]).toEqual(["failed", false]);
});

test("judge verdict fails the agent check", async ({ desk }) => {
  const result = await reviewNow(desk, review("REFUSE-POLICY: no refunds, policy or not.", 50));

  const judged = requirement(result, "follows-policy");
  expect(judged.outcome).toBe("failed");
  expect(judged.citations).toHaveLength(1);
  expect(result.outcome.verdict).toBe("failed");
});

test("uncited judge verdict cannot pass", async ({ desk }) => {
  const result = await reviewNow(desk, review("UNCITED: refunds follow our policy.", 50));

  expect(requirement(result, "follows-policy").outcome).not.toBe("passed");
  expect(result.outcome.verdict).not.toBe("passed");
});

test("raising code check cannot pass", async ({ desk }) => {
  const result = await reviewNow(desk, review("raise", 50));

  expect(requirement(result, "names-the-policy").outcome).not.toBe("passed");
  expect(result.outcome.verdict).not.toBe("passed");
});

test("forbidden code access cannot pass dependent work", async ({ desk }) => {
  const result = await reviewNow(desk, review("read the host", 50));

  expect(step(result, "answer-facts").status).toBe("failed");
  expect(requirement(result, "refund-within-limit").outcome).not.toBe("passed");
  expect(result.outcome.verdict).not.toBe("passed");
});

test("trace check fails a guess and needs its trace", async ({ server, desk }) => {
  const guessed = await recordTrace(server, desk, ["guess-policy", "draft-answer"], "refunds");

  const failed = await reviewNow(desk, review("Refunds follow our policy.", 50, guessed));
  const untraced = await reviewNow(desk, review("Refunds follow our policy.", 50));

  expect(requirement(failed, "policy-looked-up").outcome).toBe("failed");
  expect(requirement(untraced, "policy-looked-up").outcome).toBe("inconclusive");
  expect(untraced.outcome.verdict).not.toBe("passed");
});

test("missing policy is inconclusive", async ({ desk }) => {
  const result = await reviewNow(desk, { answer: { text: "Refunds follow our policy.", refund: 50 } });

  expect(result.outcome.verdict).toBe("inconclusive");
});

test("malformed policy cannot pass", async ({ desk }) => {
  const result = await reviewNow(desk, {
    answer: { text: "Refunds follow our policy.", refund: 50 },
    policy: verify.File.bytes(new TextEncoder().encode("not a pdf"), { format: "pdf" }),
  });

  expect(step(result, "policy-text").status).toBe("failed");
  expect(requirement(result, "policy-has-text").outcome).not.toBe("passed");
  expect(result.outcome.verdict).not.toBe("passed");
});

test("oversized policy is refused locally", async ({ provider, desk }) => {
  const received = provider.received.length;
  const refused = reviewNow(desk, {
    answer: { text: "Refunds follow our policy.", refund: 50 },
    policy: verify.File.bytes(new Uint8Array(65_537).fill(0x25), { format: "pdf" }),
  });

  await expect(refused).rejects.toMatchObject({ code: "WYRD_VERIFICATION_413_INPUT_TOO_LARGE" });
  expect(provider.received.slice(received), "nothing ran").toEqual([]);
});

test("submitted review is judged in the background", async ({ desk }) => {
  const submitted = await desk.state.run().observe.submit("policy-review", review("Refunds follow our policy.", 500));
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }

  const resumed = await verify.Execution.connect(submitted.execution.id, { client: desk.client });
  const result = await resumed.wait({ timeoutMs: WAIT_MS });
  const status = await resumed.status();

  expect(status.status).toBe("completed");
  expect(status.result).toEqual(result);
  expect(status.error).toBeUndefined();
  expect(result.outcome.verdict).toBe("failed");
  expect(requirement(result, "refund-within-limit").outcome).toBe("failed");
});

test("every format extracts its locked output", async ({ desk }) => {
  const documents: ReadonlyArray<readonly [string, unknown]> = [
    ["sample.pdf", { format: "pdf", pages: [{ number: 1, text: "Refunds" }] }],
    [
      "sample.pptx",
      {
        format: "pptx",
        slides: [{ number: 1, title: null, paragraphs: ["a"], tables: [{ rows: [["x", ""]] }], notes: "" }],
      },
    ],
    [
      "sample.docx",
      {
        format: "docx",
        blocks: [
          { kind: "paragraph", text: "Refund policy", style: "Heading1" },
          {
            kind: "table",
            rows: [
              ["Window", "Days"],
              ["Standard", "30"],
            ],
          },
        ],
      },
    ],
    [
      "sample.xlsx",
      {
        format: "xlsx",
        sheets: [
          {
            name: "Refunds",
            cells: [
              { address: "A1", value: "Days", formula: null, error: null },
              { address: "B2", value: 30, formula: null, error: null },
              { address: "C3", value: null, formula: "=1/0", error: "#DIV/0!" },
              { address: "D4", value: true, formula: null, error: null },
            ],
          },
        ],
      },
    ],
    [
      "sample.csv",
      {
        format: "csv",
        rows: [
          ["a", "b"],
          ["1", "2"],
        ],
      },
    ],
    ["sample.json", { format: "json", value: { nested: [1, null] } }],
    ["sample.txt", { format: "text", text: "" }],
    ["sample.md", { format: "markdown", text: "# Title" }],
    ["sample.html", { format: "html", text: "<p>x</p>" }],
  ];

  for (const [name, expected] of documents) {
    const result = await desk.state.run().observe.verify("document-formats", {
      document: verify.File.path(fixture(`cards/verifier_building_blocks/samples/${name}`)),
    });

    expect(step(result, "read").output, name).toEqual(expected);
    expect(requirement(result, "extracted").outcome, name).toBe("passed");
  }
});

test("document passed positionally fills the only slot", async ({ desk }) => {
  const result = await desk.state
    .run()
    .observe.verify("document-formats", verify.File.path(fixture("cards/verifier_building_blocks/samples/sample.md")));

  expect(step(result, "read").output).toEqual({ format: "markdown", text: "# Title" });
  expect(requirement(result, "extracted").outcome).toBe("passed");
});

test("positional value for a multi slot verifier is refused locally", async ({ provider, desk }) => {
  const received = provider.received.length;

  await expect(desk.state.run().observe.verify("policy-review", "Refunds follow our policy.")).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_400_INPUT_INVALID",
  });
  expect(provider.received.slice(received), "nothing ran").toEqual([]);
});

test("failed reader leaves its dependents inconclusive", async ({ desk }) => {
  for (const [marker] of READER_FAILURES) {
    const result = await reviewNow(desk, review(`${marker}: refunds follow our policy.`, 50));

    const rules = step(result, "refund-rules");
    expect(rules.status, `${marker}: ${JSON.stringify(rules)}`).toBe("failed");
    expect(rules.reason, `${marker}: the failure says why`).toBeTruthy();
    expect(step(result, "answer-facts").status, marker).toBe("skipped");
    for (const id of ["limit-found", "refund-within-limit", "follows-policy"]) {
      expect(requirement(result, id).outcome, `${marker} ${id}`).toBe("inconclusive");
    }
    expect(result.outcome.verdict, marker).toBe("inconclusive");
  }
});

test("failed code step leaves its assertion inconclusive", async ({ desk }) => {
  for (const text of ["fail the facts", "spin the facts"]) {
    const result = await reviewNow(desk, review(text, 50));

    const facts = step(result, "answer-facts");
    expect(facts.status, `${text}: ${JSON.stringify(facts)}`).toBe("failed");
    expect(facts.reason, `${text}: the failure says why`).toBeTruthy();
    expect(requirement(result, "refund-within-limit").outcome, text).toBe("inconclusive");
    expect(result.outcome.verdict, text).not.toBe("passed");
  }
});

test("trace check judges span order and attributes", async ({ server, desk }) => {
  const reversed = await recordTrace(server, desk, ["draft-answer", "lookup-policy"], "refunds");
  const shipping = await recordTrace(server, desk, ["lookup-policy", "draft-answer"], "shipping");

  for (const [traceId, expected] of [
    [reversed, ["failed", "failed", "passed"]],
    [shipping, ["passed", "passed", "failed"]],
  ] as const) {
    const result = await reviewNow(desk, review("Refunds follow our policy.", 50, traceId));

    const judged = ["policy-looked-up", "answer-drafted-after-lookup", "lookup-read-refunds"].map(
      (id) => requirement(result, id).outcome,
    );
    expect(judged, traceId).toEqual(expected);
    expect(result.outcome.verdict).toBe("failed");
  }
});

test("invalid judge verdicts are inconclusive", async ({ desk }) => {
  for (const marker of ["JUDGE-MALFORMED", "JUDGE-FOREIGN-CITE"]) {
    const result = await reviewNow(desk, review(`${marker}: refunds follow our policy.`, 50));

    const judged = requirement(result, "follows-policy");
    expect(judged.outcome, `${marker}: ${JSON.stringify(judged)}`).toBe("inconclusive");
    expect(judged.reason, `${marker}: it says why`).not.toBe("");
    expect(result.outcome.verdict, marker).toBe("inconclusive");
  }
});

test("code check fails with citations and is inconclusive otherwise", async ({ desk }) => {
  const failed = await reviewNow(desk, review("Refunds are fine.", 50));

  const named = requirement(failed, "names-the-policy");
  expect([named.outcome, named.citations]).toEqual(["failed", ANSWER_TEXT]);
  expect(failed.outcome.verdict).toBe("failed");
  for (const text of ["CODE-MALFORMED", "CODE-FOREIGN-CITE", "CODE-FORBIDDEN", "CODE-SPIN"]) {
    const result = await reviewNow(desk, review(text, 50));

    const judged = requirement(result, "names-the-policy");
    expect(judged.outcome, `${text}: ${JSON.stringify(judged)}`).toBe("inconclusive");
    expect(judged.reason, `${text}: it says why`).not.toBe("");
    expect(result.outcome.verdict, text).not.toBe("passed");
  }
});

test("hosted tools need the tool and domain grants", async ({ server, provider, desk }) => {
  for (const [name, extra, allowed] of [
    ["all_tools", ["tool:invoke", "cards:read", "bifrost_query:read"], TOOLS],
    ["no_tool_grant", ["cards:read", "bifrost_query:read"], []],
    ["no_domain_grant", ["tool:invoke"], [TOOLS[2]]],
  ] as const) {
    const credential = server.scopedApiKey(name, [...VERIFIES, ...extra]);

    const result = await investigate(desk, WyrdClient.connect({ credential }), name);

    assertTools(provider, result, name, allowed);
  }
  const credential = server.scopedApiKey("card_tool_only", grants({ tool: TOOLS[0] }));
  const result = await investigate(desk, WyrdClient.connect({ credential }), "card_tool_only");
  assertTools(provider, result, "card_tool_only", [TOOLS[0]]);
});

test("delegated tools keep the narrower authority", async ({ server, provider, desk }) => {
  const subject = server.scopedApiKey("tool_subject", grants("all"));
  const actor = server.scopedApiKey("tool_delegate", grants({ tool: TOOLS[0] }));
  const token = await WyrdClient.connect({ credential: subject }).accessToken();
  const delegated = await WyrdClient.connect({ credential: actor }).onBehalfOf(token, { audience: "wyrd" });
  const principals = await Principals.connect();
  const [subjectId, actorId] = await Promise.all(
    ["tool_subject", "tool_delegate"].map(
      async (name) => (await principals.list({ name })).principals[0]?.principal_id,
    ),
  );

  const result = await investigate(desk, delegated, "delegated");

  assertTools(provider, result, "delegated", [TOOLS[0]]);
  server.flushBifrost();
  const attributed = await (await Bifrost.connect()).sql(
    "SELECT sub_principal_id, act_principal_id FROM vala.verification.results WHERE execution_id = $1",
    [result.execution_id],
    z.object({ sub_principal_id: z.string(), act_principal_id: z.string() }),
  );
  expect(attributed).toEqual([{ sub_principal_id: subjectId, act_principal_id: actorId }]);
});
