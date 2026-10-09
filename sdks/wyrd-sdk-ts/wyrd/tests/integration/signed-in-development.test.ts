import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import { expect, vi } from "vitest";

import { Bifrost, WyrdClient } from "@wyrd/sdk";

import { Attributed, COMPLETION, exportSpan, invoke, work } from "../support/local-development.js";
import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** Access-token lifetime the workflow outlives. */
const ACCESS_TTL_SECONDS = 2;

/** The test server's own tenant, where alice administers. */
const FIXTURE_TENANT = "test-tenant-1";

/** The local deployment, accepting human sign-in with short-lived access tokens. */
const test = serverTest({
  provider: () => ({ status: 200, body: COMPLETION }),
  verificationRuntime: true,
  humanSso: true,
  accessTtlSeconds: ACCESS_TTL_SECONDS,
});

test("saved login completes the workflow past token expiry", async ({ server }) => {
  server.activateHumanSso();
  const config = mkdtempSync(join(tmpdir(), "wyrd-ts-signed-in-"));
  vi.stubEnv("WYRD_CONFIG_HOME", config);
  vi.stubEnv("WYRD_API_KEY", undefined);
  server.saveHumanLogin(config, FIXTURE_TENANT, "alice", "alice-password");
  const client = WyrdClient.connect({ serverUrl: server.baseUrl, grpcUrl: server.grpcUrl });
  const original = await client.accessToken();

  const worked = await work(client);
  await sleep((ACCESS_TTL_SECONDS + 1) * 1000);

  const lapsed = await fetch(`${server.baseUrl}/v1/cards`, { headers: { authorization: `Bearer ${original}` } });
  expect(lapsed.status).toBe(401);
  expect(await invoke(client)).toBe("hi");
  await exportSpan(worked.traces, worked.agent, "after-expiry");
  const rows = await (await Bifrost.connect({ client })).sql(
    "SELECT card_uid FROM vala.traces.spans WHERE run_id = $1",
    ["after-expiry"],
    Attributed,
  );
  expect(rows).toEqual([{ card_uid: worked.agentUid }]);
  await worked.traces.shutdown();
});
