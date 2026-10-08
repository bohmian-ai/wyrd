// A tenant administrator grants a Service's principal a Role with the
// `wyrd auth grant-role` CLI command; the Service's key acts with it at its
// next key exchange.
import { expect, vi } from "vitest";

import { Bifrost, WyrdClient } from "@wyrd/sdk";
import { cli } from "@wyrd/testing";

import { observedServiceTest as test } from "../support/observed-service.js";

vi.setConfig({ testTimeout: 120_000 });

/** A tenant-wide query over a built-in table, which `wyrd_default` does not permit and `workload` does. */
const TENANT_WIDE = "SELECT COUNT(*) AS n FROM vala.drift.observations";

/** The registered observed Service, granted `workload`. */
const WORKLOAD_GRANT = {
  kind: "Service",
  name: "observed-service",
  version: "1.0.0",
  space: "default",
  role: "workload",
} as const;

/** A query-only Bifrost acting as the principal behind `key`, exchanged afresh. */
function bifrostAs(key: string): Promise<Bifrost> {
  return Bifrost.connect({ client: WyrdClient.connect({ credential: key }) });
}

test("default service key needs workload for tenant wide queries", async ({ serviceKey }) => {
  const refused = await bifrostAs(serviceKey);
  await expect(refused.sql(TENANT_WIDE)).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
  await refused.shutdown();

  const granted = await cli.grantRole(WORKLOAD_GRANT);
  expect(granted.granted).toBe(true);
  expect(granted.roles).toContain("workload");

  const counted = await bifrostAs(serviceKey);
  expect((await counted.sql(TENANT_WIDE)).numRows).toBe(1);
  await counted.shutdown();
});

test("only a tenant admin can grant a role", async ({ serviceKey }) => {
  const service = WyrdClient.connect({ credential: serviceKey });

  await expect(cli.grantRole(WORKLOAD_GRANT, { client: service })).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});
