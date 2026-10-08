import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, vi } from "vitest";

import { type CardRef, Cards, type RegisteredCard, type RegistrationReceipt, Workflow, WyrdClient } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { type RegisteredRef, fixture, registered, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** A Workflow loading fixture; `fixtures/README.md` says what each proves. */
function workflowFixture(path: string): string {
  return fixture(`cards/workflow_loading/${path}`);
}

/** The code-review example Workflow, which calls its models through the Wyrd gateway. */
const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/workflows/code-review/workflow.yaml");

// The fixture Prompts send their Chat request to the built-in `mock` provider,
// which answers with the rendered user message, so each output names the
// Prompt body that ran and what was bound into it.
const LOCAL_REVIEW = "final review of diff | local security review of diff | local correctness review of diff";
const REGISTERED_REVIEW =
  "final review of diff | registered security review of diff | registered correctness review of diff";

/** The `code-review` Workflow's registry identity. */
const CODE_REVIEW = { space: "workflow-loading", name: "code-review", version: "1.0.0" } as const;

/** The registered reference of every Card the receipts registered, keyed by Card name. */
function refs(...receipts: RegistrationReceipt[]): Record<string, RegisteredRef> {
  return Object.fromEntries(
    receipts.flatMap((receipt) =>
      receipt.outcomes.map((outcome) => [outcome.card_ref.name, registered(receipt, outcome.card_ref.name)]),
    ),
  );
}

/** Server-derived outbound relationship targets of a Card, ordered by name. */
function outbound(card: RegisteredCard): CardRef[] {
  return (card.relationships?.outbound_refs ?? [])
    .map((relationship) => relationship.ref)
    .sort((left, right) => left.name.localeCompare(right.name));
}

/**
 * The registered reviewer team, the applied `code-review` Workflow with a
 * newer security reviewer registered after it, and the keys of a Card reader
 * and of a principal holding no Roles.
 */
const test = serverTest().extend<{
  team: Record<string, RegisteredRef>;
  applied: Record<string, RegisteredRef>;
  readerKey: string;
  noRolesKey: string;
}>({
  team: [
    async ({ server: _ }, use) => {
      const cards = Cards.connect();
      await use(
        refs(
          await cards.registerFromPath(workflowFixture("team/security.yaml")),
          await cards.registerFromPath(workflowFixture("team/correctness.yaml")),
        ),
      );
    },
    { scope: "file" },
  ],
  applied: [
    async ({ team }, use) => {
      const applied = { ...team, ...refs(await cli.apply(workflowFixture("mixed/workflow.yaml"))) };
      await Cards.connect().registerFromPath(workflowFixture("team-v2/security.yaml"));
      await use(applied);
    },
    { scope: "file" },
  ],
  readerKey: [
    async ({ server }, use) => use(server.scopedApiKey("workflow_reader", ["cards:read"])),
    { scope: "file" },
  ],
  noRolesKey: [async ({ server }, use) => use(server.scopedApiKey("workflow_no_roles", [])), { scope: "file" }],
});

test("local workflow runs without credentials", async () => {
  vi.stubEnv("WYRD_API_KEY", undefined);
  const local = await Workflow.fromPath(workflowFixture("shadowed/local-workflow.yaml"));

  const run = await local.run({ code: "diff" });

  expect(run).toMatchObject({
    status: "succeeded",
    outputs: { security: "local security review of diff", review: LOCAL_REVIEW },
  });
});

test("text input needs a declared input named input", async () => {
  const local = await Workflow.fromPath(workflowFixture("shadowed/local-workflow.yaml"));

  await expect(local.run("diff")).rejects.toMatchObject({ code: "WYRD_WORKFLOW_422_RUN_REQUEST" });
});

// YAML text has no directory, so its relative Agent targets never resolve.
test("yaml workflow loads without resolving file targets", async () => {
  const yaml = Workflow.fromYaml(readFileSync(workflowFixture("shadowed/local-workflow.yaml"), "utf8"));

  expect([yaml.space, yaml.name, yaml.version]).toEqual(["workflow-loading", "local-review", "1.0.0"]);
  expect(yaml.steps).toEqual(["security", "correctness", "final_review"]);
  await expect(yaml.run({ code: "diff" })).rejects.toMatchObject({ code: "WYRD_WORKFLOW_404_AGENT" });
  expect(() => Workflow.fromYaml("kind: Nope")).toThrow(
    expect.objectContaining({ code: "WYRD_WORKFLOW_422_VALIDATION" }),
  );
});

test("gateway workflow without credentials is refused before any step", async () => {
  vi.stubEnv("WYRD_API_KEY", undefined);
  const example = await Workflow.fromPath(EXAMPLE);

  expect(example.steps).toEqual(["security", "correctness", "final_review"]);
  await expect(example.run({ code: "diff" })).rejects.toMatchObject({ code: "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE" });
});

test("registry refs resolve through the registry", async ({ team: _, readerKey }) => {
  vi.stubEnv("WYRD_API_KEY", undefined);
  const reader = WyrdClient.connect({ credential: readerKey });

  const workflow = await Workflow.fromPath(workflowFixture("mixed/workflow.yaml"), { client: reader });
  const run = await workflow.run({ code: "diff" });

  expect(run).toMatchObject({ status: "succeeded", outputs: { review: REGISTERED_REVIEW } });
});

test("registry refs without read access are refused", async ({ team: _, noRolesKey }) => {
  const noRoles = WyrdClient.connect({ credential: noRolesKey });

  await expect(Workflow.fromPath(workflowFixture("mixed/workflow.yaml"), { client: noRoles })).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("local sibling never satisfies a registry ref", async ({ team: _ }) => {
  const run = await (await Workflow.fromPath(workflowFixture("shadowed/workflow.yaml"))).run({ code: "diff" });

  expect(run).toMatchObject({
    status: "succeeded",
    outputs: {
      security: "local security review of diff",
      registered_security: "registered security review of diff",
      review: LOCAL_REVIEW,
    },
  });
});

test("deleted registry card is refused", async () => {
  const cards = Cards.connect();
  const retired = await cards.registerFromPath(workflowFixture("retired/retired-prompt.yaml"));
  await cards.delete(retired.root);

  await expect(Workflow.fromPath(workflowFixture("retired/workflow.yaml"))).rejects.toMatchObject({
    code: "WYRD_REGISTRY_404_CARD_NOT_FOUND",
  });
});

test("applied workflow stays pinned to its registered cards", async ({ applied, readerKey }) => {
  const reader = Cards.connect({ client: WyrdClient.connect({ credential: readerKey }) });
  const agents = [applied["security-reviewer"], applied["correctness-reviewer"], applied["final-reviewer"]];

  const stored = await reader.get(applied["code-review"]);

  expect(stored).toMatchObject({ kind: "Workflow", spec: { steps: agents.map((target) => ({ action: { target } })) } });
  expect(outbound(stored)).toEqual([...agents].sort((left, right) => left.name.localeCompare(right.name)));
  for (const [agent, prompt] of [
    ["security-reviewer", "security-review-prompt"],
    ["correctness-reviewer", "correctness-review-prompt"],
    ["final-reviewer", "final-review-prompt"],
  ] as const) {
    const storedAgent = await reader.get(applied[agent]);
    expect(storedAgent.kind === "Agent" && storedAgent.spec.prompt, agent).toEqual(applied[prompt]);
    expect(outbound(storedAgent), agent).toEqual([applied[prompt]]);
  }
});

test("loaded workflow runs its pinned cards", async ({ applied, readerKey }) => {
  const reader = Cards.connect({ client: WyrdClient.connect({ credential: readerKey }) });
  const workflow = applied["code-review"];

  for (const selector of [CODE_REVIEW, { uid: workflow.uid }]) {
    const run = await (await reader.workflow.load(selector)).run({ code: "diff" });

    expect(run).toMatchObject({
      status: "succeeded",
      outputs: { review: REGISTERED_REVIEW },
      steps: { final_review: { text: REGISTERED_REVIEW } },
      workflow: { uid: workflow.uid },
    });
  }
});

// An Agent's uid names no Workflow.
test("loading a bad selector is refused", async ({ applied, readerKey }) => {
  const reader = Cards.connect({ client: WyrdClient.connect({ credential: readerKey }) });

  await expect(reader.workflow.load({ uid: applied["security-reviewer"].uid })).rejects.toMatchObject({
    code: "WYRD_REGISTRY_404_CARD_NOT_FOUND",
  });
});
