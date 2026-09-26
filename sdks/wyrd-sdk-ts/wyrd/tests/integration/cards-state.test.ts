import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Cards, WyrdError, WyrdState } from "@wyrd/sdk";

const PROMPT_ARTIFACT = "shared-prompt-artifact";

/**
 * Write a Service graph of one Agent and its Prompt, with one Prompt artifact.
 *
 * An artifact-bearing Card registers alone, so the Prompt is registered first
 * and the Service composite references it by exact identity.
 */
function writeServiceGraph(root: string): string {
  const digest = createHash("sha256").update(PROMPT_ARTIFACT).digest("base64");
  writeFileSync(join(root, "shared-prompt.txt"), PROMPT_ARTIFACT);
  writeFileSync(
    join(root, "shared-prompt.yaml"),
    `apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: ts-shared-prompt
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-4o
  messages: [hello]
artifacts:
  - relative_path: shared-prompt.txt
    sha256: ${digest}
    size_bytes: ${PROMPT_ARTIFACT.length}
    content_type: text/plain
`,
  );
  writeFileSync(
    join(root, "agent.yaml"),
    `apiVersion: wyrd/v1
kind: Agent
metadata:
  name: ts-agent
  version: 1.0.0
  space: default
spec:
  prompt:
    kind: Prompt
    name: ts-shared-prompt
    version: 1.0.0
    space: default
  run_config:
    max_iterations: 2
    timeout_ms: 1000
`,
  );
  const service = join(root, "service.yaml");
  writeFileSync(
    service,
    `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ts-hydrated-service
  version: 1.0.0
  space: default
spec:
  service_type: agent
  components:
    - alias: agent
      ref: ./agent.yaml
    - alias: prompt
      ref:
        kind: Prompt
        name: ts-shared-prompt
        version: 1.0.0
        space: default
`,
  );
  return service;
}

/**
 * Write an eval Verifier and a standalone Agent bound to it.
 *
 * The Agent is the smallest binding owner: one Agent-level binding running the
 * Verifier on an inline `observations_ready` Trigger. It reuses the shared
 * Prompt, so it registers after the Prompt. Returns the Verifier and Agent
 * paths in registration order.
 */
function writeBoundAgent(root: string): [string, string] {
  const verifier = join(root, "verifier.yaml");
  writeFileSync(
    verifier,
    `apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ts-eval
  version: 1.0.0
  space: default
spec:
  implementation:
    kind: eval
    spec:
      tasks: {}
`,
  );
  const agent = join(root, "bound-agent.yaml");
  writeFileSync(
    agent,
    `apiVersion: wyrd/v1
kind: Agent
metadata:
  name: ts-bound-agent
  version: 1.0.0
  space: default
spec:
  prompt:
    kind: Prompt
    name: ts-shared-prompt
    version: 1.0.0
    space: default
  verified_by:
    - verifier:
        kind: Verifier
        name: ts-eval
        version: 1.0.0
        space: default
      runs_on:
        kind: observations_ready
`,
  );
  return [verifier, agent];
}

/** Await a promise expected to reject with a structured catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/** Capture a synchronous structured catalog error. */
function thrown(action: () => unknown): WyrdError {
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return error as WyrdError;
  }
  throw new Error("expected a WyrdError");
}

describe("Card and WyrdState journey", () => {
  it("registers, reads, hydrates, and loads offline state", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-cards-"));
    const service = writeServiceGraph(root);
    const bundle = join(root, "bundle");
    const server = startTestServer();
    let serverRunning = true;
    try {
      const cards = Cards.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });

      const prompt = await cards.registerFromPath(join(root, "shared-prompt.yaml"));
      expect(prompt.root.kind).toBe("Prompt");

      const receipt = await cards.registerFromPath(service);
      expect(receipt.root.kind).toBe("Service");
      expect(receipt.root.uid).toBeTruthy();
      expect(receipt.outcomes).toHaveLength(2);

      const replay = await cards.registerFromPath(service);
      expect(replay.root.uid).toBe(receipt.root.uid);

      const card = await cards.get(receipt.root);
      expect(card.kind).toBe("Service");
      expect(card.metadata.name).toBe("ts-hydrated-service");

      const prompts = await cards.list({ kind: "Prompt", name: "ts-shared-prompt" });
      expect(prompts.items.map((item) => item.name)).toEqual(["ts-shared-prompt"]);

      const invalidRef = await rejection(cards.get("not-a-card-ref"));
      expect(invalidRef.code).toBe("WYRD_SPEC_400_VALIDATION");

      const [verifier, boundAgent] = writeBoundAgent(root);
      await cards.registerFromPath(verifier);
      const bound = await cards.registerFromPath(boundAgent);
      const bindingIds = (await cards.get(bound.root)).status?.verification?.binding_ids;
      expect(bindingIds).toEqual([expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-7/)]);
      const rebound = await cards.registerFromPath(boundAgent);
      expect((await cards.get(rebound.root)).status?.verification?.binding_ids).toEqual(
        bindingIds,
      );

      const denied = Cards.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_cards_denied", ["bifrost_query:read"]),
      });
      const deniedError = await rejection(denied.registerFromPath(service));
      expect(deniedError.status).toBe(403);

      const summary = await cards.hydrate(receipt.root, bundle);
      expect(summary.mode).toBe("complete");
      expect(summary.card_count).toBe(3);
      expect(summary.root.uid).toBe(receipt.root.uid);
      const metadataBundle = join(root, "metadata-bundle");
      const metadataSummary = await cards.hydrate(receipt.root, metadataBundle, {
        metadataOnly: true,
      });
      expect(metadataSummary.mode).toBe("metadata");

      server.shutdown();
      serverRunning = false;

      const state = WyrdState.fromPath(bundle);
      expect(state.rootRef.uid).toBe(receipt.root.uid);
      expect(state.aliases).toEqual(expect.arrayContaining(["agent", "prompt", "root"]));
      expect(state.card("agent").kind).toBe("Agent");
      expect(state.cardRef("prompt").uid).toBeTruthy();
      const [artifact] = state.artifacts("prompt");
      expect(artifact?.relative_path).toBe("shared-prompt.txt");
      expect(readFileSync(artifact?.local_path ?? "", "utf8")).toBe(PROMPT_ARTIFACT);

      expect(thrown(() => state.card("missing")).code).toBe("WYRD_SDK_404_UNKNOWN_ALIAS");
      expect(thrown(() => WyrdState.fromPath(metadataBundle)).code).toBe(
        "WYRD_SDK_400_UNHYDRATED_ARTIFACT",
      );
    } finally {
      if (serverRunning) {
        server.shutdown();
      }
    }
  }, 60_000);
});
