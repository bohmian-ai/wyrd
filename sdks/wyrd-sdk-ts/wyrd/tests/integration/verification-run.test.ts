import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import {
  Cards,
  type StartVerificationRunRequest,
  Verification,
  WyrdError,
} from "@wyrd/sdk";

/** Write a ready Custom Drift Verifier and a Service bound to it on a schedule. */
function writeBoundService(root: string): [string, string] {
  const verifier = join(root, "verifier.yaml");
  writeFileSync(
    verifier,
    `apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ts-run-drift
  version: 1.0.0
  space: default
spec:
  implementation:
    kind: drift
    spec:
      method: Custom
      signal:
        kind: Metric
        name: score
      condition:
        kind: Statistical
      profile:
        kind: Custom
        metric_name: score
        baseline_value: 1.0
        alert_threshold: 0.5
`,
  );
  const service = join(root, "service.yaml");
  writeFileSync(
    service,
    `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ts-run-service
  version: 1.0.0
  space: default
spec:
  verified_by:
    - verifier:
        kind: Verifier
        name: ts-run-drift
        version: 1.0.0
        space: default
      runs_on:
        kind: schedule
        cron: "0 2 * * *"
`,
  );
  return [verifier, service];
}

/** Build a manual Drift run request for one binding over a window ending at `end`. */
function runRequest(bindingId: string, end: string): StartVerificationRunRequest {
  return {
    target: { kind: "binding", binding_id: bindingId },
    input: { kind: "drift_window", start: "2026-09-17T00:00:00Z", end },
  };
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

describe("manual verification journey", () => {
  it("reads a binding, starts a keyed run, and reads the run back", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-verification-"));
    const [verifierPath, servicePath] = writeBoundService(root);
    const server = startTestServer();
    try {
      const cards = Cards.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });
      await cards.registerFromPath(verifierPath);
      const service = (await cards.registerFromPath(servicePath)).root;
      const bindingIds = (await cards.get(service)).status?.verification?.binding_ids;
      expect(bindingIds).toHaveLength(1);
      const bindingId = bindingIds?.[0] ?? "";

      const writer = Verification.connect({
        serverUrl: server.baseUrl,
        credential: server.credentialRegisteredService(
          "default/Service/ts-run-service@1.0.0",
          ["writer"],
        ),
      });
      const binding = await writer.getBinding(bindingId);
      expect(binding.binding_id).toBe(bindingId);
      expect(binding.subject_card_uid).toBe(service.uid);
      expect(binding.readiness).toBe("ready");

      const request = runRequest(bindingId, "2026-09-17T01:00:00Z");
      const runId = await writer.startRun(request, { idempotencyKey: "ts-journey-0001" });
      expect(await writer.startRun(request, { idempotencyKey: "ts-journey-0001" })).toBe(
        runId,
      );
      const run = await writer.getRun(runId);
      expect(run.run_id).toBe(runId);
      expect(run.requested_by_principal_id).toBeTruthy();

      const conflict = await rejection(
        writer.startRun(runRequest(bindingId, "2026-09-17T02:00:00Z"), {
          idempotencyKey: "ts-journey-0001",
        }),
      );
      expect(conflict.code).toBe("WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");
      const inverted = await rejection(
        writer.startRun(runRequest(bindingId, "2026-09-16T00:00:00Z")),
      );
      expect(inverted.code).toBe("WYRD_VERIFICATION_400_INVALID_WINDOW");
      const malformed = await rejection(writer.getRun("not-a-uuid"));
      expect(malformed.code).toBe("WYRD_SPEC_400_VALIDATION");

      const reader = Verification.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_run_reader", ["cards:read"]),
      });
      const denied = await rejection(reader.startRun(request));
      expect(denied.status).toBe(403);
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
