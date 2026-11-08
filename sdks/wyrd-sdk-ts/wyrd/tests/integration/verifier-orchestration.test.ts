// Verifier orchestration, end to end: `orchestration-desk`'s
// `specialist-review` selects specialists by a planner's decisions and
// consolidates every settled collection honestly, its quoter requires the
// security account by name, `specialists-only` judges nothing when no
// specialist is selected, and a held execution cancels.
//
// The local upstream stands in for the model provider and scripts each Agent
// by its prompt marker and the number of assistant turns already in its
// conversation. The test then inspects what each conversation received.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import {
  Cards,
  type CardTypes,
  type RequirementResult,
  verify,
  type VerificationResult,
  WyrdClient,
  WyrdState,
} from "wyrd-ai";

import { judgeReply, toolCall, toolTurn } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

/** How long a test waits for a held request to arrive, and how long the held planner holds its reply. */
const WAIT_MS = 90_000;

/** One Chat Completions message as the upstream receives it. */
type Message = {
  readonly role?: string;
  readonly content?: unknown;
  readonly tool_call_id?: string;
};

/** The fields of an `fs.read` result the test reads. */
type ToolResult = { readonly units?: ReadonlyArray<{ readonly text?: string }> };

/** The messages `request` carries. */
function messages(request: ReceivedRequest): readonly Message[] {
  return (JSON.parse(request.body) as { messages?: readonly Message[] }).messages ?? [];
}

/** The text of every message `request` carries, joined. */
function text(request: ReceivedRequest): string {
  return messages(request)
    .map((message) => (typeof message.content === "string" ? message.content : ""))
    .join("\n");
}

/** Every tool result `request` carries, by the provider call ID it answers, parsed from its JSON content. */
function results(request: ReceivedRequest): Record<string, ToolResult> {
  const found: Record<string, ToolResult> = {};
  for (const message of messages(request)) {
    if (message.role === "tool" && message.tool_call_id !== undefined && typeof message.content === "string") {
      found[message.tool_call_id] = JSON.parse(message.content) as ToolResult;
    }
  }
  return found;
}

/** The text of an `fs.read` result: its units' lines, joined by newlines. */
function lines(read: ToolResult | undefined): string {
  return (read?.units ?? []).map((unit) => unit.text ?? "").join("\n");
}

/** The final `{"done": true}` answer. */
const DONE = judgeReply('{"done":true}');

/** A reply the Agent runtime cannot decode. */
const GARBLED: Reply = { status: 200, body: { garbled: true } };

/** The planner's decision for each question marker; a question with none gets {@link GARBLED}. */
const DECISIONS: ReadonlyArray<readonly [string, unknown]> = [
  ["NEITHER", { security: false, style: false }],
  ["SECURITY-ONLY", { security: true, style: false }],
  ["BOTH", { security: true, style: true }],
  ["UNDECIDED", { security: null, style: false }],
  ["FORGOTTEN", { style: false }],
];

/** The prompt text of every request that reached the upstream, recorded on arrival, before any held reply is sent. */
const arrived: string[] = [];

/**
 * The scripted model: one reply per request, chosen by the Agent's prompt
 * marker and turn. The consolidator reads every collection's manifest; the
 * planner decides by the question's marker, holds a `HELD` question's reply,
 * and breaks without one; the generalist and the security specialist publish
 * an account; the stylist publishes a partial draft, then breaks; the quoter
 * reads the security account.
 */
function upstream(request: ReceivedRequest): Reply {
  const prompt = text(request);
  arrived.push(prompt);
  const turn = messages(request).filter((message) => message.role === "assistant").length;
  if (prompt.includes("CONSOLIDATE:")) {
    const read = (collection: string): unknown =>
      toolCall(`c-${collection}`, "fs.read", { path: `/inputs/${collection}/manifest.json` });
    return turn === 0 ? toolTurn(["general", "security", "style"].map(read)) : DONE;
  }
  if (prompt.includes("PLAN:")) {
    if (prompt.includes("HELD")) {
      return { ...judgeReply(JSON.stringify({ security: true, style: true })), delayMs: WAIT_MS };
    }
    const decision = DECISIONS.find(([marker]) => prompt.includes(marker));
    return decision === undefined ? GARBLED : judgeReply(JSON.stringify(decision[1]));
  }
  if (prompt.includes("GENERAL:") || prompt.includes("SECURITY:")) {
    const focus = prompt.includes("GENERAL:") ? "general" : "security";
    return turn === 0 ? toolTurn([toolCall("s-publish", "publish", { name: "account", value: { focus } })]) : DONE;
  }
  if (prompt.includes("STYLE:")) {
    return turn === 0
      ? toolTurn([toolCall("s-draft", "publish", { name: "draft", value: { partial: true } })])
      : GARBLED;
  }
  if (prompt.includes("QUOTE:")) {
    return turn === 0 ? toolTurn([toolCall("q-read", "fs.read", { path: "/inputs/security.json" })]) : DONE;
  }
  return { status: 500 };
}

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

/** The grants of a principal that verifies, invokes the gateway, and may call every tool. */
const EVERY_TOOL = [
  { resource: "verifier", action: "run", scope: "all" },
  { resource: "gateway", action: "invoke", scope: "all" },
  { resource: "tool", action: "invoke", scope: "all" },
  { resource: "cards", action: "read", scope: "all" },
  { resource: "bifrost_query", action: "read", scope: "all" },
];

/**
 * `test` on a verification-capable server whose gateway deploys `gpt-4o` on
 * the scripted upstream, with `orchestration-desk` declared and hydrated as a
 * principal granted every tool.
 */
const test = serverTest({ provider: upstream, verificationRuntime: true }).extend<{ state: WyrdState }>({
  state: async ({ server }, use) => {
    await configureGateway(WyrdClient.connect());
    const cards = await Cards.connect();
    const receipt = await cards.declareFromPath(fixture("cards/verifier_orchestration/orchestration-desk.yaml"));
    const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-orchestration-desk-"));
    await cards.pull(receipt.root, bundle);
    const credential = server.scopedApiKey("every_tool", EVERY_TOOL);
    await use(WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential }) }));
  },
});

/** One `specialist-review` question's expected observations. */
type Case = {
  /** The question, carrying the planner's marker. */
  readonly question: string;
  /** The statuses of `plan`, `security`, `style`, and `quote`. */
  readonly statuses: readonly [string, string, string, string];
  /** The markers of the Agents that called the model. */
  readonly called: readonly string[];
  /** The `security` and `style` manifests' status and entry names. */
  readonly collections: readonly [readonly [string, readonly string[]], readonly [string, readonly string[]]];
  /** What the security step's failure reason names, when it failed on the decision. */
  readonly securityReason?: string;
};

/**
 * The planner selects specialists while the generalist reviews. A selected
 * specialist runs; an unselected one is skipped without calling the model; a
 * `null`, missing, or undecodable decision fails the condition rather than
 * skipping it. The consolidator always runs and reads every collection's
 * manifest with its settled status, a failed stylist's partial draft
 * included. The quoter, which requires the security account by name, runs
 * only when security published it and is skipped otherwise. A Verifier
 * judging only an unselected specialist is inconclusive, not passed. A held
 * execution cancels through the SDK and reports `cancelled`.
 */
test("conditions_and_collections_preserve_missing_evidence", async ({ provider, state }) => {
  const cases: readonly Case[] = [
    {
      question: "NEITHER",
      statuses: ["succeeded", "skipped", "skipped", "skipped"],
      called: ["GENERAL:", "CONSOLIDATE:"],
      collections: [
        ["skipped", []],
        ["skipped", []],
      ],
    },
    {
      question: "SECURITY-ONLY",
      statuses: ["succeeded", "succeeded", "skipped", "succeeded"],
      called: ["GENERAL:", "SECURITY:", "CONSOLIDATE:", "QUOTE:"],
      collections: [
        ["succeeded", ["account"]],
        ["skipped", []],
      ],
    },
    {
      question: "BOTH",
      statuses: ["succeeded", "succeeded", "failed", "succeeded"],
      called: ["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"],
      collections: [
        ["succeeded", ["account"]],
        ["failed", ["draft"]],
      ],
    },
    {
      question: "UNDECIDED",
      statuses: ["succeeded", "failed", "skipped", "skipped"],
      called: ["GENERAL:", "CONSOLIDATE:"],
      collections: [
        ["failed", []],
        ["skipped", []],
      ],
      securityReason: "kind (null, bool)",
    },
    {
      question: "FORGOTTEN",
      statuses: ["succeeded", "failed", "skipped", "skipped"],
      called: ["GENERAL:", "CONSOLIDATE:"],
      collections: [
        ["failed", []],
        ["skipped", []],
      ],
      securityReason: "security",
    },
    {
      question: "UNPLANNED",
      statuses: ["failed", "failed", "failed", "skipped"],
      called: ["GENERAL:", "CONSOLIDATE:"],
      collections: [
        ["failed", []],
        ["failed", []],
      ],
    },
  ];
  const asked = (marker: string, question: string): ReceivedRequest[] =>
    provider.received.filter((request) => {
      const prompt = text(request);
      return prompt.includes(marker) && prompt.includes(question);
    });

  for (const { question, statuses, called, collections, securityReason } of cases) {
    const result = await state.run().observe.verify("specialist-review", question);

    expect(
      ["plan", "security", "style", "quote"].map((id) => step(result, id).status),
      `${question}: ${JSON.stringify(result)}`,
    ).toEqual(statuses);
    expect(step(result, "general").status, question).toBe("succeeded");
    expect(
      requirement(result, "consolidated").outcome,
      `${question}: consolidation reads failed and skipped collections`,
    ).toBe("passed");
    for (const marker of ["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"]) {
      expect(asked(marker, question).length > 0, `${question}: ${marker}`).toBe(called.includes(marker));
    }
    if (securityReason !== undefined) {
      const security = step(result, "security");
      expect(security.reason ?? "", `${question}: ${JSON.stringify(security)}`).toContain(securityReason);
    }
    const consolidator = asked("CONSOLIDATE:", question).at(-1);
    if (consolidator === undefined) {
      throw new Error(`${question}: the consolidator called the model`);
    }
    const read = results(consolidator);
    for (const [index, collection] of (["security", "style"] as const).entries()) {
      const manifest = JSON.parse(lines(read[`c-${collection}`])) as {
        status?: string;
        entries?: ReadonlyArray<{ name?: string }>;
      };
      expect(
        [manifest.status, (manifest.entries ?? []).map((entry) => entry.name)],
        `${question}: ${collection} ${JSON.stringify(manifest)}`,
      ).toEqual(collections[index]);
    }
  }

  const unjudged = await state.run().observe.verify("specialists-only", "NEITHER-ONLY");
  expect([
    step(unjudged, "security").status,
    requirement(unjudged, "secured").outcome,
    unjudged.outcome.verdict,
  ]).toEqual(["skipped", "not_applicable", "inconclusive"]);

  const submitted = await state.run().observe.submit("specialist-review", "HELD");
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await expect
    .poll(() => arrived.some((prompt) => prompt.includes("PLAN:") && prompt.includes("HELD")), {
      timeout: WAIT_MS,
      interval: 50,
    })
    .toBe(true);
  const execution = await verify.Execution.connect(submitted.execution.id);
  expect((await execution.invocations()).status, "the held planner keeps the execution running").toBe("running");
  for (let repeat = 0; repeat < 2; repeat += 1) {
    expect(await execution.cancel()).toEqual({ execution_id: submitted.execution.id, status: "cancelled" });
  }
  expect((await execution.status()).status).toBe("cancelled");
});
