import { expect, vi } from "vitest";

import { OperatorConnections, WyrdClient } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** Every secret the stories write; none may appear in a view or an error. */
const SECRETS = ["xoxb-ts-sdk-secret", "xoxb-ts-sdk-rotated"] as const;

/** The `ops-slack` connection the stories share. */
const SLACK = { provider: "slack", name: "ops-slack", workspace_id: "T0001", bot_token: SECRETS[0] } as const;

/** Fail when a story secret appears in `value`. */
function expectRedacted(value: unknown): void {
  const text = JSON.stringify(value) + String(value);
  for (const secret of SECRETS) {
    expect(text).not.toContain(secret);
  }
}

/** The administrator's connection handle and the id of `ops-slack`. */
const test = serverTest().extend<{ connections: OperatorConnections; slack: string }>({
  connections: [async ({ server: _ }, use) => use(OperatorConnections.connect()), { scope: "file" }],
  slack: [async ({ connections }, use) => use((await connections.create(SLACK)).connection_id), { scope: "file" }],
});

test("admin manages redacted connections", async ({ connections, slack }) => {
  const rotated = await connections.update(slack, { provider: "slack", bot_token: SECRETS[1] });
  const disabled = await connections.disable(slack);
  const enabled = await connections.update(slack, { provider: "slack", status: "active" });
  const http = await connections.create({
    provider: "http",
    name: "ops-http",
    origin: "HTTPS://Hooks.Example.COM:443",
    auth: { scheme: "header", name: "X-Api-Key", value: SECRETS[0] },
  });
  const pager = await connections.create({ provider: "pager_duty", name: "ops-pager", integration_key: SECRETS[0] });

  expectRedacted([rotated, disabled, enabled, http, pager, await connections.list()]);
  expect([disabled.status, enabled.status]).toEqual(["disabled", "active"]);
  expect(http).toMatchObject({ origin: "https://hooks.example.com", auth: { scheme: "header", name: "X-Api-Key" } });
});

test("writer is refused", async ({ server }) => {
  const writer = OperatorConnections.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("card_writer", ["cards:read", "cards:write"]) }),
  });

  await expect(writer.list()).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
});

test("other tenant sees nothing", async ({ server, slack }) => {
  const foreign = OperatorConnections.connect({
    client: WyrdClient.connect({
      credential: server.bootstrapServiceInTenant(server.seedTenant("other-tenant"), ["admin"], "other-admin"),
    }),
  });

  expect(await foreign.list()).toEqual([]);
  await expect(foreign.get(slack)).rejects.toMatchObject({ code: "WYRD_OPERATOR_404_CONNECTION_NOT_FOUND" });
});

test("reader cannot disable a connection", async ({ server, connections, slack }) => {
  const reader = OperatorConnections.connect({
    client: WyrdClient.connect({ credential: server.scopedApiKey("operator_reader", ["operators:read"]) }),
  });

  expect(await reader.get(slack)).toEqual(await connections.get(slack));
  await expect(reader.disable(slack)).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
});
