// Hosted Agent tools, end to end: `publish-desk`'s drafter writes and revises
// files in its private `/work` scratch with `fs.write`, publishes a file, a
// directory, a JSON value, and an explicit `null` under logical names, and a
// downstream consumer reads exactly the accepted content through its mounted
// inputs and the collection manifest.
//
// The local upstream stands in for the model provider and scripts each Agent
// by its prompt marker and the number of assistant turns already in the
// conversation. The test then inspects what the model received, the recorded
// tool calls, and the publications read back through the public execution
// surface.
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

vi.setConfig({ testTimeout: 180_000 });

/** The stable code of a permission denial, carried by every denied tool call's error. */
const DENIED = "WYRD_PERMISSION_403_DENIED_RBAC";

/** The stable code of a refused tool argument. */
const INVALID = "WYRD_SPEC_400_VALIDATION";

/** The report the drafter publishes; later scratch writes never change it. */
const REPORT = "# Final report\n";

/** The JSON assessment the drafter publishes by value. */
const ASSESSMENT = { risk: "low", score: 0.9, notes: ["é", "two words"] };

/** One Chat Completions message as the upstream receives it. */
type Message = {
  readonly role?: string;
  readonly content?: unknown;
  readonly tool_call_id?: string;
};

/** The fields of a tool result the test reads: a receipt, a read, a listing, a write, or a refusal. */
type ToolResult = {
  readonly name?: string;
  readonly kind?: string;
  readonly path?: string;
  readonly bytes?: number;
  readonly units?: ReadonlyArray<{ readonly text?: string }>;
  readonly files?: ReadonlyArray<{ readonly path?: string }>;
  readonly code?: string;
  readonly details?: { readonly rule?: string; readonly field?: string };
};

/** The Chat Completions body fields the test reads. */
type Completion = {
  readonly messages?: readonly Message[];
  readonly tools?: ReadonlyArray<{ readonly function?: { readonly name?: string } }>;
};

/** The parsed Chat Completions body of `request`, empty when there is none. */
function completion(request: ReceivedRequest | undefined): Completion {
  return request === undefined ? {} : (JSON.parse(request.body) as Completion);
}

/** The text of every message `request` carries, joined. */
function text(request: ReceivedRequest): string {
  return (completion(request).messages ?? [])
    .map((message) => (typeof message.content === "string" ? message.content : ""))
    .join("\n");
}

/** Every tool result `request` carries, by the provider call ID it answers, parsed from its JSON content. */
function results(request: ReceivedRequest | undefined): Record<string, ToolResult> {
  const found: Record<string, ToolResult> = {};
  for (const message of completion(request).messages ?? []) {
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

/** The stable code and repair rule of a refused tool result. */
function refusal(result: ToolResult | undefined): [string | undefined, string | undefined] {
  return [result?.code, result?.details?.rule];
}

/** The function names the model was offered in `request`, sorted. */
function offered(request: ReceivedRequest | undefined): string[] {
  return (completion(request).tools ?? []).map((tool) => tool.function?.name ?? "").sort();
}

/** An `fs.write` call's arguments. */
function write(path: string, content: string): unknown {
  return { path, content };
}

/** The final `{"done": true}` answer. */
const DONE = judgeReply('{"done":true}');

/**
 * The drafter's scripted turn `turn`: write and revise the report, write a
 * tree with a nested Unicode, spaced filename and an empty directory, publish
 * all four publications, then mutate scratch and try every refused publication
 * and write, then finish.
 */
function draft(turn: number): Reply {
  switch (turn) {
    case 0:
      return toolTurn([toolCall("w1", "fs.write", write("/work/report.md", "draft v1"))]);
    case 1:
      return toolTurn([
        toolCall("w2", "fs.write", write("/work/report.md", REPORT)),
        toolCall("w3", "fs.write", write("/work/tree/a.md", "alpha")),
        toolCall("w4", "fs.write", write("/work/tree/nested dir/é b.md", "beta")),
        toolCall("w5", "code.run", { source: "import os\nos.makedirs('/work/tree/empty')" }),
      ]);
    case 2:
      return toolTurn([
        toolCall("r1", "fs.read", { path: "/work/report.md" }),
        toolCall("p-report", "publish", { name: "report", path: "/work/report.md" }),
        toolCall("p-tree", "publish", { name: "tree", path: "/work/tree" }),
        toolCall("p-assessment", "publish", { name: "assessment", value: ASSESSMENT }),
        toolCall("p-nothing", "publish", { name: "nothing", value: null }),
      ]);
    case 3:
      return toolTurn([
        toolCall("w6", "fs.write", write("/work/report.md", "mutated after publishing")),
        toolCall("x-conflict", "publish", { name: "report", value: 1 }),
        toolCall("x-both", "publish", { name: "both", path: "/work/report.md", value: 1 }),
        toolCall("x-neither", "publish", { name: "neither" }),
        toolCall("x-name", "publish", { name: "bad name!", value: 1 }),
        toolCall("x-missing", "publish", { name: "ghost", path: "/work/missing.md" }),
        toolCall("x-inputs", "publish", { name: "foreign", path: "/inputs/report.md" }),
        toolCall("x-escape", "publish", { name: "escape", path: "/work/../etc" }),
        toolCall("x-forged", "publish", { name: "forged", value: 1, attempt: 9, step_id: "consume" }),
        toolCall("x-write-inputs", "fs.write", write("/inputs/x.md", "x")),
        toolCall("x-write-escape", "fs.write", write("/work/../x.md", "x")),
      ]);
    default:
      return DONE;
  }
}

/**
 * The consumer's scripted turn `turn` of `request`: read every mount and the
 * collection manifest, then read the report through the manifest entry's
 * actual path, then finish.
 */
function consume(turn: number, request: ReceivedRequest): Reply {
  if (turn === 0) {
    return toolTurn([
      toolCall("c-report", "fs.read", { path: "/inputs/report.md" }),
      toolCall("c-tree", "fs.list", { path: "/inputs/tree" }),
      toolCall("c-assessment", "fs.read", { path: "/inputs/assessment.json" }),
      toolCall("c-nothing", "fs.read", { path: "/inputs/nothing.json" }),
      toolCall("c-manifest", "fs.read", { path: "/inputs/all/manifest.json" }),
    ]);
  }
  if (turn === 1) {
    const manifest = JSON.parse(lines(results(request)["c-manifest"]) || "{}") as {
      entries?: ReadonlyArray<{ name?: string; path?: string }>;
    };
    const entry = manifest.entries?.find((each) => each.name === "report")?.path ?? "missing";
    return toolTurn([toolCall("c-entry", "fs.read", { path: `/inputs/all/${entry}` })]);
  }
  return DONE;
}

/** The local model upstream: one reply per request, chosen by the Agent's prompt marker and the turn. */
function upstream(request: ReceivedRequest): Reply {
  const prompt = text(request);
  const turn = (completion(request).messages ?? []).filter((message) => message.role === "assistant").length;
  if (prompt.includes("DRAFT:")) {
    return draft(turn);
  }
  if (prompt.includes("CONSUME:")) {
    return consume(turn, request);
  }
  return { status: 404 };
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

/** The base64 SHA-256 a download plan reports for each publication's planned file. */
const DIGESTS = {
  report: "Ir06sS2VoUyv9Zu1ER4D6fjj0mKQiWHzAgYl/q7kE3M=",
  tree: "9E5k5185SOn3P436lHIcTOjLtPJlxHkMcCstQc+/J1M=",
} as const;

/**
 * `test` on a fresh verification-capable server per test whose gateway
 * deploys `gpt-4o` on the local upstream, with `publish-desk` declared and its
 * Card bundle pulled.
 */
const test = serverTest({ provider: upstream, verificationRuntime: true, perTest: true }).extend<{
  bundle: string;
}>({
  bundle: async ({ server: _ }, use) => {
    await configureGateway(WyrdClient.connect());
    const cards = await Cards.connect();
    const receipt = await cards.declareFromPath(fixture("cards/hosted_agent_tools/publish-desk.yaml"));
    const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-publish-desk-"));
    await cards.pull(receipt.root, bundle);
    await use(bundle);
  },
});

/**
 * The drafter revises a report in scratch and publishes it, a tree with a
 * nested Unicode, spaced filename and an empty directory, a JSON assessment,
 * and an explicit `null`; each receipt is exactly `{name, kind}`. Its later
 * scratch mutation, a same-name republish, both or neither source, an invalid
 * name, a missing, `/inputs`, or escaping path, a forged producer field, and
 * writes outside scratch are refused and publish nothing. The consumer reads
 * exactly the published bytes through its mounts and finds the report through
 * the collection manifest's actual path; the public readback returns the same
 * content with matching download digests. A principal granted only `fs.write`
 * writes scratch while every other call is denied, publishing nothing.
 */
test("agent_publishes_file_directory_and_json", async ({ server, provider, bundle }) => {
  const verifyAs = (name: string, toolScope: unknown, question: string): Promise<VerificationResult> => {
    const credential = server.scopedApiKey(name, grants(toolScope));
    const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential }) });
    return state.run().observe.verify("report-pipeline", question);
  };

  const result = await verifyAs("every_tool", "all", "FULL-RUN");
  const received = [...provider.received];
  const marked = (marker: string) => received.filter((request) => text(request).includes(marker));
  const drafted = marked("DRAFT:");
  const consumed = marked("CONSUME:");

  expect(requirement(result, "consumed").outcome, JSON.stringify(result)).toBe("passed");
  expect(offered(drafted[0])).toEqual(["code_run", "fs_read", "fs_write", "publish"]);
  expect(offered(consumed[0]), "the consumer is offered no write tools").toEqual(["fs_list", "fs_read"]);

  const drafter = results(drafted.at(-1));
  for (const [call, name, kind] of [
    ["p-report", "report", "file"],
    ["p-tree", "tree", "directory"],
    ["p-assessment", "assessment", "json"],
    ["p-nothing", "nothing", "json"],
  ] as const) {
    expect(drafter[call], call).toEqual({ name, kind });
  }
  expect(lines(drafter.r1)).toBe(REPORT.trimEnd());
  expect(drafter.w6).toEqual({ path: "/work/report.md", bytes: 24 });
  for (const [call, rule] of [
    ["x-conflict", "publication_name"],
    ["x-both", "publication_source"],
    ["x-neither", "publication_source"],
    ["x-missing", "workspace_missing"],
    ["x-inputs", "publication_path"],
    ["x-escape", "publication_path"],
    ["x-write-inputs", "write_path"],
    ["x-write-escape", "write_path"],
  ] as const) {
    expect(refusal(drafter[call]), call).toEqual([INVALID, rule]);
  }
  expect([drafter["x-name"]?.code, drafter["x-name"]?.details?.field]).toEqual([INVALID, "name"]);
  expect(drafter["x-forged"]?.code, "the host owns the producer identity").toBe("SKALD_TOOL_422_INPUT");

  const consumer = results(consumed.at(-1));
  expect(lines(consumer["c-report"]), "scratch changes after publishing never reach the publication").toBe(
    REPORT.trimEnd(),
  );
  expect(lines(consumer["c-entry"])).toBe(REPORT.trimEnd());
  expect(consumer["c-tree"]?.files?.map((file) => file.path)).toEqual([
    "/inputs/tree/a.md",
    "/inputs/tree/nested dir/é b.md",
  ]);
  expect(JSON.parse(lines(consumer["c-assessment"]))).toEqual(ASSESSMENT);
  expect(lines(consumer["c-nothing"])).toBe("null");
  const manifest = JSON.parse(lines(consumer["c-manifest"])) as {
    entries: ReadonlyArray<{ name: string; kind: string }>;
  };
  expect(
    manifest.entries.map((entry) => [entry.name, entry.kind]).sort(),
    "publishes in one turn run concurrently, in acceptance order",
  ).toEqual([
    ["assessment", "json"],
    ["nothing", "json"],
    ["report", "file"],
    ["tree", "directory"],
  ]);

  const admin = WyrdClient.connect({ credential: server.apiKey });
  const publications = async (executionId: string): Promise<PublicationSummary[]> => {
    const execution = await verify.Execution.connect(executionId, { client: admin });
    const listed: PublicationSummary[] = [];
    let cursor: string | null | undefined;
    do {
      const page = await execution.publications({ limit: 2, cursor });
      listed.push(...page.items);
      cursor = page.next_cursor;
    } while (cursor);
    return listed;
  };
  expect(
    (await publications(result.execution_id))
      .map((item) => [item.producer.step_id, item.name, item.producer.attempt, item.kind])
      .sort((left, right) => String(left[1]).localeCompare(String(right[1]))),
    "refused calls and the final answer publish nothing",
  ).toEqual([
    ["draft", "assessment", 1, "json"],
    ["draft", "nothing", 1, "json"],
    ["draft", "report", 1, "file"],
    ["draft", "tree", 1, "directory"],
  ]);
  const execution = await verify.Execution.connect(result.execution_id, { client: admin });
  expect(await execution.publication("draft", "tree", { attempt: 1 })).toMatchObject({
    kind: "directory",
    manifest: {
      files: [{ path: "a.md" }, { path: "nested dir/é b.md" }],
      directories: ["empty", "nested dir"],
    },
  });
  expect(await execution.publication("draft", "assessment", { attempt: 1 })).toMatchObject({
    kind: "json",
    value: ASSESSMENT,
  });
  expect(await execution.publication("draft", "nothing", { attempt: 1 })).toMatchObject({ kind: "json", value: null });
  for (const [name, path, content] of [
    ["report", null, REPORT],
    ["tree", "nested dir/é b.md", "beta"],
  ] as const) {
    const planned = await execution.download("draft", name, { attempt: 1, path });
    expect([planned.size_bytes, planned.sha256], name).toEqual([Buffer.byteLength(content), DIGESTS[name]]);
  }

  const question = "FULL-RUN narrowed";
  const narrowed = await verifyAs("writes_only", { tool: "fs.write" }, question);
  const calls = step(narrowed, "draft").tool_calls ?? [];
  expect(calls.length, question).toBeGreaterThan(0);
  for (const call of calls) {
    const denied = (call.error ?? "").includes(DENIED);
    expect(denied, `${question} ${call.tool}: ${JSON.stringify(call)}`).toBe(call.tool !== "fs.write");
    if (denied) {
      expect([call.output_sha256 ?? null, call.returned_bytes], `${question}: a denied call returns nothing`).toEqual([
        null,
        0,
      ]);
    }
  }
  const asked = provider.received.filter((request) => text(request).includes(question));
  expect(
    JSON.stringify(results(asked.at(-1))["p-assessment"]),
    `${question}: a denied publish echoes no content`,
  ).not.toContain("low");
  expect(await publications(narrowed.execution_id), question).toEqual([]);
  expect(step(narrowed, "consume").status, question).toBe("failed");
  expect(
    asked.every((request) => !text(request).includes("CONSUME:")),
    `${question}: the consumer never reached the model`,
  ).toBe(true);
});
