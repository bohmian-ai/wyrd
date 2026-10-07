import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";

import { Cards, type RegistrationReceipt, WyrdState, cli } from "@wyrd/sdk";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const DESK = fixture("cards/register_and_hydrate/support-desk.yaml");

/** The support desk graph, registered after the artifact-bearing Model it references. */
const test = serverTest().extend<{
  cards: Cards;
  model: RegistrationReceipt;
  desk: RegistrationReceipt;
}>({
  cards: [async ({ server: _ }, use) => use(Cards.connect()), { scope: "file" }],
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
  expect(state.card("agent").kind).toBe("Agent");
  const [artifact] = state.artifacts("model");
  expect(artifact?.relative_path).toBe("support-model.bin");
  expect(readFileSync(artifact?.local_path ?? "")).toEqual(
    readFileSync(fixture("cards/register_and_hydrate/support-model.bin")),
  );
});

test("registering the graph again is idempotent", async ({ cards, desk }) => {
  const bindings = (await cards.get(desk.root)).status?.verification?.binding_ids;

  const again = await cards.registerFromPath(DESK);

  expect(again.root).toEqual(desk.root);
  expect((await cards.get(again.root)).status?.verification?.binding_ids).toEqual(bindings);
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
  expect(verifier?.kind === "Verifier" && verifier.spec.implementation.kind).toBe("eval");
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
  const reader = Cards.connect({ credential: server.scopedApiKey("card_reader", ["cards:read"]) });

  await expect(reader.registerFromPath(DESK)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("cli apply and get round trip the graph", async ({ desk }) => {
  const applied = await cli.apply(DESK);
  const bundle = bundleDir();

  const summary = await cli.get({ kind: "Service", space: "default", name: "support-desk", version: "1.0.0" }, bundle);

  expect(applied.root).toEqual(desk.root);
  expect(summary).toMatchObject({ mode: "complete", root: desk.root });
  expect(WyrdState.fromPath(bundle).rootRef).toEqual(desk.root);
});

test("refused cli command raises its catalog code", async ({ server: _ }) => {
  await expect(
    cli.get({ kind: "Service", space: "default", name: "no-such-service", version: "1.0.0" }, bundleDir()),
  ).rejects.toMatchObject({ code: "WYRD_REGISTRY_404_CARD_NOT_FOUND" });
});

/** The `wyrd` executable the package installs, resolved from its `bin` entry. */
const WYRD_BIN = (() => {
  const root = resolve(import.meta.dirname, "../..");
  const { bin } = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as { bin: { wyrd: string } };
  return join(root, bin.wyrd);
})();

test("installed wyrd executable applies the graph", async ({ desk }) => {
  const { stdout } = await promisify(execFile)(process.execPath, [WYRD_BIN, "apply", DESK, "--format", "json"]);

  expect(JSON.parse(stdout)).toMatchObject({ root: desk.root });
});
