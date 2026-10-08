import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Bifrost, Cards, WyrdClient, WyrdState } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { fixture, serverTest } from "./server.js";

/**
 * `test` with the `observed-service` graph registered and hydrated once,
 * the Service's own Card-scoped key, a Bifrost reader, and a fresh state
 * per test that acts as that Service. Only the Service key's Card scope
 * covers the Agent and Model a run observes.
 */
export const observedServiceTest = serverTest().extend<{
  bundle: string;
  serviceKey: string;
  reader: Bifrost;
  state: WyrdState;
}>({
  bundle: [
    async ({ server: _ }, use) => {
      const cards = Cards.connect();
      await cards.registerFromPath(fixture("cards/observe_a_run/observed-model.yaml"));
      const { root } = await cards.registerFromPath(fixture("cards/observe_a_run/observed-service.yaml"));
      const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-observed-"));
      await cards.hydrate(root, bundle);
      await use(bundle);
    },
    { scope: "file" },
  ],
  serviceKey: [
    async ({ bundle: _ }, use) => {
      const issued = await cli.issueKey({
        kind: "Service",
        name: "observed-service",
        version: "1.0.0",
        space: "default",
      });
      await use(issued.key);
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
