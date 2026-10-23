// A tenant administrator discovers a Service account and grants it Roles.
// A new Service account holds direct `workload`: it queries but cannot author
// until `editor` is granted, and each change reaches the key's next token.
import { expect, vi } from "vitest";

import { Bifrost, Cards, Principals, WyrdClient } from "wyrd-ai";

import { observedServiceTest as test } from "../support/observed-service.js";
import { fixture } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** A tenant-wide query over a built-in table, which `workload` permits. */
const TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/** A Card only an author can register. */
const AUTHORED = fixture("cards/gateway_inference/ask-prompt.yaml");

const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** Create the unrestricted Service account `name` holding only `workload`; its principal id and key. */
async function issue(name: string): Promise<{ principal: string; key: string }> {
  const created = await (await Principals.connect()).createServicePrincipal({ name, roles: ["workload"] });
  return { principal: created.principal_id, key: created.credential };
}

/** Register `AUTHORED` through a fresh client, so it carries the key's next token. */
async function author(key: string): Promise<unknown> {
  return (await Cards.connect({ client: WyrdClient.connect({ credential: key }) })).declareFromPath(AUTHORED);
}

test("granted editor reaches the next token until revoked", async ({ bundle: _ }) => {
  const { principal, key } = await issue("granted-editor");
  const principals = await Principals.connect();

  const [found] = (await principals.list({ kind: "service", name: "granted-editor" })).principals;
  expect(found?.principal_id).toBe(principal);
  expect(found?.workload_restriction).toEqual({ mode: "unrestricted" });
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
  const { principal, key } = await issue("own-roles");
  const own = await Principals.connect({ client: WyrdClient.connect({ credential: key }) });

  await expect(own.grantRole(principal, "editor")).rejects.toMatchObject(DENIED);
  await expect(own.revokeRole(principal, "workload")).rejects.toMatchObject(DENIED);
});

test("direct and idp user assignments coexist", async ({ server }) => {
  const user = server.bootstrapUser(["viewer"], "pr-person");
  const principals = await Principals.connect();

  const page = await principals.list({ email: "pr-person@test.wyrd" });
  expect(page.principals.map((found) => [found.principal_id, found.kind])).toEqual([[user, "user"]]);
  expect(page.next).toBeNull();
  expect(page.principals[0]?.workload_restriction).toBeNull();

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
