// Verifier orchestration, end to end: `orchestration-desk`'s
// `specialist-review` selects specialists by a planner's decisions and
// consolidates every settled collection honestly, its quoter requires the
// security account by name, `specialists-only` judges nothing when no
// specialist is selected, and a held execution cancels. `speculative-review`
// admits, discards, or blocks a speculative extra review by its comparison,
// and `document-pipeline` maps an uploaded document's arrays, repeats each
// item and a summary, and escalates conditionally.
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
  type PublicationSummary,
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

/** How long a held reply waits, so the other side of a race settles first. */
const HELD_MS = 3_000;

/** How long an early decision waits, so the bootstrap it races settles first while a held extra review still runs. */
const BRIEF_MS = 1_000;

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

/** The iteration index a prompt was sent for, from its `index=` marker. */
function iteration(prompt: string): number {
  return Number(prompt.split("index=")[1]?.split(/\s/)[0]);
}

/** The entries of the collection manifest an `fs.read` `read` returned, as `[item_index, iteration_index, name]`, sorted. */
function entries(read: ToolResult | undefined): unknown[] {
  const manifest = JSON.parse(lines(read)) as {
    entries?: ReadonlyArray<{ item_index?: number | null; iteration_index?: number | null; name?: string }>;
  };
  return (manifest.entries ?? [])
    .map((entry) => [entry.item_index ?? null, entry.iteration_index ?? null, entry.name])
    .sort((left, right) => JSON.stringify(left).localeCompare(JSON.stringify(right)));
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
 * reads the security account. A `SPEC-` consolidator reads only the extra
 * collection; a `LATE-DECISION` comparison and a `KNOWN` bootstrap hold their
 * replies; an `EARLY-DECISION` comparison waits briefly while its extra
 * review holds its first turn longer. A section reviewer publishes its
 * section and index; the polisher scores each round by its index; the
 * escalation publishes a note, then breaks; the summarizer reads both
 * collections, publishes a summary, and scores it by its iteration index; the
 * document judge passes, citing the document's title.
 */
function upstream(request: ReceivedRequest): Reply {
  const prompt = text(request);
  arrived.push(prompt);
  const turn = messages(request).filter((message) => message.role === "assistant").length;
  if (prompt.includes("CONSOLIDATE:")) {
    const read = (collection: string): unknown =>
      toolCall(`c-${collection}`, "fs.read", { path: `/inputs/${collection}/manifest.json` });
    const collections = prompt.includes("SPEC-") ? ["extra"] : ["general", "security", "style"];
    return turn === 0 ? toolTurn(collections.map(read)) : DONE;
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
  if (prompt.includes("BOOTSTRAP:")) {
    return prompt.includes("KNOWN") ? { ...DONE, delayMs: HELD_MS } : DONE;
  }
  if (prompt.includes("COMPARE:")) {
    const agree = prompt.includes("ADMIT") ? false : prompt.includes("BLOCK") ? null : true;
    const reply = judgeReply(JSON.stringify({ agree }));
    const delayMs = prompt.includes("LATE-DECISION") ? HELD_MS : prompt.includes("EARLY-DECISION") ? BRIEF_MS : 0;
    return { ...reply, delayMs };
  }
  if (prompt.includes("EXTRA:")) {
    if (turn > 0) {
      return DONE;
    }
    const reply = toolTurn([toolCall("e-publish", "publish", { name: "account", value: { extra: true } })]);
    return prompt.includes("EARLY-DECISION") ? { ...reply, delayMs: HELD_MS } : reply;
  }
  if (prompt.includes("SECTION:")) {
    const [section = "", at = ""] = prompt.split("SECTION: ")[1]?.split(" at=") ?? [];
    const value = { section, at: Number(at.split(/\s/)[0]) };
    return turn === 0 ? toolTurn([toolCall("d-account", "publish", { name: "account", value })]) : DONE;
  }
  if (prompt.includes("POLISH:")) {
    const [section = ""] = prompt.split("POLISH: ")[1]?.split(" item=") ?? [];
    const round = Number(prompt.split("round=")[1]?.split(/\s/)[0] ?? 0);
    return judgeReply(JSON.stringify({ score: round, text: `${section} v${round}` }));
  }
  if (prompt.includes("ESCALATE:")) {
    return turn === 0
      ? toolTurn([toolCall("x-note", "publish", { name: "note", value: { escalated: true } })])
      : GARBLED;
  }
  if (prompt.includes("SUMMARIZE:")) {
    const index = iteration(prompt);
    if (turn === 0) {
      return toolTurn([
        toolCall("m-accounts", "fs.read", { path: "/inputs/accounts/manifest.json" }),
        toolCall("m-escalation", "fs.read", { path: "/inputs/escalation/manifest.json" }),
      ]);
    }
    return turn === 1
      ? toolTurn([toolCall("m-summary", "publish", { name: "summary", value: { version: index } })])
      : judgeReply(JSON.stringify({ score: index, text: `summary v${index}` }));
  }
  if (prompt.includes("JUDGE-DOC:")) {
    return judgeReply(
      JSON.stringify({
        outcome: "passed",
        reason: "the summary names the report",
        citations: [{ slot: "document", locator: { json_pointer: "/title" } }],
      }),
    );
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
  state: [
    async ({ server }, use) => {
      await configureGateway(WyrdClient.connect());
      const cards = await Cards.connect();
      const receipt = await cards.declareFromPath(fixture("cards/verifier_orchestration/orchestration-desk.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-orchestration-desk-"));
      await cards.pull(receipt.root, bundle);
      const credential = server.scopedApiKey("every_tool", EVERY_TOOL);
      await use(WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential }) }));
    },
    { scope: "file" },
  ],
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

/** Every publication of execution `id`, read back as the administrator through the public paged listing. */
async function publications(id: string): Promise<PublicationSummary[]> {
  const execution = await verify.Execution.connect(id);
  const listed: PublicationSummary[] = [];
  let cursor: string | null | undefined;
  do {
    const page = await execution.publications({ limit: 2, cursor });
    listed.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  return listed;
}

/** One `speculative-review` question's expected observations. */
type Speculated = {
  /** The question, carrying the comparison's and timing's markers. */
  readonly question: string;
  /** Whether the extra reviewer was called at all. */
  readonly called: boolean;
  /** The extra step's status. */
  readonly status: string;
  /** The disposition the consolidator's manifest reports. */
  readonly speculation: string;
  /** The entry names the consolidator was given. */
  readonly entries: readonly string[];
  /** The dispositions the execution's publication listing reports for the extra account. */
  readonly retained: readonly string[];
};

/**
 * After the bootstrap succeeds, the extra review runs while the comparison
 * is pending. A disagreement admits it whether it finished before or after
 * the decision; an agreement discards completed work or cancels running
 * work; a `null` comparison blocks it; a decision known before the extra
 * review is eligible makes no call. The consolidator only ever reads the
 * recorded disposition and receives the account only when it was admitted,
 * while a caller holding only `verifier:read` still inspects the retained
 * excluded account, leaving its disposition unchanged.
 */
test("speculative_disposition_controls_consumer_admission", async ({ provider, server, state }) => {
  const cases: readonly Speculated[] = [
    {
      question: "SPEC-ADMIT-LATE-DECISION",
      called: true,
      status: "succeeded",
      speculation: "admitted",
      entries: ["account"],
      retained: ["admitted"],
    },
    {
      question: "SPEC-ADMIT-EARLY-DECISION",
      called: true,
      status: "succeeded",
      speculation: "admitted",
      entries: ["account"],
      retained: ["admitted"],
    },
    {
      question: "SPEC-DISCARD-LATE-DECISION",
      called: true,
      status: "skipped",
      speculation: "discarded",
      entries: [],
      retained: ["discarded"],
    },
    {
      question: "SPEC-DISCARD-EARLY-DECISION",
      called: true,
      status: "skipped",
      speculation: "discarded",
      entries: [],
      retained: [],
    },
    {
      question: "SPEC-BLOCK-LATE-DECISION",
      called: true,
      status: "failed",
      speculation: "blocked",
      entries: [],
      retained: ["blocked"],
    },
    {
      question: "SPEC-KNOWN",
      called: false,
      status: "skipped",
      speculation: "discarded",
      entries: [],
      retained: [],
    },
  ];
  const asked = (marker: string, question: string): ReceivedRequest[] =>
    provider.received.filter((request) => {
      const prompt = text(request);
      return prompt.includes(marker) && prompt.includes(question);
    });
  const reader = WyrdClient.connect({
    credential: server.scopedApiKey("speculation_reader", [{ resource: "verifier", action: "read", scope: "all" }]),
  });
  const extra = async (id: string): Promise<string[]> =>
    (await publications(id))
      .filter((publication) => publication.producer.step_id === "extra")
      .map((publication) => publication.speculation);

  for (const { question, called, status, speculation, entries, retained } of cases) {
    const result = await state.run().observe.verify("speculative-review", question);

    expect(requirement(result, "consolidated").outcome, `${question}: ${JSON.stringify(result)}`).toBe("passed");
    expect(step(result, "extra").status, `${question}: ${JSON.stringify(result)}`).toBe(status);
    expect(
      arrived.some((prompt) => prompt.includes("EXTRA:") && prompt.includes(question)),
      `${question}: a held extra review counts once it arrives`,
    ).toBe(called);
    const consolidator = asked("CONSOLIDATE:", question).at(-1);
    if (consolidator === undefined) {
      throw new Error(`${question}: the consolidator called the model`);
    }
    const manifest = JSON.parse(lines(results(consolidator)["c-extra"])) as {
      speculation?: string;
      entries?: ReadonlyArray<{ name?: string }>;
    };
    expect(
      [manifest.speculation, (manifest.entries ?? []).map((entry) => entry.name)],
      `${question}: ${JSON.stringify(manifest)}`,
    ).toEqual([speculation, entries]);
    expect(await extra(result.execution_id), question).toEqual(retained);
    if (retained.length > 0) {
      const execution = await verify.Execution.connect(result.execution_id, { client: reader });
      expect(
        await execution.publication("extra", "account", { attempt: 1 }),
        `${question}: a verifier:read grant inspects the retained account`,
      ).toMatchObject({ kind: "json", value: { extra: true } });
      expect(await extra(result.execution_id), `${question}: inspection leaves the disposition unchanged`).toEqual(
        retained,
      );
    }
  }
});

/**
 * A non-repository document pipeline uses the same public controls: the
 * uploaded document's sections fan out as array work in order, the duplicate
 * section keeping its own index; each section is polished by its own
 * sequential repeat, the final values in input order; the empty appendix runs
 * no reviewer and is honestly empty; the flagged document escalates and its
 * failed escalation keeps its partial note; the summarizer refines one
 * iteration at a time over both settled collections within its original
 * bound. The final requirements pass, fail, and stay inconclusive by their own
 * evidence, and the cited judgment points into the uploaded document.
 */
test("document_pipeline_uses_array_repeat_and_publications", async ({ provider, state }) => {
  const result = await state
    .run()
    .observe.verify(
      "document-pipeline",
      verify.File.path(fixture("cards/verifier_orchestration/samples/field-report.json")),
    );
  const requests = (marker: string): string[] =>
    provider.received.map(text).filter((prompt) => prompt.includes(marker));

  const sections = step(result, "sections");
  expect([sections.status, sections.output], JSON.stringify(result)).toEqual([
    "succeeded",
    [{ done: true }, { done: true }, { done: true }],
  ]);
  expect(
    [...new Set(requests("SECTION:").map((prompt) => prompt.split("SECTION: ")[1]?.split("\n")[0]))].sort(),
    "the duplicate keeps its own index and the empty appendix calls no reviewer",
  ).toEqual(["claims at=1", "claims at=2", "intro at=0"]);
  const polish = step(result, "polish");
  expect(
    [polish.status, polish.output],
    "each section repeats independently and the final values keep input order",
  ).toEqual([
    "succeeded",
    [
      { score: 1, text: "intro v1" },
      { score: 1, text: "claims v1" },
      { score: 1, text: "claims v1" },
    ],
  ]);
  const rounds: Record<string, string[]> = {};
  for (const prompt of requests("POLISH:")) {
    const [item, round] = prompt.split(" item=")[1]?.split(" round=") ?? [];
    if (item !== undefined && round !== undefined) {
      rounds[item] = [...(rounds[item] ?? []), round.split(/\s/)[0] ?? ""];
    }
  }
  expect(rounds, "every item runs its own rounds in order until its condition holds").toEqual({
    "0": ["0", "1"],
    "1": ["0", "1"],
    "2": ["0", "1"],
  });
  const appendix = step(result, "appendix");
  expect([appendix.status, appendix.output]).toEqual(["succeeded", []]);
  expect(step(result, "escalate").status).toBe("failed");
  const summarize = step(result, "summarize");
  expect([summarize.status, summarize.output]).toEqual(["succeeded", { score: 1, text: "summary v1" }]);
  const summaries = provider.received.filter((request) => text(request).includes("SUMMARIZE:"));
  expect(
    summaries.map((request) => iteration(text(request))),
    "one iteration at a time, stopping within the bound of 3",
  ).toEqual([0, 0, 0, 1, 1, 1]);
  expect(text(summaries[0] as ReceivedRequest)).toContain("previous=null");
  const read = results(summaries[1] as ReceivedRequest);
  expect(entries(read["m-accounts"])).toEqual([
    [0, null, "account"],
    [1, null, "account"],
    [2, null, "account"],
  ]);
  expect(
    [(JSON.parse(lines(read["m-escalation"])) as { status?: string }).status, entries(read["m-escalation"])],
    "the failed escalation keeps its partial note",
  ).toEqual(["failed", [[null, null, "note"]]]);
  const execution = await verify.Execution.connect(result.execution_id);
  const accounts: [number | null | undefined, unknown][] = [];
  for (const publication of await publications(result.execution_id)) {
    if (publication.producer.step_id !== "sections") {
      continue;
    }
    const { producer } = publication;
    const account = await execution.publication(producer.step_id, publication.name, {
      attempt: producer.attempt,
      item_index: producer.item_index,
    });
    if (account.kind !== "json") {
      throw new Error(`an account is a JSON publication: ${JSON.stringify(account)}`);
    }
    accounts.push([producer.item_index, account.value]);
  }
  expect(accounts.sort(([left], [right]) => Number(left) - Number(right))).toEqual([
    [0, { section: "intro", at: 0 }],
    [1, { section: "claims", at: 1 }],
    [2, { section: "claims", at: 2 }],
  ]);

  expect(Object.fromEntries(result.outcome.requirements.map((judged) => [judged.id, judged.outcome]))).toEqual({
    "appendix-reviewed": "failed",
    cited: "passed",
    "escalation-done": "inconclusive",
    summarized: "passed",
  });
  expect(requirement(result, "cited").citations).toEqual([{ slot: "document", locator: { json_pointer: "/title" } }]);
  expect(result.outcome.verdict).toBe("failed");
});
