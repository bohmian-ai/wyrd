import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import {
  Cards,
  type RegistrationReceipt,
  Workflow,
  type WorkflowSelector,
  WyrdError,
} from "@wyrd/sdk";

/** Repository root, four levels above this file. */
const REPO = resolve(import.meta.dirname, "../../../../..");

/** Shared Workflow loading fixtures; see their README. */
const FIXTURES = join(REPO, "tests/fixtures/workflow-loading");

/** Capture the structured catalog error a promise rejects with. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/** The UID of every Card a registration registered, keyed by Card name. */
function uids(receipt: RegistrationReceipt): Record<string, string> {
  return Object.fromEntries(
    receipt.outcomes.map((outcome) => [outcome.card_ref.name, outcome.card_ref.uid ?? ""]),
  );
}

describe("Workflow loading", () => {
  it("workflow loading journey", async () => {
    const server = startTestServer();
    try {
      const writer = Cards.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
      const readerKey = server.scopedApiKey("ts_workflow_reader", ["cards:read"]);
      const reader = Cards.connect({ serverUrl: server.baseUrl, credential: readerKey });
      const outsiderKey = server.scopedApiKey("ts_workflow_outsider", ["bifrost_query:read"]);
      const outsider = Cards.connect({ serverUrl: server.baseUrl, credential: outsiderKey });

      // Workflow.fromPath reads credentials from the environment, so start with none.
      process.env.WYRD_SERVER_URL = server.baseUrl;
      process.env.WYRD_CONFIG_HOME = mkdtempSync(join(tmpdir(), "wyrd-config-"));
      delete process.env.WYRD_API_KEY;
      delete process.env.WYRD_ACCESS_TOKEN;

      // 1. A wholly local Workflow file loads without credentials.
      const local = await Workflow.fromPath(
        join(REPO, "examples/workflows/code-review/workflow.yaml"),
      );
      expect(local.stepIds).toEqual(["security", "correctness", "final_review"]);

      // 2. The team registers its reviewer Agents.
      const team = {
        ...uids(await writer.registerFromPath(join(FIXTURES, "team/security.yaml"))),
        ...uids(await writer.registerFromPath(join(FIXTURES, "team/correctness.yaml"))),
      };

      // 3. A file referencing registered Agents needs a credential that can read them.
      const mixed = join(FIXTURES, "mixed/workflow.yaml");
      expect((await rejection(Workflow.fromPath(mixed))).code).toBe(
        "WYRD_CLIENT_401_NO_CREDENTIALS",
      );

      process.env.WYRD_API_KEY = outsiderKey;
      expect((await rejection(Workflow.fromPath(mixed))).code).toBe(
        "WYRD_PERMISSION_403_DENIED_RBAC",
      );

      process.env.WYRD_API_KEY = readerKey;
      const authored = await Workflow.fromPath(mixed);
      expect(authored.stepIds).toEqual(["security", "correctness", "final_review"]);

      // 4. A local sibling and a registered Agent with the same identity load side by side.
      const shadowed = await Workflow.fromPath(join(FIXTURES, "shadowed/workflow.yaml"));
      expect(shadowed.stepIds).toEqual([
        "security",
        "registered_security",
        "correctness",
        "final_review",
      ]);

      // 5. A reference to a deleted Card is refused.
      const retired = await writer.registerFromPath(join(FIXTURES, "retired/retired-prompt.yaml"));
      await writer.delete(retired.root);
      expect(
        (await rejection(Workflow.fromPath(join(FIXTURES, "retired/workflow.yaml")))).code,
      ).toBe("WYRD_REGISTRY_404_CARD_NOT_FOUND");

      // 6. Apply the mixed Workflow and load it back by identity and by UID.
      const workflowUid = uids(await writer.registerFromPath(mixed))["code-review"] ?? "";
      const byIdentity = await reader.workflow.load({
        space: "workflow-loading",
        name: "code-review",
        version: "1.0.0",
      });
      const byUid = await reader.workflow.load({ uid: workflowUid });
      for (const workflow of [byIdentity, byUid]) {
        expect(workflow.stepIds).toEqual(["security", "correctness", "final_review"]);
        // These steps call models through the Wyrd gateway, which a plain
        // run does not have yet; the run is refused before any step starts.
        expect((await rejection(workflow.run({ code: "diff" }))).code).toBe(
          "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
        );
      }

      // 7. Mixed, wrong-kind, and unauthorized selectors are refused.
      // The WorkflowSelector type already rejects this mix; the cast proves
      // the runtime refuses it too, for callers without type checking.
      const mixedSelector = {
        uid: workflowUid,
        space: "workflow-loading",
      } as unknown as WorkflowSelector;
      expect((await rejection(reader.workflow.load(mixedSelector))).code).toBe(
        "WYRD_SPEC_400_VALIDATION",
      );
      // An Agent's UID names no Workflow.
      expect(
        (await rejection(reader.workflow.load({ uid: team["security-reviewer"] ?? "" }))).code,
      ).toBe("WYRD_REGISTRY_404_CARD_NOT_FOUND");
      expect((await rejection(outsider.workflow.load({ uid: workflowUid }))).code).toBe(
        "WYRD_PERMISSION_403_DENIED_RBAC",
      );
    } finally {
      delete process.env.WYRD_SERVER_URL;
      delete process.env.WYRD_CONFIG_HOME;
      delete process.env.WYRD_API_KEY;
      server.shutdown();
    }
  });
});
