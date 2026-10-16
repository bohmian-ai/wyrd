import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { Cards, type RegistrationReceipt, WyrdClient, WyrdState } from "wyrd-ai";
import { cli } from "@wyrd/testing";

import { fixture, registered, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const DESK = fixture("cards/register_and_hydrate/support-desk.yaml");

/** The support desk graph, registered after the artifact-bearing Model it references. */
const test = serverTest().extend<{
  cards: Cards;
  model: RegistrationReceipt;
  desk: RegistrationReceipt;
}>({
  cards: [async ({ server: _ }, use) => use(await Cards.connect()), { scope: "file" }],
  model: [
    async ({ cards }, use) =>
      use(await cards.registerFromPath(fixture("cards/register_and_hydrate/support-model.yaml"))),
    { scope: "file" },
  ],
  desk: [async ({ cards, model: _ }, use) => use(await cards.registerFromPath(DESK)), { scope: "file" }],
});

/** A fresh directory for one hydrated bundle. */
function bundleDir(): string {
  return mkdtempSync(join(tmpdir(), "wyrd-ts-bundle-"));
}

test("service graph registers and hydrates", async ({ cards, desk }) => {
  const bundle = bundleDir();
  const summary = await cards.hydrate(desk.root, bundle);
  expect(summary).toMatchObject({ mode: "complete", root: desk.root });

  const state = WyrdState.fromPath(bundle);
  expect(state.rootRef).toEqual(desk.root);
  expect(state.service.metadata.name).toBe("support-desk");
  expect(state.model("model").metadata.name).toBe("support-model");
  expect(state.card("agent").kind).toBe("Agent");
  expect(state.agent("agent").metadata.name).toBe("support-agent");
  expect(() => state.data("model")).toThrow(expect.objectContaining({ code: "WYRD_SDK_400_CARD_KIND_MISMATCH" }));
  expect(() => state.prompt("model")).toThrow(expect.objectContaining({ code: "WYRD_SDK_400_CARD_KIND_MISMATCH" }));
  const [artifact] = state.artifacts("model");
  expect(artifact.relative_path).toBe("support-model.bin");
  expect(readFileSync(artifact.local_path)).toEqual(
    readFileSync(fixture("cards/register_and_hydrate/support-model.bin")),
  );
});

test("latest version resolves to the registered card", async ({ cards, desk }) => {
  expect(await cards.resolveLatest("Service", "default", "support-desk")).toMatchObject(desk.root);
  await expect(cards.resolveLatest("Service", "default", "never-registered")).rejects.toMatchObject({
    code: "WYRD_REGISTRY_404_CARD_NOT_FOUND",
  });
});

test("registering the graph again is idempotent", async ({ cards, desk }) => {
  const again = await cards.registerFromPath(DESK);

  expect(again.root).toEqual(desk.root);
  // The desk's only binding has no schedule Trigger, so none is projected.
  expect((await cards.get(again.root)).status?.verification).toBeUndefined();
});

test("cards get returns every kind typed", async ({ cards, model, desk }) => {
  const baseline = await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
  const refs = [baseline, model, desk].flatMap((receipt) => receipt.outcomes.map((outcome) => outcome.card_ref));

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

test("artifact without digest registers", async ({ model }) => {
  expect(model.root.kind).toBe("Model");
  expect(model.outcomes[0]?.artifact_hash).toEqual(expect.any(String));
});

test("wrong artifact digest is refused", async ({ cards }) => {
  await expect(
    cards.registerFromPath(fixture("invalid/wrong-artifact-digest/support-model.yaml")),
  ).rejects.toMatchObject({ code: "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH" });
});

// Registration reports the loader's refusal under its own catalog code.
test("retired card kind is refused", async ({ cards }) => {
  await expect(cards.registerFromPath(fixture("invalid/retired-drift-kind.yaml"))).rejects.toMatchObject({
    code: "WYRD_LOADER_400_INVALID_ENVELOPE",
  });
});

test("reader cannot register cards", async ({ server, desk: _ }) => {
  const reader = await Cards.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("card_reader", ["cards:read"]) }),
  });

  await expect(reader.registerFromPath(DESK)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("cli apply and get round trip the graph", async ({ server, desk }) => {
  const client = WyrdClient.connect({ credential: server.apiKey });
  const applied = await cli.apply(DESK, { client });
  const bundle = bundleDir();

  const summary = await cli.get({ kind: "Service", space: "default", name: "support-desk", version: "1.0.0" }, bundle, {
    client,
  });

  expect(applied.root).toEqual(desk.root);
  expect(summary).toMatchObject({ mode: "complete", root: desk.root });
  expect(WyrdState.fromPath(bundle).rootRef).toEqual(desk.root);
});

test("refused cli command raises its catalog code", async ({ server: _ }) => {
  await expect(
    cli.get({ kind: "Service", space: "default", name: "no-such-service", version: "1.0.0" }, bundleDir()),
  ).rejects.toMatchObject({ code: "WYRD_REGISTRY_404_CARD_NOT_FOUND" });
});

test("hydrated agent prompt card navigation", async ({ cards }) => {
  await cards.registerFromPath(fixture("cards/agent_prompt/desk-prompt.yaml"));
  const desk = await cards.registerFromPath(fixture("cards/agent_prompt/prompt-desk.yaml"));
  const bundle = bundleDir();
  await cards.hydrate(desk.root, bundle);
  vi.stubEnv("WYRD_SERVER_URL", "http://127.0.0.1:1");

  const state = WyrdState.fromPath(bundle);

  const explicit = state.get("my_prompt");
  if (explicit.kind !== "Prompt") {
    throw new Error("my_prompt is looked up as a Prompt");
  }
  expect(explicit.ref).toEqual(registered(desk, "desk-prompt"));
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
    expect(agent.prompt().ref).toEqual(registered(desk, prompt));
    expect(agent.promptRef).toEqual(registered(desk, prompt));
    expect(agent.prompt().metadata.name).toBe(prompt);
    expect(state.agent(alias).prompt().ref).toEqual(registered(desk, prompt));
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
  const receipt = await cards.registerFromPath(fixture("cards/register_and_hydrate/authored-prompts.yaml"));
  const bundle = bundleDir();
  await cards.hydrate(receipt.root, bundle);
  const state = WyrdState.fromPath(bundle);

  const answer = PORTABLE_SPEC.response_format;
  const media = ["__wyrd_media_0", "photo"];
  for (const [name, hydrated, authored, variables, mediaVariables, schema] of [
    ["portable-prompt", state.prompt("portable"), PORTABLE_SPEC, ["question", "team"], media, answer],
    ["native-prompt", state.prompt("native"), NATIVE_SPEC, ["ticket"], [], null],
    ["inline-prompt-agent-prompt", state.agent("agent").prompt(), PORTABLE_SPEC, ["question", "team"], media, answer],
    ["inline-native-prompt-agent-prompt", state.agent("native-agent").prompt(), NATIVE_SPEC, ["ticket"], [], null],
  ] as const) {
    const read = await cards.get(registered(receipt, name));
    expect(hydrated.ref).toEqual(registered(receipt, name));
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
