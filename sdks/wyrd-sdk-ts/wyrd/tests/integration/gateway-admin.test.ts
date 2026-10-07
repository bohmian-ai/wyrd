import { expect, vi } from "vitest";

import { Gateway, type ProviderDeployment, cli } from "@wyrd/sdk";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** A provider credential the server resolves from its `test-provider-key` binding. */
function environmentCredential(name: string) {
  return { name, provider: "openai", source: { environment: { binding: "test-provider-key" } } } as const;
}

/** An `openai` deployment named `name` that authenticates with `credential`. */
function deployment(name: string, credential: string): ProviderDeployment {
  return {
    name,
    model: { provider: "openai", model: "gpt-4o" },
    adapter: "openai",
    auth: { bearer: { credential } },
    capabilities: ["chat_completions"],
    routing_weight: 1,
  };
}

/** A Gateway administration handle for the deployment's writer. */
const test = serverTest().extend<{ gateway: Gateway }>({
  gateway: [async ({ server: _ }, use) => use(Gateway.connect()), { scope: "file" }],
});

test("cli written credential reads back through the gateway", async ({ gateway }) => {
  const written = await cli.putProviderCredential(environmentCredential("read-back-key"));

  expect(written).toMatchObject({ ...environmentCredential("read-back-key"), state: "active" });
  expect(await gateway.credential("read-back-key")).toEqual(written);
  expect((await gateway.credentials()).map((credential) => credential.name)).toContain("read-back-key");
});

test("deployment round trips", async ({ gateway }) => {
  await cli.putProviderCredential(environmentCredential("round-trip-key"));
  const primary = deployment("round-trip", "round-trip-key");

  expect(await gateway.putDeployment(primary)).toEqual(primary);
  expect(await gateway.deployment("round-trip")).toEqual(primary);
  expect(await gateway.deployments()).toContainEqual(primary);

  await gateway.deleteDeployment("round-trip");
  await expect(gateway.deployment("round-trip")).rejects.toMatchObject({
    code: "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND",
  });
});

test("credential in use cannot be deleted", async ({ gateway }) => {
  await cli.putProviderCredential(environmentCredential("in-use-key"));
  await gateway.putDeployment(deployment("in-use", "in-use-key"));

  await expect(cli.deleteProviderCredential("in-use-key")).rejects.toMatchObject({
    code: "WYRD_GATEWAY_409_RESOURCE_CONFLICT",
  });
});

test("deleting a credential twice succeeds", async ({ gateway }) => {
  await cli.putProviderCredential(environmentCredential("deleted-key"));

  await cli.deleteProviderCredential("deleted-key");
  await cli.deleteProviderCredential("deleted-key");

  await expect(gateway.credential("deleted-key")).rejects.toMatchObject({
    code: "WYRD_GATEWAY_404_RESOURCE_NOT_FOUND",
  });
});

test("deployment without capabilities is refused", async ({ gateway }) => {
  await cli.putProviderCredential(environmentCredential("no-capability-key"));

  await expect(
    gateway.putDeployment({ ...deployment("no-capability", "no-capability-key"), capabilities: [] }),
  ).rejects.toMatchObject({ code: "WYRD_GATEWAY_400_INVALID_CONFIGURATION" });
});

test("gateway reader cannot delete a deployment", async ({ server, gateway }) => {
  await cli.putProviderCredential(environmentCredential("reader-key"));
  await gateway.putDeployment(deployment("read-only", "reader-key"));
  const reader = Gateway.connect({ credential: server.scopedApiKey("gateway_reader", ["gateway:read"]) });

  expect(await reader.deployment("read-only")).toMatchObject({ name: "read-only" });
  await expect(reader.deleteDeployment("read-only")).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("capture policy round trips", async ({ gateway }) => {
  expect(await gateway.capturePolicy()).toMatchObject({ mode: "disabled" });

  const capture = await gateway.putCapturePolicy({ mode: "payload", payload_fields: ["request"] });

  expect(capture).toMatchObject({ mode: "payload", payload_fields: ["request"] });
  expect(await gateway.capturePolicy()).toEqual(capture);
});
