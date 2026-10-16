// Verifier building blocks, end to end: one `policy-review` Verifier runs
// every action kind (`extract`, `agent`, `code`) and every Run check kind
// (`assert`, `trace`, `agent`, `code`) over an answer, its refund policy PDF,
// and the trace that produced it; `document-formats` extracts each of the nine
// extractable formats into its locked output.
//
// The local upstream stands in for the model provider: the refund reader first
// calls `fs.read` on its workspace copy of the policy and then answers a
// 100-dollar limit, and the judge passes every answer with a citation unless
// the answer names a scripted case.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { BasicTracerProvider, SimpleSpanProcessor } from "@opentelemetry/sdk-trace-base";
import { expect, vi } from "vitest";

import {
  Cards,
  type CardTypes,
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

/** The policy every review reads: the support desk's checked-in refund policy. */
const POLICY = resolve(import.meta.dirname, "../../../../../examples/support-desk/service/refund-policy.pdf");

/**
 * The local model upstream. The refund reader's first turn calls `fs.read`
 * on the workspace policy and its turn with the tool result answers the
 * limit; the judge answers by the scripted case its claim names.
 */
function upstream(request: ReceivedRequest): Reply {
  const body = request.body;
  if (body.includes("REFUND RULES")) {
    if (/"role"\s*:\s*"tool"/.test(body)) {
      return judgeReply('{"max_refund":100}');
    }
    const call = { name: "fs.read", arguments: '{"path":"/inputs/policy.pdf"}' };
    const reply = judgeReply("");
    const message = {
      role: "assistant",
      content: null,
      tool_calls: [{ id: "call-read-policy", type: "function", function: call }],
    };
    return {
      ...reply,
      body: { ...(reply.body as object), choices: [{ index: 0, finish_reason: "tool_calls", message }] },
    };
  }
  for (const [marker, verdict] of [
    ["REFUSE-POLICY", BREAKS],
    ["UNCITED", UNCITED],
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

/** The registered `policy-desk` Service, its own client, and its hydrated state with Bifrost started. */
interface PolicyDesk {
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
    await use({ root, client, state });
    await state.shutdown();
  },
});

/** Verify `inputs` with `policy-review` now. */
function reviewNow(desk: PolicyDesk, inputs: VerifierInputs): Promise<VerificationResult> {
  return desk.state.run().observe.verify("policy-review", inputs);
}

/** Export one span named `name` as the Service, wait until its trace is stored, and return its trace ID. */
async function recordTrace(server: { flushBifrost(): void }, desk: PolicyDesk, name: string): Promise<string> {
  const provider = new BasicTracerProvider({ spanProcessors: [new SimpleSpanProcessor(spanExporter(desk.client))] });
  const span = provider.getTracer("journey.service").startSpan(name);
  span.end();
  await provider.shutdown();
  const traceId = span.spanContext().traceId;
  expect(await timeline(server, traceId), "the trace is stored").not.toEqual([]);
  return traceId;
}

test("policy review runs every action and check kind", async ({ server, provider, desk }) => {
  const trace = await recordTrace(server, desk, "lookup-policy");

  const result = await reviewNow(desk, review("Refunds follow our policy.", 50, trace));

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
    "follows-policy",
    "names-the-policy",
  ]) {
    expect(requirement(result, id).outcome, id).toBe("passed");
  }
  const cited = [{ slot: "answer", locator: { json_pointer: "/text" } }];
  expect(requirement(result, "follows-policy").citations).toEqual(cited);
  expect(requirement(result, "names-the-policy").citations).toEqual(cited);
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
  const guessed = await recordTrace(server, desk, "guess-policy");

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
  const refused = reviewNow(desk, {
    answer: { text: "Refunds follow our policy.", refund: 50 },
    policy: verify.File.bytes(new Uint8Array(65_537).fill(0x25), { format: "pdf" }),
  });

  await expect(refused).rejects.toMatchObject({ code: "WYRD_VERIFICATION_413_INPUT_TOO_LARGE" });
  expect(provider.received, "nothing ran").toEqual([]);
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
      document: verify.File.path(fixture(`cards/verifier_building_blocks/evidence/${name}`)),
    });

    expect(step(result, "read").output, name).toEqual(expected);
    expect(requirement(result, "extracted").outcome, name).toBe("passed");
  }
});
