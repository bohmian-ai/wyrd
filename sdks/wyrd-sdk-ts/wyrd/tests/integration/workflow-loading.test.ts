import { resolve } from "node:path";

import { expect, vi } from "vitest";

import {
  type CardRef,
  Cards,
  type RegisteredCard,
  type RegistrationReceipt,
  Workflow,
  type WorkflowSelector,
} from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The shared Workflow loading fixtures; their README says what each proves. */
function workflowFixture(path: string): string {
  return resolve(import.meta.dirname, "../../../../../tests/fixtures/workflow-loading", path);
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

/** The registered reference of every Card a registration registered, keyed by Card name. */
function refs(...receipts: RegistrationReceipt[]): Record<string, CardRef> {
  return Object.fromEntries(
    receipts.flatMap((receipt) => receipt.outcomes.map((outcome) => [outcome.card_ref.name, outcome.card_ref])),
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
 * and of an outsider who cannot read Cards.
 */
const test = serverTest().extend<{
  team: Record<string, CardRef>;
  applied: Record<string, CardRef>;
  readerKey: string;
  outsiderKey: string;
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
      const cards = Cards.connect();
      const applied = { ...team, ...refs(await cards.registerFromPath(workflowFixture("mixed/workflow.yaml"))) };
      await cards.registerFromPath(workflowFixture("team-v2/security.yaml"));
      await use(applied);
    },
    { scope: "file" },
  ],
  readerKey: [
    async ({ server }, use) => use(server.scopedApiKey("workflow_reader", ["cards:read"])),
    { scope: "file" },
  ],
  outsiderKey: [
    async ({ server }, use) => use(server.scopedApiKey("workflow_outsider", ["bifrost_query:read"])),
    { scope: "file" },
  ],
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

test("gateway workflow without credentials is refused before any step", async () => {
  vi.stubEnv("WYRD_API_KEY", undefined);
  const example = await Workflow.fromPath(EXAMPLE);

  expect(example.stepIds).toEqual(["security", "correctness", "final_review"]);
  await expect(example.run({ code: "diff" })).rejects.toMatchObject({ code: "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE" });
});

test("registry refs resolve through the registry", async ({ team: _, readerKey }) => {
  vi.stubEnv("WYRD_API_KEY", readerKey);

  const run = await (await Workflow.fromPath(workflowFixture("mixed/workflow.yaml"))).run({ code: "diff" });

  expect(run).toMatchObject({ status: "succeeded", outputs: { review: REGISTERED_REVIEW } });
});

test.for([
  ["no credential", undefined, "WYRD_CLIENT_401_NO_CREDENTIALS"],
  ["a credential that cannot read cards", "outsider", "WYRD_PERMISSION_403_DENIED_RBAC"],
] as const)("registry refs with %s are refused", async ([, caller, code], { team: _, outsiderKey }) => {
  vi.stubEnv("WYRD_API_KEY", caller && outsiderKey);

  await expect(Workflow.fromPath(workflowFixture("mixed/workflow.yaml"))).rejects.toMatchObject({ code });
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
  const reader = Cards.connect({ credential: readerKey });
  const agents = ["security-reviewer", "correctness-reviewer", "final-reviewer"].map((name) => applied[name]);

  const stored = await reader.get(applied["code-review"] as CardRef);

  expect(stored).toMatchObject({ kind: "Workflow", spec: { steps: agents.map((target) => ({ action: { target } })) } });
  expect(outbound(stored)).toEqual(
    [...agents].sort((left, right) => (left?.name ?? "").localeCompare(right?.name ?? "")),
  );
  for (const [agent, prompt] of [
    ["security-reviewer", "security-review-prompt"],
    ["correctness-reviewer", "correctness-review-prompt"],
    ["final-reviewer", "final-review-prompt"],
  ] as const) {
    const storedAgent = await reader.get(applied[agent] as CardRef);
    expect(storedAgent.kind === "Agent" && storedAgent.spec.prompt, agent).toEqual(applied[prompt]);
    expect(outbound(storedAgent), agent).toEqual([applied[prompt]]);
  }
});

test.for([
  ["identity", () => CODE_REVIEW],
  ["uid", (applied: Record<string, CardRef>) => ({ uid: applied["code-review"]?.uid ?? "" })],
] as const)("loaded workflow by %s runs its pinned cards", async ([, selector], { applied, readerKey }) => {
  const workflow = await Cards.connect({ credential: readerKey }).workflow.load(selector(applied));

  const run = await workflow.run({ code: "diff" });

  expect(run).toMatchObject({
    status: "succeeded",
    outputs: { review: REGISTERED_REVIEW },
    workflow: { uid: applied["code-review"]?.uid },
  });
  expect(run.steps.final_review?.text).toBe(REGISTERED_REVIEW);
});

/** One Workflow selector `workflow.load` refuses, who asks, and the catalog code. */
type Refusal = readonly [string, (applied: Record<string, CardRef>) => WorkflowSelector, "reader" | "outsider", string];

// Malformed selectors are refused before any registry read: the outsider
// cannot read Cards, so a read would fail with the RBAC denial instead.
test.for<Refusal>([
  [
    "a mixed selector",
    // The WorkflowSelector type rejects this mix; the cast proves the runtime does too.
    (applied) => ({ uid: applied["code-review"]?.uid, space: "workflow-loading" }) as unknown as WorkflowSelector,
    "outsider",
    "WYRD_WORKFLOW_400_INVALID_CARD_REF",
  ],
  ["a malformed uid", () => ({ uid: "not-a-uid" }), "outsider", "WYRD_WORKFLOW_400_INVALID_CARD_REF"],
  [
    "a malformed space",
    () => ({ ...CODE_REVIEW, space: "Not A Space" }),
    "outsider",
    "WYRD_WORKFLOW_400_INVALID_CARD_REF",
  ],
  [
    "a malformed name",
    () => ({ ...CODE_REVIEW, name: "not a name" }),
    "outsider",
    "WYRD_WORKFLOW_400_INVALID_CARD_REF",
  ],
  ["a version range", () => ({ ...CODE_REVIEW, version: "^1.0.0" }), "outsider", "WYRD_WORKFLOW_400_INVALID_CARD_REF"],
  ["a partial version", () => ({ ...CODE_REVIEW, version: "1.0" }), "outsider", "WYRD_WORKFLOW_400_INVALID_CARD_REF"],
  [
    "an agent uid",
    (applied) => ({ uid: applied["security-reviewer"]?.uid ?? "" }),
    "reader",
    "WYRD_REGISTRY_404_CARD_NOT_FOUND",
  ],
  [
    "an outsider",
    (applied) => ({ uid: applied["code-review"]?.uid ?? "" }),
    "outsider",
    "WYRD_PERMISSION_403_DENIED_RBAC",
  ],
])("loading %s is refused", async ([, selector, caller, code], { applied, readerKey, outsiderKey }) => {
  const cards = Cards.connect({ credential: caller === "reader" ? readerKey : outsiderKey });

  await expect(cards.workflow.load(selector(applied))).rejects.toMatchObject({ code });
});
