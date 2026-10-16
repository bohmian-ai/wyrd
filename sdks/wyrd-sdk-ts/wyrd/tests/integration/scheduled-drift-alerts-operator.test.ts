import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import { expect, vi } from "vitest";

import {
  Cards,
  OperatorConnections,
  type RegistrationReceipt,
  type VerificationResult,
  WyrdClient,
  WyrdState,
} from "wyrd-ai";
import { cli, type NativeWyrdTestServer } from "@wyrd/testing";

import { Receiver } from "../support/http-receiver.js";
import { binding, fixture, registered, scheduledResult, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 240_000 });

/** The bearer token the `on-call-hooks` connection sends to the hook. */
const ON_CALL_TOKEN = "on-call-hook-token";

/** How long the story waits for the baseline to fit, for the result, and for the alert. */
const WAIT_MS = 90_000;

/** How long the story listens for an alert that must not arrive. */
const QUIET_MS = 5_000;

/**
 * The latency watch Service on a fresh server per test, so each window is
 * judged alone, registered after its
 * baseline and after the `on-call-hooks` connection that gives its path-only
 * Operator an origin, with the `latency-shift` baseline already fitted by the
 * real fitter.
 */
const test = serverTest({ verificationRuntime: true, perTest: true }).extend<{
  hooks: Receiver;
  cards: Cards;
  watch: RegistrationReceipt;
}>({
  hooks: async ({}, use) => {
    const hooks = await Receiver.start(() => ({ status: 200 }));
    await use(hooks);
    await hooks.close();
  },
  cards: async ({ server: _ }, use) => use(await Cards.connect()),
  watch: async ({ server, hooks, cards }, use) => {
    await (await OperatorConnections.connect()).create({
      provider: "http",
      name: "on-call-hooks",
      origin: hooks.url,
      auth: { scheme: "bearer", token: ON_CALL_TOKEN },
    });
    await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
    const watch = await cards.registerFromPath(fixture("cards/scheduled_drift_alerts_operator/latency-watch.yaml"));
    server.waitForBaseline(registered(watch, "latency-shift").uid, WAIT_MS);
    await use(watch);
  },
});

/**
 * Submit every latency as the Service, assert each was recorded for the
 * schedule rather than judged, and drain the samples.
 */
async function submit(server: NativeWyrdTestServer, cards: Cards, watch: RegistrationReceipt, latencies: number[]) {
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-latency-watch-"));
  await cards.hydrate(watch.root, bundle);
  const watchKey = await cli.issueKey({ kind: "Service", name: "latency-watch", version: "1.0.0", space: "default" });
  const state = WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: watchKey.key }) });
  await state.startBifrost();
  const run = state.run();
  for (const latency of latencies) {
    expect(await run.observe.submit("latency-shift", { latency })).toEqual({ kind: "recorded" });
  }
  await state.shutdown();
  server.flushBifrost();
}

/** Make the Service's one binding due and read back the result the real scheduler, queue, and runner retain. */
async function judge(
  server: NativeWyrdTestServer,
  cards: Cards,
  watch: RegistrationReceipt,
): Promise<VerificationResult> {
  const id = await binding(cards, watch.root);
  server.makeBindingDue(id);
  return scheduledResult(server, id, WAIT_MS);
}

/** Assert the hook receives nothing for {@link QUIET_MS}. */
async function assertQuiet(hooks: Receiver, already = 0): Promise<void> {
  await sleep(QUIET_MS);
  expect(hooks.received.slice(already)).toEqual([]);
}

test("failed schedule alerts its operator on the connection origin", async ({ server, hooks, cards, watch }) => {
  await submit(
    server,
    cards,
    watch,
    Array.from({ length: 100 }, () => 99),
  );
  expect(hooks.received, "submitting alone runs no check and alerts no one").toEqual([]);

  const result = await judge(server, cards, watch);

  expect(result.outcome.verdict).toBe("failed");
  expect(result.subject).toEqual(watch.root);
  expect(result.verifier.name).toBe("latency-shift");
  expect(result.run_id ?? null, "a scheduled execution has no Run").toBeNull();
  expect(result.outcome.requirements).toHaveLength(1);
  const [requirement] = result.outcome.requirements;
  expect([requirement?.id, requirement?.outcome]).toEqual(["latency-unshifted", "failed"]);
  expect(requirement?.provenance).toMatchObject({ kind: "drift", window: { start: expect.any(String) } });
  expect(requirement?.check).toMatchObject({ signal: { baseline_ref: { name: "latency-baseline" } } });
  const latency = requirement?.detail?.drift.features.latency;
  expect(latency?.score ?? 0).toBeGreaterThan(0.25);

  const [alert] = await hooks.requests(1);
  expect(alert).toMatchObject({
    method: "POST",
    path: "/hooks/latency-shift",
    headers: { authorization: `Bearer ${ON_CALL_TOKEN}` },
  });
  expect(alert?.body, "the alert names the failed execution").toContain(result.execution_id);
  await assertQuiet(hooks, 1);
});

test("passing window alerts no one", async ({ server, hooks, cards, watch }) => {
  await submit(
    server,
    cards,
    watch,
    Array.from({ length: 100 }, (_, row) => (row * 37) % 100),
  );

  const result = await judge(server, cards, watch);

  expect(result.outcome.verdict).toBe("passed");
  await assertQuiet(hooks);
});

test("empty window is inconclusive and alerts no one", async ({ server, hooks, cards, watch }) => {
  const result = await judge(server, cards, watch);

  expect(result.outcome.verdict).toBe("inconclusive");
  await assertQuiet(hooks);
});

test("path only operator without connection is refused", async ({ server: _ }) => {
  const cards = await Cards.connect();
  await expect(cards.registerFromPath(fixture("invalid/operator-path-without-connection.yaml"))).rejects.toMatchObject({
    code: "WYRD_SPEC_400_INVALID_OPERATOR",
  });
});
