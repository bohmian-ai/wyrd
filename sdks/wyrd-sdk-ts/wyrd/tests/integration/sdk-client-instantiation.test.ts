// Every server-facing handle opens with `connect()`: the ambient client from
// the environment or saved configuration, resolved once, exchanged eagerly,
// and retained with its renewable token cache for the handle's lifetime.
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, Cards, Gateway, OperatorConnections, Principals, TableConfig, WyrdClient } from "wyrd-ai";
import type { NativeWyrdTestServer } from "@wyrd/testing";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/**
 * Access-token lifetime: the client renews a token within 30 seconds of its
 * expiry, so each token is reused for five seconds and then renewed.
 */
const ACCESS_TTL_SECONDS = 35;

/** How long a freshly exchanged token is reused before the client renews it. */
const FRESH_SECONDS = ACCESS_TTL_SECONDS - 30;

/** A loopback port nothing listens on. */
const UNREACHABLE = "http://127.0.0.1:1";

const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

const test = serverTest({ accessTtlSeconds: ACCESS_TTL_SECONDS });

/** Every server-facing handle, each with one harmless real call. */
const HANDLES = [
  { handle: Bifrost, call: (bifrost: Bifrost) => bifrost.listTables() },
  { handle: Cards, call: (cards: Cards) => cards.list({ kind: "Prompt" }) },
  { handle: Gateway, call: (gateway: Gateway) => gateway.deployments() },
  { handle: OperatorConnections, call: (connections: OperatorConnections) => connections.list() },
  { handle: Principals, call: (principals: Principals) => principals.list() },
] as const;

/** One handle connected through `options`, paired with its harmless call. */
type Opened = { readonly name: string; readonly call: () => Promise<unknown> };

/** Connect every handle with `options` and bind each to its harmless call. */
async function openAll(options: { client?: WyrdClient } = {}): Promise<Opened[]> {
  const opened: Opened[] = [];
  for (const { handle, call } of HANDLES) {
    const connected = await (handle.connect as (options: object) => Promise<never>)(options);
    opened.push({ name: handle.name, call: () => (call as (handle: never) => Promise<unknown>)(connected) });
  }
  return opened;
}

/**
 * Replace the deployment environment with saved configuration alone: the
 * server's endpoints in `config.toml` and its key in `credentials.toml`.
 */
function savedConfigurationOnly(server: NativeWyrdTestServer): void {
  const home = mkdtempSync(join(tmpdir(), "wyrd-ts-saved-config-"));
  const files = {
    "config.toml": `[client]\nhttp_url = "${server.baseUrl}"\ngrpc_url = "${server.grpcUrl}"\n`,
    "credentials.toml": `[default]\napi_key = "${server.apiKey}"\n`,
  };
  for (const [name, contents] of Object.entries(files)) {
    writeFileSync(join(home, name), contents);
    chmodSync(join(home, name), 0o600);
  }
  vi.stubEnv("WYRD_CONFIG_HOME", home);
  for (const name of ["WYRD_SERVER_URL", "WYRD_GRPC_URL", "WYRD_API_KEY"]) {
    vi.stubEnv(name, undefined);
  }
}

test("ambient connect opens every handle", async ({ server }) => {
  for (const ambient of [() => {}, () => savedConfigurationOnly(server)]) {
    ambient();
    for (const { name, call } of await openAll()) {
      await expect(call(), name).resolves.toBeDefined();
    }
    const bound = await Bifrost.connect({
      table: TableConfig.fromJsonSchema("vala.datasets.ambient_options", z.object({ id: z.int() })),
      clientByteLimitBytes: 8 * 1024 * 1024,
    });
    expect(["created", "already_exists"]).toContain(await bound.register());
    await bound.shutdown();
  }
});

test("connect without a credential fails eagerly", async () => {
  vi.stubEnv("WYRD_API_KEY", undefined);

  for (const { handle } of HANDLES) {
    await expect(handle.connect(), handle.name).rejects.toMatchObject({
      code: "WYRD_CLIENT_401_NO_CREDENTIALS",
      status: 401,
    });
  }
});

test("credential paths preserve exchange and bearer behavior", async ({ server }) => {
  const [tenant] = server.apiKey.slice("wyrd_sk_".length).split("_");
  const forged = WyrdClient.connect({ credential: `wyrd_sk_${tenant}_forged_secret` });
  const issued = await WyrdClient.connect({ credential: server.apiKey }).accessToken();
  const signature = issued.lastIndexOf(".") + 1;
  const bearer = WyrdClient.connect({
    credential: issued.slice(0, signature) + "A".repeat(issued.length - signature),
  });

  for (const { handle } of HANDLES) {
    await expect(handle.connect({ client: forged }), handle.name).rejects.toMatchObject({
      code: "WYRD_SPEC_400_VALIDATION",
      detail: expect.stringContaining("authorization server refused"),
    });
  }
  for (const { name, call } of await openAll({ client: bearer })) {
    await expect(call(), name).rejects.toMatchObject({ status: 401 });
  }
});

test("delegated connect uses the subject principal", async ({ server }) => {
  const subject = server.scopedApiKey("instantiation_subject", ["bifrost_query:read"]);
  const actor = WyrdClient.connect({
    credential: server.scopedApiKey("instantiation_actor", ["bifrost_query:read", "bifrost_table:read"]),
  });
  const token = await WyrdClient.connect({ credential: subject }).accessToken();
  const delegated = await actor.onBehalfOf(token, { audience: "bifrost" });

  await expect((await Bifrost.connect({ client: delegated })).listTables()).rejects.toMatchObject(DENIED);
  await expect((await Bifrost.connect({ client: actor })).listTables()).resolves.toBeDefined();
});

test("handles reuse the client cache and renew", async ({ server }) => {
  const client = WyrdClient.connect({ credential: server.apiKey });
  const explicit = await openAll({ client });
  const ambient = await openAll();
  const exchanged = await client.accessToken();

  for (const { name, call } of [...explicit, ...ambient]) {
    await expect(call(), name).resolves.toBeDefined();
  }
  expect(await client.accessToken()).toBe(exchanged);

  vi.stubEnv("WYRD_SERVER_URL", UNREACHABLE);
  vi.stubEnv("WYRD_GRPC_URL", UNREACHABLE);
  vi.stubEnv("WYRD_API_KEY", undefined);
  await sleep((FRESH_SECONDS + 1) * 1000);

  for (const { name, call } of [...explicit, ...ambient]) {
    await expect(call(), name).resolves.toBeDefined();
  }
  expect(await client.accessToken()).not.toBe(exchanged);
});

test("Bifrost lists tables and describes schemas on demand", async ({ server }) => {
  const own = await Bifrost.connect({
    table: TableConfig.fromJsonSchema("vala.datasets.listed", z.object({ id: z.int(), label: z.string() })),
  });
  await own.register();
  const foreign = await Bifrost.connect({
    client: WyrdClient.connect({
      credential: server.bootstrapServiceInTenant(server.seedTenant("listing-tenant"), ["admin"], "listing-admin"),
    }),
    table: TableConfig.fromJsonSchema("vala.datasets.foreign_listed", z.object({ id: z.int() })),
  });
  await foreign.register();

  const listed = (await own.listTables()).map((entry) => `${entry.namespace}.${entry.name}`);

  expect(listed).toContain("vala.datasets.listed");
  expect(listed).toContain("vala.drift.observations");
  expect(listed).not.toContain("vala.datasets.foreign_listed");
  const unreadable = WyrdClient.connect({ credential: server.scopedApiKey("no_table_read", ["bifrost_query:read"]) });
  await expect((await Bifrost.connect({ client: unreadable })).listTables()).rejects.toMatchObject(DENIED);
  const described = await own.describeTable("vala.datasets", "listed");
  expect(described.user_fields.map((field) => field.name)).toEqual(["id", "label"]);
});
