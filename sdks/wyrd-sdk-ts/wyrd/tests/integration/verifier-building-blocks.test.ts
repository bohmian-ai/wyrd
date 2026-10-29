// Verifier building blocks, end to end: one `policy-review` Verifier runs
// every action kind (`extract`, `agent`, `code`) and every Run check kind
// (`assert`, `trace`, `agent`, `code`) over an answer, its refund policy PDF,
// and the trace that produced it; `document-formats` extracts each of the nine
// extractable formats into its locked output; `scored-review` returns custom
// Verdict data from plain, expanded, Agent, code, and step checks; `whole-trace`
// judges every span of a trace, whichever workload wrote it.
//
// The local upstream stands in for the model provider: the refund reader first
// calls `fs.extract` on its workspace copy of the policy and then answers a
// 100-dollar limit, the judge passes every answer with a citation unless the
// answer names a scripted case, and the `tool-review` investigator calls the
// Card, Bifrost, and code tools at once and then finishes. Scripted answer
// texts also fail the reader's provider, schema, and deadline and the code step
// and check; scoped and delegated principals exercise every tool grant.
// `handbook-desk`'s judges mount an uploaded, extracted Artifact and cite the
// line `fs.read` returned.
import { createHash, randomBytes } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { context, trace, TraceFlags } from "@opentelemetry/api";
import { BasicTracerProvider, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";
import { z } from "zod";

import {
  Bifrost,
  Cards,
  type CardTypes,
  Principals,
  type RequirementResult,
  verify,
  type VerificationResult,
  type VerifierInputs,
  WyrdError,
  WyrdClient,
  WyrdState,
} from "wyrd-ai";
import { spanExporter } from "wyrd-ai/otel";

import { judgeReply, toolCall, toolTurn } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import {
  type DeclaredRef,
  fixture,
  declared,
  rooted,
  runtimeAccount,
  serverTest,
  timeline,
} from "../support/server.js";

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
const TOOLS = ["cards.get", "bifrost.query", "code.run"] as const;

/** The stable code of a permission denial, carried by every denied tool call's error. */
const DENIED = "WYRD_PERMISSION_403_DENIED_RBAC";

/** The permissions every `tool-review` principal holds: run any Verifier and invoke the gateway its investigator calls the model through. */
const VERIFIES = ["verifier:run", "gateway:invoke"];

/** The investigator's one tool turn: one call of each of {@link TOOLS}, in order. */
const INVESTIGATION = [
  toolCall("call-card", TOOLS[0], {
    card_ref: { kind: "Service", name: "policy-desk", version: "1.0.0" },
  }),
  toolCall("call-query", TOOLS[1], {
    sql: "SELECT execution_id FROM bifrost.verification.results LIMIT 1",
    max_rows: 1,
  }),
  toolCall("call-code", TOOLS[2], { source: "6 * 7" }),
];

/** The `scored-judge`'s custom data for an answer naming no scripted case. */
const SCORED = { score: 0.9, category: "refund" };

/** The policy every review reads: the support desk's checked-in refund policy. */
const POLICY = resolve(import.meta.dirname, "../../../../../examples/support-desk/service/refund-policy.pdf");

/**
 * The local model upstream. The refund reader fails by the scripted failure
 * its answer names; otherwise its first turn calls `fs.extract` on the workspace
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
    return toolTurn([
      toolCall("call-read-policy", "fs.extract", { path: "/inputs/policy.pdf", output_path: "/work/policy.json" }),
    ]);
  }
  if (body.includes("HANDBOOK JUDGE")) {
    return toolResults ? judgeReply(handbookVerdict(body)) : toolTurn(READ_HANDBOOK);
  }
  if (body.includes("INVESTIGATE")) {
    return toolResults ? judgeReply('{"done":true}') : toolTurn(INVESTIGATION);
  }
  if (body.includes("SCORE:")) {
    const data = body.includes("BAD-DATA")
      ? { score: "high", category: "refund" }
      : body.includes("HUGE-DATA")
        ? { score: 0.9, category: "x".repeat(70_000) }
        : SCORED;
    return judgeReply(JSON.stringify({ outcome: "passed", reason: "scored", citations: ANSWER_TEXT, data }));
  }
  if (body.includes("Judge whether")) {
    return judgeReply(FOLLOWS);
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

/** The handbook judge's first turn: read its mounted handbook. */
const READ_HANDBOOK = [toolCall("call-handbook", "fs.read", { path: "/inputs/handbook.json" })];

/**
 * The handbook judge's Verdict once `body` carries its `fs.read` result: a
 * pass citing the one read line holding a `MARKER-` on the slot the read
 * named. A failed read, a wrong call ID or path, or no single marked line
 * answers no Verdict.
 */
function handbookVerdict(body: string): string {
  const messages = (JSON.parse(body) as { messages: { role: string; content: string; tool_call_id?: string }[] })
    .messages;
  const read = messages.find((message) => message.role === "tool");
  let output: { path?: string; sources?: string[]; units?: { text: string; locator: unknown }[] };
  try {
    output = JSON.parse(read?.content ?? "") as typeof output;
  } catch {
    return "not a verdict";
  }
  const marked = (output.units ?? []).filter((unit) => /MARKER-[0-9a-f]{16}/.test(unit.text));
  const [unit] = marked;
  if (
    read?.tool_call_id !== "call-handbook" ||
    output.path !== "/inputs/handbook.json" ||
    marked.length !== 1 ||
    unit === undefined
  ) {
    return "not a verdict";
  }
  const marker = /MARKER-[0-9a-f]{16}/.exec(unit.text)?.[0];
  return JSON.stringify({
    outcome: "passed",
    reason: `the handbook names ${marker}`,
    citations: [{ slot: output.sources?.[0], locator: unit.locator }],
  });
}

/** The `policy-review` inputs: an answer with `text` offering `refund` dollars, the policy, and its trace when there is one. */
function review(text: string, refund: number, trace?: string): VerifierInputs {
  return { answer: { text, refund }, policy: verify.File.path(POLICY), ...(trace === undefined ? {} : { trace }) };
}

/** The citation of the answer's text, which every cited Verdict of the story names. */
const ANSWER_TEXT = [{ slot: "answer", locator: { json_pointer: "/text" } }];

/** The requirement `id` of `result`. */
function requirement<T>(result: VerificationResult<T>, id: string): RequirementResult<T> {
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

/** The declared `policy-desk` Service, its Card bundle, its own client, and its hydrated state with Bifrost started. */
interface PolicyDesk {
  readonly bundle: string;
  readonly root: DeclaredRef;
  readonly client: WyrdClient;
  readonly state: WyrdState;
}

/**
 * `test` on a fresh verification-capable server per test whose gateway
 * deploys `gpt-4o` on the local upstream, with `policy-desk` declared and
 * hydrated and acting as its runtime account.
 */
const test = serverTest({ provider: upstream, verificationRuntime: true, perTest: true }).extend<{
  desk: PolicyDesk;
}>({
  desk: async ({ server: _ }, use) => {
    await configureGateway(WyrdClient.connect());
    const cards = await Cards.connect();
    const receipt = await cards.declareFromPath(fixture("cards/verifier_building_blocks/policy-desk.yaml"));
    const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-policy-desk-"));
    await cards.pull(receipt.root, bundle);
    const root = declared(receipt);
    const { key } = await runtimeAccount({ kind: "Service", name: root.name });
    const client = WyrdClient.connect({ credential: key, workloadUid: root.uid });
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
 * is the trace document's `spans[0]` and `inner` its `spans[1]`; the SDK's
 * default start time has millisecond precision, so `inner` starts 1ms later.
 */
async function recordTrace(
  server: { flushBifrost(): void },
  desk: PolicyDesk,
  [outer, inner]: readonly [string, string],
  policyId: string,
): Promise<string> {
  const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(spanExporter(desk.client))] });
  const tracer = provider.getTracer("journey.service");
  const start = Date.now();
  const span = tracer.startSpan(outer, { attributes: { policy_id: policyId }, startTime: new Date(start) });
  tracer.startSpan(inner, { startTime: new Date(start + 1) }, trace.setSpan(context.active(), span)).end();
  span.end();
  await provider.shutdown();
  const traceId = span.spanContext().traceId;
  expect(await timeline(server, traceId), "both spans are stored").toHaveLength(2);
  return traceId;
}

/**
 * Export the Service's `retrieve` span with an untagged writer's `score` span
 * started 1ms later inside it, wait until both are stored, and return the trace
 * ID. Given `traceId`, both spans join that trace.
 */
async function recordWholeTrace(server: { flushBifrost(): void }, desk: PolicyDesk, traceId?: string): Promise<string> {
  const exporting = (client: WyrdClient) =>
    new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(spanExporter(client))] });
  const [own, untagged] = [exporting(desk.client), exporting(WyrdClient.connect())];
  const parent =
    traceId === undefined
      ? context.active()
      : trace.setSpanContext(context.active(), {
          traceId,
          spanId: randomBytes(8).toString("hex"),
          traceFlags: TraceFlags.SAMPLED,
          isRemote: true,
        });
  const start = Date.now();
  const span = own.getTracer("journey.desk").startSpan("retrieve", { startTime: new Date(start) }, parent);
  untagged
    .getTracer("journey.scorer")
    .startSpan("score", { startTime: new Date(start + 1) }, trace.setSpan(parent, span))
    .end();
  span.end();
  await Promise.all([own.shutdown(), untagged.shutdown()]);
  const traced = span.spanContext().traceId;
  expect(await timeline(server, traced), "both spans are stored").toHaveLength(2);
  return traced;
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
  expect(rules.tool_calls?.map((call) => call.tool)).toEqual(["fs.extract"]);
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

test("trace evidence keeps whole trace and waits when submitted", async ({ server, desk }) => {
  const whole = await recordWholeTrace(server, desk);
  expect((await desk.state.run().observe.verify("whole-trace", { trace: whole })).outcome.verdict).toBe("passed");

  const absent = await desk.state.run().observe.verify("whole-trace", { trace: randomBytes(16).toString("hex") });
  const [unlanded] = absent.outcome.requirements;
  expect(unlanded?.outcome).toBe("inconclusive");
  expect(unlanded?.reason).toContain("not yet landed");

  const late = randomBytes(16).toString("hex");
  const submitted = await desk.state.run().observe.submit("whole-trace", { trace: late });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  const { execution } = submitted;
  const status = () => execution.status().catch((error: WyrdError) => ({ status: error.code, error: undefined }));
  await expect.poll(async () => (await status()).status, { timeout: WAIT_MS, interval: 100 }).toBe("retrying");
  expect((await execution.status()).error?.code).toBe("evidence_unavailable");
  await recordWholeTrace(server, desk, late);

  const result = await execution.wait({ timeoutMs: WAIT_MS });
  expect(result.outcome.verdict).toBe("passed");
  expect((await execution.status()).result).toEqual(result);
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

test("file inputs use shared uploads", async ({ server, desk }) => {
  const path = fixture("cards/verifier_building_blocks/samples/sample.csv");
  const inMemory = new TextEncoder().encode("a,b\n3,4");
  const files: ReadonlyArray<readonly [ReturnType<typeof verify.File.path>, unknown]> = [
    [
      verify.File.path(path),
      [
        ["a", "b"],
        ["1", "2"],
      ],
    ],
    [
      verify.File.bytes(inMemory, { format: "csv" }),
      [
        ["a", "b"],
        ["3", "4"],
      ],
    ],
  ];
  for (const [document, rows] of files) {
    const result = await desk.state.run().observe.verify("document-formats", { document });
    expect(step(result, "read").output).toEqual({ format: "csv", rows });
  }

  const token = await WyrdClient.connect().accessToken();
  for (const content of [readFileSync(path), inMemory]) {
    const response = await fetch(`${server.baseUrl}/v1/verification/evidence`, {
      method: "POST",
      headers: { "x-wyrd-access-token": `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({
        files: [{ sha256: createHash("sha256").update(content).digest("hex"), size_bytes: content.length }],
      }),
    });
    expect(response.status).toBe(200);
    expect(((await response.json()) as { uploads: unknown[] }).uploads).toEqual([]);
  }
});

test("stored evidence readback uses canonical records", async ({ desk }) => {
  const content = new TextEncoder().encode("a,b\n5,6");
  const submitted = await desk.state
    .run()
    .observe.submit("document-formats", { document: verify.File.bytes(content, { format: "csv" }) });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }

  const result = await submitted.execution.wait({ timeoutMs: WAIT_MS });
  const status = await submitted.execution.status();

  expect(status.result).toEqual(result);
  expect(result.outcome.evidence).toEqual({
    document: {
      kind: "file",
      sha256: createHash("sha256").update(content).digest("hex"),
      size_bytes: content.length,
      format: "csv",
    },
  });
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
    ["no_domain_grant", ["tool:invoke", "cards:read"], [TOOLS[0], TOOLS[2]]],
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
  const delegated = await (await rooted(actor, "policy-desk")).onBehalfOf(token, { audience: "wyrd" });
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
    "SELECT sub_principal_id, act_principal_id FROM bifrost.verification.results WHERE execution_id = $1",
    [result.execution_id],
    z.object({ sub_principal_id: z.string(), act_principal_id: z.string() }),
  );
  expect(attributed).toEqual([{ sub_principal_id: subjectId, act_principal_id: actorId }]);
});

/** The custom data of `scored-review`'s scored checks: a score with its category, or the step check's word count. */
const Scored = z.union([
  z.object({ score: z.number(), category: z.string() }).strict(),
  z.object({ words: z.number().int() }).strict(),
]);

/** Submit `text` to `scored-review` and return its queued execution. */
async function submitScored(desk: PolicyDesk, text: string): Promise<verify.Execution> {
  const submitted = await desk.state.run().observe.submit("scored-review", { text }, { positional: true });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  return submitted.execution;
}

/**
 * Assert `result` carries `scored-review`'s readback for a four-word answer:
 * the plain `verdict` and `{kind: verdict}` judges pass with no data, and the
 * scored Agent, code, and step checks keep their exact custom data, failed
 * requirements included.
 */
function assertScored(result: VerificationResult): void {
  for (const id of ["polite", "on-topic"]) {
    const plain = requirement(result, id);
    expect(plain.outcome, id).toBe("passed");
    expect(plain.citations, id).toEqual(ANSWER_TEXT);
    expect(plain.data, id).toBeUndefined();
  }
  for (const [id, outcome, data] of [
    ["helpful", "passed", SCORED],
    ["well-formed", "failed", { score: 0.5, category: "format" }],
    ["tally-counted", "passed", { words: 4 }],
  ] as const) {
    const scored = requirement(result, id);
    expect(scored.outcome, id).toBe(outcome);
    expect(scored.citations, id).toEqual(ANSWER_TEXT);
    expect(scored.data, id).toEqual(data);
  }
  expect(requirement(result, "tally-counted").step).toBe("tally");
}

test("custom_verdict_data_survives_readback", async ({ server, desk }) => {
  const text = "Refunds follow our policy.";

  assertScored(await desk.state.run().observe.verify("scored-review", { text }, { positional: true }));
  const execution = await submitScored(desk, text);
  const waited = await execution.wait({ timeoutMs: WAIT_MS });
  assertScored(waited);
  expect((await execution.status()).result).toEqual(waited);

  const retained = async (executionId: string): Promise<unknown> => {
    server.flushBifrost();
    const rows = await (await Bifrost.connect()).sql(
      "SELECT to_json(output) AS output FROM bifrost.verification.execution_outputs WHERE execution_id = $1",
      [executionId],
      z.object({ output: z.string() }),
    );
    expect(rows).toHaveLength(1);
    return JSON.parse(rows[0]?.output ?? "null").requirements;
  };
  expect(await retained(waited.execution_id), "the retained output keeps its data").toEqual(
    waited.outcome.requirements,
  );

  for (const [marker, data] of Object.entries(EXACT_DATA)) {
    const text = `${marker} refunds`;
    const direct = await desk.state.run().observe.verify("scored-review", { text }, { positional: true });
    expect(requirement(direct, "well-formed").data, `${marker}: direct`).toEqual(data);
    const execution = await submitScored(desk, text);
    const exact = await execution.wait({ timeoutMs: WAIT_MS });
    expect(requirement(exact, "well-formed").data, `${marker}: wait`).toEqual(data);
    expect((await execution.status()).result, `${marker}: status`).toEqual(exact);
    expect(await retained(exact.execution_id), `${marker}: retained`).toEqual(exact.outcome.requirements);
  }
});

/** A `tree` that, inside its data object, nests `containers` containers. */
function deep(containers: number): unknown {
  let tree: unknown = "s3cr3t";
  for (let level = 1; level < containers; level += 1) {
    tree = [tree];
  }
  return tree;
}

/** The `well-formed` code check's data at its inclusive bounds, by answer marker. */
const EXACT_DATA: Record<string, unknown> = {
  "EXACT-BYTES": { score: 0.5, category: `${"é".repeat(32_000)}\n"\u0001${"x".repeat(1_499)}` },
  "EXACT-WIDE": { score: 0.5, category: "format", tree: Array<number>(32_747).fill(0) },
  "DEEP-16": { score: 0.5, category: "format", tree: deep(16) },
};

test("invalid_custom_verdict_data_is_inconclusive", async ({ desk }) => {
  for (const marker of ["BAD-DATA", "HUGE-DATA"]) {
    const result = await desk.state
      .run()
      .observe.verify("scored-review", { text: `${marker} refunds` }, { positional: true });
    for (const id of ["helpful", "well-formed"]) {
      const refused = requirement(result, id);
      expect(refused.outcome, `${marker} ${id}`).toBe("inconclusive");
      expect(refused.data, `${marker} ${id}`).toBeUndefined();
      expect(refused.reason, `${marker} ${id}`).not.toMatch(/high|xxxx/);
    }
    expect(requirement(result, "tally-counted").data, `${marker}: valid data elsewhere is kept`).toEqual({ words: 2 });
    expect(result.outcome.verdict).toBe("inconclusive");
  }

  for (const marker of [
    "MISSING-DATA",
    "NULL-DATA",
    "EXTRA-DATA",
    "EXTRA-FIELD",
    "OVER-BYTES",
    "OVER-WIDE",
    "DEEP-17",
  ]) {
    const result = await desk.state
      .run()
      .observe.verify("scored-review", { text: `${marker} refunds` }, { positional: true });
    const refused = requirement(result, "well-formed");
    expect(refused.outcome, marker).toBe("inconclusive");
    expect(refused.data, marker).toBeUndefined();
    expect(refused.reason, marker).not.toMatch(/s3cr3t|xxxx/);
    expect(requirement(result, "helpful").data, `${marker}: the Agent's valid data is kept`).toEqual(SCORED);
    expect(requirement(result, "tally-counted").data, `${marker}: valid data elsewhere is kept`).toEqual({ words: 2 });
  }

  const result = await desk.state
    .run()
    .observe.verify("scored-review", { text: "ENVELOPE-DATA refunds" }, { positional: true });
  const refused = requirement(result, "plainly-checked");
  expect(refused.outcome).toBe("inconclusive");
  expect(refused.data).toBeUndefined();
  expect(refused.reason).not.toMatch(/s3cr3t/);
  expect(requirement(result, "well-formed").data, "a declared sibling keeps its data").toEqual({
    score: 0.5,
    category: "format",
  });
});

test("custom_verdict_data_is_typed_on_return", async ({ desk }) => {
  const text = "Refunds follow our policy.";
  const expectTyped = (result: VerificationResult<z.infer<typeof Scored>>): void => {
    expect(requirement(result, "polite").data).toBeUndefined();
    expect(requirement(result, "helpful").data).toEqual(SCORED);
    expect(requirement(result, "well-formed").data).toEqual({ score: 0.5, category: "format" });
    expect(requirement(result, "tally-counted").data).toEqual({ words: 4 });
  };

  expectTyped(await desk.state.run().observe.verifyTyped("scored-review", { text }, Scored, { positional: true }));
  const execution = await submitScored(desk, text);
  expectTyped(await execution.waitTyped({ timeoutMs: WAIT_MS }, Scored));
  const status = await execution.statusTyped(Scored);
  if (status.result == null) {
    throw new Error("a settled result");
  }
  expectTyped(status.result);
});

test("typed_verdict_data_mismatch_keeps_original_result", async ({ provider, desk }) => {
  const text = "Refunds follow our policy.";
  const numbersOnly = z.record(z.string(), z.number());
  const mismatch = async (read: Promise<unknown>): Promise<void> => {
    const error = await read.then(
      () => undefined,
      (thrown: unknown) => thrown,
    );
    expect(error).toBeInstanceOf(WyrdError);
    const refused = error as WyrdError;
    expect(refused.code).toBe("WYRD_SDK_422_VERIFICATION_DATA_INVALID");
    expect(refused.details).toEqual({ requirement: "helpful", path: "" });
    expect(refused.message).not.toContain("refund");
  };
  const scoredCalls = (): number => provider.received.filter((request) => request.body.includes("SCORE:")).length;

  await mismatch(desk.state.run().observe.verifyTyped("scored-review", { text }, numbersOnly, { positional: true }));
  const execution = await submitScored(desk, text);
  const ordinary = await execution.wait({ timeoutMs: WAIT_MS });
  const judged = scoredCalls();
  await mismatch(execution.waitTyped({ timeoutMs: WAIT_MS }, numbersOnly));
  await mismatch(execution.statusTyped(numbersOnly));
  expect((await execution.status()).result, "the stored result is unchanged").toEqual(ordinary);
  expect(scoredCalls(), "nothing is judged again").toBe(judged);
});

test("uploaded artifacts are reused by service judges", async ({ server, provider }) => {
  const marker = `MARKER-${randomBytes(8).toString("hex")}`;
  const source = mkdtempSync(join(tmpdir(), "wyrd-ts-handbook-"));
  writeFileSync(join(source, "handbook.md"), `# Staff handbook\nOutages escalate with code ${marker}.\n`);
  writeFileSync(join(source, "rota.txt"), "never mounted\n");
  await configureGateway(WyrdClient.connect());
  const cards = await Cards.connect();
  const handbook = await cards.uploadArtifact(source, {
    metadata: { name: "staff-handbook", version: "1.0.0" },
    extract: true,
  });
  const files = await cards.listArtifacts(handbook);
  expect(files.artifacts.map((file) => file.relative_path)).toContain("extracted/handbook.md.json");
  const receipt = await cards.declareFromPath(fixture("cards/uploaded_artifacts/handbook-desk.yaml"));
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-handbook-desk-"));
  await cards.pull(receipt.root, bundle);
  const credential = server.scopedApiKey("handbook_judge", grants({ tool: "fs.read" }));
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential }) });
  const asked = provider.received.length;

  for (const verifier of ["handbook-review", "handbook-audit"]) {
    const result = await state.run().observe.verify(verifier, "How do outages escalate?");

    expect(result.outcome.verdict, JSON.stringify(result)).toBe("passed");
    const judged = requirement(result, "handbook-answers");
    expect(judged.reason).toContain(marker);
    const [citation] = judged.citations ?? [];
    expect(citation?.slot).toMatch(/^attachment_/);
    expect(citation?.locator).toHaveProperty("lines");
  }
  const judges = provider.received.slice(asked).filter((request) => request.body.includes("HANDBOOK JUDGE"));
  expect(judges).toHaveLength(4);
  for (const request of judges) {
    const body = JSON.parse(request.body) as { tools: { function: { name: string } }[]; messages: unknown[] };
    expect(body.tools.map((tool) => tool.function.name)).toEqual(["fs_read"]);
    expect(JSON.stringify(body.messages[0])).not.toContain(marker);
    expect(request.body).not.toContain("rota");
  }
});
