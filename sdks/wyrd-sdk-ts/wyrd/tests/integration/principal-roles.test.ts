// A tenant administrator discovers a Service account and grants it Roles.
// A new Service account holds direct `workload`: it queries but cannot author
// until `editor` is granted, and each change reaches the key's next token.
// One stable runtime account runs every release of its Service family.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Cards, Principals, WyrdClient, WyrdState } from "wyrd-ai";

import { observedServiceTest as test } from "../support/observed-service.js";
import { declared, fixture, runtimeAccount } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** A tenant-wide query over a built-in table, which `workload` permits. */
const TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/** A Card only an author can register. */
const AUTHORED = fixture("cards/gateway_inference/ask-prompt.yaml");

const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** How the token endpoint refuses an unusable key or root (RFC 6749 §5.2 `invalid_request`). */
const EXCHANGE_REFUSED = { code: "WYRD_SPEC_400_VALIDATION", details: { error: "invalid_request" } };

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

/** The second release of the observed Service, sharing its Agent and Model. */
const RELEASE_2 = fixture("cards/observe_a_run/observed-service-2.yaml");

const FAMILY = { mode: "family", kind: "Service", name: "observed-service" } as const;

/** Declare {@link RELEASE_2} and pull it, so a state can run as it. */
async function secondRelease(): Promise<{ bundle: string; uid: string }> {
  const cards = await Cards.connect();
  const receipt = await cards.declareFromPath(RELEASE_2);
  const bundle = mkdtempSync(join(tmpdir(), "wyrd-ts-release-2-"));
  await cards.pull(receipt.root, bundle);
  return { bundle, uid: declared(receipt).uid };
}

test("stable accounts preserve execution identity", async ({ bundle, server, reader }) => {
  const release2 = await secondRelease();
  const account = await runtimeAccount({ kind: "Service", name: "observed-service" });
  expect((await runtimeAccount({ kind: "Service", name: "observed-service" })).principalId).toBe(account.principalId);
  const family = WyrdClient.connect({ credential: account.key });
  const states = [
    WyrdState.fromPath(bundle, { client: family }),
    WyrdState.fromPath(release2.bundle, { client: family }),
  ];
  expect(states[1]?.rootRef.uid).toBe(release2.uid);
  expect(states[0]?.rootRef.uid).not.toBe(release2.uid);

  const runs = await Promise.all(
    states.map(async (state, index) => {
      await state.startBifrost();
      const run = state.run();
      expect(await run.forCard("model").observe.submit("observed-drift", { latency: index + 1 })).toEqual({
        kind: "recorded",
      });
      await state.shutdown();
      return run.runId;
    }),
  );
  server.flushBifrost();

  const rows = await reader.sql(
    "SELECT run_id, card_uid, num_value FROM vala.drift.observations WHERE run_id = $1 OR run_id = $2 ORDER BY num_value",
    runs,
    z.object({ run_id: z.string(), card_uid: z.string(), num_value: z.number() }),
  );
  const model = states[0]?.cardRef("model").uid;
  expect(rows).toEqual([
    { run_id: runs[0], card_uid: model, num_value: 1 },
    { run_id: runs[1], card_uid: model, num_value: 2 },
  ]);

  const component = states[0]?.cardRef("agent").uid ?? "";
  await expect(family.forWorkload(component).accessToken()).rejects.toMatchObject(EXCHANGE_REFUSED);
});

test("a tenant admin restricts accounts to one family runtime", async ({ bundle: _ }) => {
  const principals = await Principals.connect();
  const owner = await runtimeAccount({ kind: "Service", name: "observed-service" });
  const { principal, key } = await issue("stable-spare");

  await expect(principals.setWorkloadRestriction(principal, FAMILY)).rejects.toMatchObject({
    code: "WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT",
  });
  const other = { mode: "family", kind: "Service", name: "stable-spare-family" } as const;
  expect((await principals.setWorkloadRestriction(principal, other)).workload_restriction).toEqual(other);
  expect((await principals.setWorkloadRestriction(principal, { mode: "unrestricted" })).workload_restriction).toEqual({
    mode: "unrestricted",
  });
  const own = await Principals.connect({ client: WyrdClient.connect({ credential: owner.key }) });
  await expect(own.setWorkloadRestriction(principal, other)).rejects.toMatchObject(DENIED);

  await principals.revokePrincipal(principal, { principal_kind: "service", reason: "story retires the account" });
  await expect(WyrdClient.connect({ credential: key }).accessToken()).rejects.toMatchObject(EXCHANGE_REFUSED);
});

test("a rooted service acts within its callers authority", async ({ server, state }) => {
  const account = await runtimeAccount({ kind: "Service", name: "observed-service" });
  const actor = WyrdClient.connect({ credential: account.key }).forWorkload(state.rootRef.uid ?? "");
  const delegated = await actor.onBehalfOf(server.token, { audience: "wyrd" });

  expect((await (await Cards.connect({ client: delegated })).get(state.rootRef)).metadata.name).toBe(
    "observed-service",
  );
  await expect((await Principals.connect({ client: delegated })).list()).rejects.toMatchObject(DENIED);
});
