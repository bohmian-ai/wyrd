import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { expect, vi } from "vitest";

import { Cards, type DeclarationReceipt, WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { fixture, declared, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const DESK = fixture("cards/declare_and_pull/support-desk.yaml");
const QUICKSTART = resolve(import.meta.dirname, "../../../../../examples/quickstart");

/** The support desk graph, declared after the artifact-bearing Model it references. */
const test = serverTest().extend<{
  cards: Cards;
  model: DeclarationReceipt;
  desk: DeclarationReceipt;
}>({
  cards: [async ({ server: _ }, use) => use(await Cards.connect()), { scope: "file" }],
  model: [
    async ({ cards }, use) => use(await cards.declareFromPath(fixture("cards/declare_and_pull/support-model.yaml"))),
    { scope: "file" },
  ],
  desk: [async ({ cards, model: _ }, use) => use(await cards.declareFromPath(DESK)), { scope: "file" }],
});

/** A fresh directory for one Card bundle. */
function bundleDir(): string {
  return mkdtempSync(join(tmpdir(), "wyrd-ts-bundle-"));
}

test("service graph declares and pulls", async ({ cards, desk }) => {
  const bundle = bundleDir();
  const summary = await cards.pull(desk.root, bundle);
  expect(summary).toMatchObject({ mode: "complete", root: desk.root });

  const state = WyrdState.fromPath(bundle);
  expect(state.rootRef).toEqual(desk.root);
  expect(state.service.metadata.name).toBe("support-desk");
  expect(state.model("model").metadata.name).toBe("support-model");
  expect(state.card("agent").kind).toBe("Agent");
  expect(state.agent("agent").metadata.name).toBe("support-agent");
  expect(() => state.data("model")).toThrow(expect.objectContaining({ code: "WYRD_SDK_400_CARD_KIND_MISMATCH" }));
  expect(() => state.prompt("model")).toThrow(expect.objectContaining({ code: "WYRD_SDK_400_CARD_KIND_MISMATCH" }));
  expect(readFileSync(join(bundle, "cards/model/artifacts/support-model.bin"))).toEqual(
    readFileSync(fixture("cards/declare_and_pull/support-model.bin")),
  );
  expect("artifacts" in state).toBe(false);
});

test("runs the Quickstart script from a directory holding only its Cards", async ({ server: _ }) => {
  const reader = mkdtempSync(join(tmpdir(), "wyrd-ts-quickstart-reader-"));
  for (const file of ["no-refund-promise.yaml", "support-reply.yaml"]) cpSync(join(QUICKSTART, file), join(reader, file));
  const ref = (kind: string, name: string) => `default/${kind}/${name}@1\\.0\\.0#[0-9a-f-]+`;
  const printed = () =>
    execFileSync(process.execPath, [join(QUICKSTART, "quickstart.ts")], { cwd: reader, encoding: "utf8" });

  const first = printed();
  expect(first).toMatch(
    new RegExp(
      `^passed: the comparison held\\n  subject:  ${ref("Service", "support-reply")}\\n  verifier: ${ref("Verifier", "no-refund-promise")}\\n` +
        `failed: operator NotContains returned false\\n  subject:  ${ref("Service", "support-reply")}\\n  verifier: ${ref("Verifier", "no-refund-promise")}\\n$`,
    ),
  );
  expect(printed()).toBe(first);
  expect(readdirSync(reader).sort()).toEqual(["no-refund-promise.yaml", "support-reply.yaml"]);
});

test("declares a ready verification state", async ({ server, model: _ }) => {
  const quickstart = mkdtempSync(join(tmpdir(), "wyrd-ts-quickstart-"));
  cpSync(QUICKSTART, quickstart, { recursive: true });
  const service = join(quickstart, "support-reply.yaml");

  const state = await WyrdState.declare(service);
  const run = state.run();
  const passed = await run.observe.verify("no-refund-promise", { answer: "Your refund request is approved." });
  const failed = await run.observe.verify("no-refund-promise", { answer: "You have a guaranteed refund." });
  expect([passed.outcome.verdict, failed.outcome.verdict]).toEqual(["passed", "failed"]);
  expect((await WyrdState.declare(service)).rootRef).toEqual(state.rootRef);

  const reader = WyrdClient.connect({ credential: server.scopedApiKey("quickstart_reader", ["cards:read"]) });
  await expect(WyrdState.declare(service, { client: reader })).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
  const verifier = join(quickstart, "no-refund-promise.yaml");
  writeFileSync(verifier, readFileSync(verifier, "utf8").replace("expected: guaranteed refund", "expected: guaranteed"));
  await expect(WyrdState.declare(service)).rejects.toMatchObject({ code: "WYRD_REGISTRY_409_SPEC_DRIFT" });
  rmSync(verifier);
  await expect(WyrdState.declare(service)).rejects.toMatchObject({ code: "WYRD_LOADER_400_INVALID_ENVELOPE" });

  // A caller-owned bundle stays after the state and loads offline.
  const bundle = bundleDir();
  const desk = await WyrdState.declare(DESK, { bundleDir: bundle });
  expect(desk.model("model").metadata.name).toBe("support-model");
  expect(WyrdState.fromPath(bundle).rootRef).toEqual(desk.rootRef);
  expect(readFileSync(join(bundle, "cards/model/artifacts/support-model.bin"))).toEqual(
    readFileSync(fixture("cards/declare_and_pull/support-model.bin")),
  );
});

test("latest version resolves to the declared card", async ({ cards, desk }) => {
  expect(await cards.resolveLatest("Service", "default", "support-desk")).toMatchObject(desk.root);
  await expect(cards.resolveLatest("Service", "default", "never-registered")).rejects.toMatchObject({
    code: "WYRD_REGISTRY_404_CARD_NOT_FOUND",
  });
});

test("declaring the graph again is idempotent", async ({ cards, desk }) => {
  const again = await cards.declareFromPath(DESK);

  expect(again.root).toEqual(desk.root);
  // The desk's only binding has no schedule Trigger, so none is projected.
  expect((await cards.get(again.root)).status?.verification).toBeUndefined();
});

test("cards get returns every kind typed", async ({ cards, model, desk }) => {
  const baseline = await cards.declareFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  const daily = await cards.declareFromPath(fixture("cards/scheduled_drift_alerts_operator/daily.yaml"));
  const refs = [baseline, daily, model, desk].flatMap((receipt) => receipt.outcomes.map((outcome) => outcome.card_ref));

  const read = await Promise.all(refs.map((ref) => cards.get(ref)));

  expect(read.map((card) => card.kind).sort()).toEqual([
    "Agent",
    "Data",
    "Model",
    "Operator",
    "Prompt",
    "Service",
    "Trigger",
    "Verifier",
  ]);
  const verifier = read.find((card) => card.kind === "Verifier");
  expect(verifier?.kind === "Verifier" && verifier.spec.requirements?.[0]?.check.kind).toBe("assert");
  const data = read.find((card) => card.kind === "Data");
  expect(data?.kind === "Data" && data.spec.stats?.row_count).toBe(100);
});

test("artifact without digest declares", async ({ model }) => {
  expect(model.root.kind).toBe("Model");
  expect(model.outcomes[0]?.artifact_hash).toEqual(expect.any(String));
});

test("wrong artifact digest is refused", async ({ cards }) => {
  await expect(
    cards.declareFromPath(fixture("invalid/wrong-artifact-digest/support-model.yaml")),
  ).rejects.toMatchObject({ code: "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH" });
});

// Registration reports the loader's refusal under its own catalog code.
test("retired card kind is refused", async ({ cards }) => {
  await expect(cards.declareFromPath(fixture("invalid/retired-drift-kind.yaml"))).rejects.toMatchObject({
    code: "WYRD_LOADER_400_INVALID_ENVELOPE",
  });
});

test("reader cannot declare cards", async ({ server, desk: _ }) => {
  const reader = await Cards.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("card_reader", ["cards:read"]) }),
  });

  await expect(reader.declareFromPath(DESK)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("cli declare get and pull round trip the graph", async ({ server, desk }) => {
  const client = WyrdClient.connect({ credential: server.apiKey });
  const declaredGraph = await cli.declare(DESK, { client });
  const selector = { kind: "Service", space: "default", name: "support-desk", version: "1.0.0" };
  const bundle = bundleDir();

  const envelope = await cli.get(selector, { client });
  const summary = await cli.pull(selector, bundle, { client });

  expect(declaredGraph.root).toEqual(desk.root);
  expect(envelope).toMatchObject({ apiVersion: "wyrd/v1", kind: "Service", metadata: { uid: desk.root.uid } });
  expect(summary).toMatchObject({ mode: "complete", root: desk.root });
  expect(WyrdState.fromPath(bundle).rootRef).toEqual(desk.root);
});

test("refused cli command raises its catalog code", async ({ server: _ }) => {
  const missing = { kind: "Service", space: "default", name: "no-such-service", version: "1.0.0" };
  await expect(cli.get(missing)).rejects.toMatchObject({ code: "WYRD_REGISTRY_404_CARD_NOT_FOUND" });
  await expect(cli.pull(missing, bundleDir())).rejects.toMatchObject({ code: "WYRD_REGISTRY_404_CARD_NOT_FOUND" });
});

test("pulled agent prompt card navigation", async ({ cards }) => {
  await cards.declareFromPath(fixture("cards/agent_prompt/desk-prompt.yaml"));
  const desk = await cards.declareFromPath(fixture("cards/agent_prompt/prompt-desk.yaml"));
  const bundle = bundleDir();
  await cards.pull(desk.root, bundle);
  vi.stubEnv("WYRD_SERVER_URL", "http://127.0.0.1:1");

  const state = WyrdState.fromPath(bundle);

  const explicit = state.get("my_prompt");
  if (explicit.kind !== "Prompt") {
    throw new Error("my_prompt is looked up as a Prompt");
  }
  expect(explicit.ref).toEqual(declared(desk, "desk-prompt"));
  expect(state.prompt("my_prompt").ref).toEqual(explicit.ref);
  for (const [alias, prompt] of [
    ["inline_agent", "inline-agent-prompt"],
    ["path_agent", "desk-prompt"],
    ["ref_agent", "desk-prompt"],
  ] as const) {
    const agent = state.get(alias);
    if (agent.kind !== "Agent") {
      throw new Error(`${alias} is looked up as an Agent`);
    }
    expect(agent.prompt().ref).toEqual(declared(desk, prompt));
    expect(agent.promptRef).toEqual(declared(desk, prompt));
    expect(agent.prompt().metadata.name).toBe(prompt);
    expect(state.agent(alias).prompt().ref).toEqual(declared(desk, prompt));
  }
  expect(state.agent("path_agent").prompt().spec).toEqual(explicit.spec);
  expect(() => state.get("missing")).toThrow(expect.objectContaining({ code: "WYRD_SDK_404_UNKNOWN_ALIAS" }));
  expect(() => state.agent("my_prompt")).toThrow(expect.objectContaining({ code: "WYRD_SDK_400_CARD_KIND_MISMATCH" }));
});

const PORTABLE_SPEC = {
  provider: "anthropic",
  model: "claude-sonnet-4-5",
  system: `You answer for \${team}.`,
  messages: [
    {
      role: "user",
      content: [
        { type: "text", text: `\${question}` },
        { type: "image", source: `\${media:photo}` },
        { type: "image", source: { type: "base64", mime_type: "image/png", data: "iVBORw0KGgo=" } },
        { type: "document", source: `\${media:__wyrd_media_0}` },
      ],
    },
  ],
  settings: { temperature: 0.2, max_output_tokens: 256 },
  response_format: {
    type: "object",
    properties: { answer: { type: "string" } },
    required: ["answer"],
  },
};
const NATIVE_SPEC = {
  provider: "openai",
  model: "gpt-4o",
  format: "chat_completion",
  body: {
    messages: [{ role: "user", content: "Summarize {{ticket}}." }],
    seed: 123,
    future_knob: { enabled: true },
  },
};

test("prompt forms round trip as authored", async ({ cards }) => {
  const receipt = await cards.declareFromPath(fixture("cards/declare_and_pull/authored-prompts.yaml"));
  const bundle = bundleDir();
  await cards.pull(receipt.root, bundle);
  const state = WyrdState.fromPath(bundle);

  const answer = PORTABLE_SPEC.response_format;
  const media = ["__wyrd_media_0", "photo"];
  for (const [name, hydrated, authored, variables, mediaVariables, schema] of [
    ["portable-prompt", state.prompt("portable"), PORTABLE_SPEC, ["question", "team"], media, answer],
    ["native-prompt", state.prompt("native"), NATIVE_SPEC, ["ticket"], [], null],
    ["inline-prompt-agent-prompt", state.agent("agent").prompt(), PORTABLE_SPEC, ["question", "team"], media, answer],
    ["inline-native-prompt-agent-prompt", state.agent("native-agent").prompt(), NATIVE_SPEC, ["ticket"], [], null],
  ] as const) {
    const read = await cards.get(declared(receipt, name));
    expect(hydrated.ref).toEqual(declared(receipt, name));
    expect(read.spec).toEqual(authored);
    expect(hydrated.spec).toEqual(authored);
    expect(read.metadata.spec_hash).toBeTruthy();
    expect(hydrated.metadata.spec_hash).toBe(read.metadata.spec_hash);
    expect([...hydrated.variables].sort()).toEqual(variables);
    expect([...hydrated.mediaVariables].sort()).toEqual(mediaVariables);
    expect(hydrated.responseSchema).toEqual(schema);
    expect(hydrated.responseSchemaName).toBe(schema === null ? null : "response");
  }
});
