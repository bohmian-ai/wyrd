import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Bifrost, Cards, WyrdClient, WyrdState } from "wyrd-ai";
import { fixture, runtimeAccount, type ServerOptions, serverTest } from "./server.js";

/**
 * `test` on a server started with `options`, with the `observed-service`
 * graph declared and hydrated once, a key of the Service's runtime account, a
 * Bifrost reader, and a fresh state per test that acts as that Service. Only
 * the Service's Card scope covers the Agent and Model a run observes.
 */
export const observedService = (options: ServerOptions = {}) =>
  serverTest(options).extend<{
    bundle: string;
    serviceKey: string;
    reader: Bifrost;
    state: WyrdState;
  }>({
    bundle: [
      async ({ server: _ }, use) => {
        const cards = await Cards.connect();
        await cards.declareFromPath(fixture("cards/observe_a_run/observed-model.yaml"));
        const { root } = await cards.declareFromPath(fixture("cards/observe_a_run/observed-service.yaml"));
        const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-observed-"));
        await cards.pull(root, bundle);
        await use(bundle);
      },
      { scope: "file" },
    ],
    serviceKey: [
      async ({ bundle: _ }, use) => {
        const { key } = await runtimeAccount({ kind: "Service", name: "observed-service" });
        await use(key);
      },
      { scope: "file" },
    ],
    reader: [
      async ({ server: _ }, use) => {
        const reader = await Bifrost.connect();
        await use(reader);
        await reader.shutdown();
      },
      { scope: "file" },
    ],
    state: async ({ bundle, serviceKey }, use) =>
      use(WyrdState.fromPath(bundle, { client: WyrdClient.connect({ credential: serviceKey }) })),
  });

/** {@link observedService} on a server with its default upstreams. */
export const observedServiceTest = observedService();
