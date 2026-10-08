import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { Cards, WyrdClient } from "@wyrd/sdk";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** The test server's own tenant, where bob reads Cards and cannot write them. */
const READER_TENANT = "test-tenant-1";
/** A second tenant where alice administers. */
const ADMIN_TENANT = "saved-login-two";
const PROMPT = fixture("cards/gateway_inference/ask-prompt.yaml");

/** `Cards` resolved from the saved logins, with `WYRD_TENANT` the only selector. */
function cards(tenant?: string, credential?: string): Cards {
  vi.stubEnv("WYRD_TENANT", tenant);
  return Cards.connect({ client: WyrdClient.connect({ credential }) });
}

/**
 * Human single sign-on enabled once for both tenants, then per test a fresh
 * configuration home holding bob's saved reader login and no API key.
 */
const test = serverTest({ humanSso: true }).extend<{ sso: void; config: string }>({
  sso: [
    async ({ server }, use) => {
      server.activateHumanSso();
      server.activateHumanSso(ADMIN_TENANT);
      await use();
    },
    { scope: "file" },
  ],
  config: async ({ server, sso: _ }, use) => {
    const config = mkdtempSync(join(tmpdir(), "wyrd-ts-saved-login-"));
    vi.stubEnv("WYRD_CONFIG_HOME", config);
    vi.stubEnv("WYRD_API_KEY", undefined);
    server.saveHumanLogin(config, READER_TENANT, "bob", "wyrd-test");
    await use(config);
  },
});

test("newest saved login is used without a selector", async ({ server, config }) => {
  server.saveHumanLogin(config, ADMIN_TENANT, "alice", "alice-password");

  const receipt = await cards().registerFromPath(PROMPT);

  expect(receipt.root).toMatchObject({ kind: "Prompt", name: "ask-prompt" });
});

test("saved reader login is denied a write", async ({ config: _ }) => {
  await expect(cards(READER_TENANT).registerFromPath(PROMPT)).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });
});

test("stale login refreshes and saves the renewal", async ({ server, config }) => {
  server.expireSavedLogin(config, READER_TENANT);

  await cards(READER_TENANT).list({ kind: "Prompt" });

  expect(server.savedLoginIsStale(config, READER_TENANT)).toBe(false);
});

test("revoked login is refused", async ({ server, config }) => {
  server.revokeSavedLogin(config, READER_TENANT);
  server.expireSavedLogin(config, READER_TENANT);

  await expect(cards(READER_TENANT).list({ kind: "Prompt" })).rejects.toMatchObject({
    code: "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE",
  });
});

test("selector naming no saved login is refused", ({ config: _ }) => {
  expect(() => cards("no-such-tenant")).toThrow(
    expect.objectContaining({ code: "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE" }),
  );
});

// An explicit machine credential names its own tenant.
test("explicit key beside a tenant selector is refused", ({ server, config: _ }) => {
  expect(() => cards(READER_TENANT, server.apiKey)).toThrow(
    expect.objectContaining({ code: "WYRD_CLIENT_400_CONFIG_INVALID" }),
  );
});
