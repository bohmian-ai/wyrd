// A tenant administrator discovers a Service's principal and grants it Roles.
// A new Service key holds direct `workload`: it queries but cannot author
// until `editor` is granted, and each change reaches the key's next token.
import { expect, vi } from "vitest";

import { Bifrost, Cards, Principals, WyrdClient } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { observedServiceTest as test } from "../support/observed-service.js";
import { fixture } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** A tenant-wide query over a built-in table, which `workload` permits. */
const TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/** A Card only an author can register. */
const AUTHORED = fixture("cards/gateway_inference/ask-prompt.yaml");

const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** Issue the observed Service's own key; its principal id and key. */
async function issue(): Promise<{ principal: string; key: string }> {
  const issued = await cli.issueKey({ kind: "Service", name: "observed-service", version: "1.0.0", space: "default" });
  return { principal: issued.principal_id, key: issued.key };
}

/** Register `AUTHORED` through a fresh client, so it carries the key's next token. */
function author(key: string): Promise<unknown> {
  return Cards.connect({ client: WyrdClient.connect({ credential: key }) }).registerFromPath(AUTHORED);
}

test("granted editor reaches the next token until revoked", async ({ bundle: _ }) => {
  const { principal, key } = await issue();
  const principals = Principals.connect();

  expect((await principals.roles(principal)).roles).toEqual([{ role: "workload", source: "direct" }]);
  const reader = await Bifrost.connect({ client: WyrdClient.connect({ credential: key }) });
  expect((await reader.sql(TENANT_WIDE)).numRows).toBe(1);
  await reader.shutdown();
  await expect(author(key)).rejects.toMatchObject(DENIED);

  const granted = await principals.grantRole(principal, "editor");
  expect(granted.changed).toBe(true);
  expect(granted.roles).toEqual([
    { role: "editor", source: "direct" },
    { role: "workload", source: "direct" },
  ]);
  expect((await principals.grantRole(principal, "editor")).changed).toBe(false);
  await author(key);

  expect((await principals.revokeRole(principal, "editor")).changed).toBe(true);
  await expect(author(key)).rejects.toMatchObject(DENIED);
});

test("only a tenant admin assigns roles", async ({ bundle: _ }) => {
  const { principal, key } = await issue();
  const own = Principals.connect({ client: WyrdClient.connect({ credential: key }) });

  await expect(own.grantRole(principal, "editor")).rejects.toMatchObject(DENIED);
  await expect(own.revokeRole(principal, "workload")).rejects.toMatchObject(DENIED);
});

test("direct and idp user assignments coexist", async ({ server }) => {
  const user = server.bootstrapUser(["viewer"], "pr-person");
  const principals = Principals.connect();

  const page = await principals.list({ email: "pr-person@test.wyrd" });
  expect(page.principals.map((found) => [found.principal_id, found.kind])).toEqual([[user, "user"]]);
  expect(page.next).toBeNull();

  const granted = await principals.grantRole(user, "viewer");
  expect(granted.changed).toBe(true);
  expect(granted.roles).toEqual([
    { role: "viewer", source: "direct" },
    { role: "viewer", source: "idp" },
  ]);
  const revoked = await principals.revokeRole(user, "viewer");
  expect(revoked.changed).toBe(true);
  expect(revoked.roles).toEqual([{ role: "viewer", source: "idp" }]);
});
