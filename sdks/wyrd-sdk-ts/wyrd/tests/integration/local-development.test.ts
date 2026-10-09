import { vi } from "vitest";

import { WyrdClient } from "@wyrd/sdk";

import { COMPLETION, work } from "../support/local-development.js";
import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** A local deployment that verifies in real time and routes the Gateway to an upstream answering `hi`. */
const test = serverTest({ provider: () => ({ status: 200, body: COMPLETION }), verificationRuntime: true });

test("admin key completes the local workflow", async ({ server }) => {
  const client = WyrdClient.connect({
    serverUrl: server.baseUrl,
    credential: server.tenantAdminKey(),
    grpcUrl: server.grpcUrl,
  });

  const worked = await work(client);

  await worked.traces.shutdown();
});
