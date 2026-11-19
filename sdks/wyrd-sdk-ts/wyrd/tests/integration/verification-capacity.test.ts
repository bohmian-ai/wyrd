/**
 * A service verifies through hosted Python while process memory is
 * contended: `verify` waits for memory and then passes, is refused as
 * retryable `503` when memory cannot even hold the waiting request, and ends
 * in `504` at the one deadline from its arrival without a replay; `submit`
 * defers a queued execution until memory frees.
 */
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { NativeWyrdTestServer } from "@wyrd/testing";
import { expect } from "vitest";
import { Cards, type VerificationResult, type VerifierInput, WyrdClient, WyrdState } from "wyrd-ai";

import { declared, fixture, runtimeAccount, serverTest } from "../support/server.js";

/** The direct deadline every story here runs under. */
const VERIFY_TIMEOUT_MS = 2_000;

/** Bytes left free while an operation should wait: short of one session envelope. */
const SHORT_OF_A_SESSION = 64 << 20;

/** Bytes of the answer the full-memory refusal sends. */
const REQUEST_BYTES = 256 << 10;

/** How long a story waits for a queued execution or a waiter to appear. */
const WAIT_MS = 60_000;

/** A fresh deployment hosting Python Verifiers under a short deadline, with `capacity-desk` hydrated as its own key. */
const test = serverTest({ verificationRuntime: true, verifyTimeoutMs: VERIFY_TIMEOUT_MS, perTest: true }).extend<{
  state: WyrdState;
}>({
  state: async ({ server: _ }, use) => {
    const cards = await Cards.connect();
    const receipt = await cards.declareFromPath(fixture("cards/verification_capacity/capacity-desk.yaml"));
    const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-capacity-desk-"));
    await cards.pull(receipt.root, bundle);
    const root = declared(receipt);
    const { key } = await runtimeAccount({ kind: "Service", name: root.name });
    const state = WyrdState.fromPath(bundle, {
      client: WyrdClient.connect({ credential: key, workloadUid: root.uid }),
    });
    await state.startBifrost();
    await use(state);
    await state.shutdown();
  },
});

/** Verify the agent's `answer` with `adds-one` now. */
function verifyAnswer(state: WyrdState, answer: VerifierInput = 2): Promise<VerificationResult> {
  return state.run("agent").observe.verify("adds-one", { answer });
}

/** Return once `count` hosted operations wait for process memory. */
function awaitWaiters(server: NativeWyrdTestServer, count: number): void {
  server.waitForHostedWaiters(count, WAIT_MS);
}

test("verify_waits_for_hosted_memory_then_passes", async ({ server, state }) => {
  const held = server.occupyHostedMemory(SHORT_OF_A_SESSION);
  let settled = false;
  const waiting = verifyAnswer(state).finally(() => (settled = true));
  awaitWaiters(server, 1);
  expect(settled, "the verify waits for memory").toBe(false);
  held.release();
  expect((await waiting).outcome.verdict).toBe("passed");
});

test("submit_defers_under_hosted_memory_then_passes", async ({ server, state }) => {
  const held = server.occupyHostedMemory(SHORT_OF_A_SESSION);
  const accepted = await state.run("agent").observe.submit("adds-one", { answer: 2 });
  if (accepted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  awaitWaiters(server, 1);
  held.release();
  expect((await accepted.execution.wait({ timeoutMs: WAIT_MS })).outcome.verdict).toBe("passed");
});

test("verify_refusals_follow_the_arrival_deadline", async ({ server, state }) => {
  const full = server.occupyHostedMemory(0);
  await expect(verifyAnswer(state, "2".repeat(REQUEST_BYTES))).rejects.toMatchObject({
    code: "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
    status: 503,
    details: { reason: "process memory cannot hold the Python operation now" },
  });
  full.release();

  const held = server.occupyHostedMemory(SHORT_OF_A_SESSION);
  const started = performance.now();
  await expect(verifyAnswer(state)).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT",
    status: 504,
  });
  const elapsed = performance.now() - started;
  expect(elapsed, "one wait ends at the deadline, unreplayed").toBeGreaterThanOrEqual(VERIFY_TIMEOUT_MS);
  expect(elapsed, "one wait ends at the deadline, unreplayed").toBeLessThan(2 * VERIFY_TIMEOUT_MS);
  awaitWaiters(server, 0);
  held.release();

  expect((await verifyAnswer(state)).outcome.verdict).toBe("passed");
});
