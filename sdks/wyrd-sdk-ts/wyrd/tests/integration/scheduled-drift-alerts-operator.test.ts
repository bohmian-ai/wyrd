import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { Cards, OperatorConnections, type RegistrationReceipt, WyrdState } from "@wyrd/sdk";

import { Receiver } from "../support/http-receiver.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** The bearer token the `on-call-hooks` connection sends to the hook. */
const ON_CALL_TOKEN = "on-call-hook-token";

/**
 * The latency watch Service, registered after its baseline and after the
 * `on-call-hooks` connection that gives its path-only Operator an origin.
 */
const test = serverTest({ verificationRuntime: true }).extend<{
  hooks: Receiver;
  cards: Cards;
  watch: RegistrationReceipt;
}>({
  hooks: [
    async ({}, use) => {
      const hooks = await Receiver.start();
      await use(hooks);
      await hooks.close();
    },
    { scope: "file" },
  ],
  cards: [async ({ server: _ }, use) => use(Cards.connect()), { scope: "file" }],
  watch: [
    async ({ hooks, cards }, use) => {
      await OperatorConnections.connect().create({
        provider: "http",
        name: "on-call-hooks",
        origin: hooks.url,
        auth: { scheme: "bearer", token: ON_CALL_TOKEN },
      });
      await cards.registerFromPath(fixture("cards/latency_baseline/latency-baseline.yaml"));
      await use(await cards.registerFromPath(fixture("cards/scheduled_drift_alerts_operator/latency-watch.yaml")));
    },
    { scope: "file" },
  ],
});

test("failed schedule alerts its operator on the connection origin", async ({ server, hooks, cards, watch }) => {
  const latencyShift = watch.outcomes.find((outcome) => outcome.card_ref.name === "latency-shift");
  server.waitForBaseline(latencyShift?.card_ref.uid ?? "", 90_000);
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-latency-watch-"));
  await cards.hydrate(watch.root, bundle);
  const state = WyrdState.fromPath(bundle);
  await state.startBifrost({
    credential: server.credentialRegisteredService("default/Service/latency-watch@1.0.0", []),
  });
  const run = state.run();
  for (let request = 0; request < 100; request += 1) {
    run.observe.drift({ latency: 99 });
  }
  await state.shutdown();
  server.flushBifrost();

  const [binding] = (await cards.get(watch.root)).status?.verification?.binding_ids ?? [];
  server.makeBindingDue(binding ?? "");

  const [alert] = await hooks.requests(1);
  expect(alert).toMatchObject({
    method: "POST",
    path: "/hooks/latency-shift",
    headers: { authorization: `Bearer ${ON_CALL_TOKEN}` },
  });
});

test("path only operator without connection is refused", async ({ cards }) => {
  await expect(cards.registerFromPath(fixture("invalid/operator-path-without-connection.yaml"))).rejects.toMatchObject({
    code: "WYRD_SPEC_400_INVALID_OPERATOR",
  });
});
