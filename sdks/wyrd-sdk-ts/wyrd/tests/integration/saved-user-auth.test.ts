import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { Cards } from "@wyrd/sdk";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const FIXTURE_TENANT = "test-tenant-1";
const SECOND_TENANT = "saved-login-two";
const PROMPT = fixture("cards/gateway_inference/ask-prompt.yaml");

/** `Cards` resolved from the saved logins, with `WYRD_TENANT` the only selector. */
function cards(tenant?: string, credential?: string): Cards {
  vi.stubEnv("WYRD_TENANT", tenant);
  return Cards.connect({ credential });
}

/** The refusal of a saved login that could not be used, for `reason`. */
function unusable(reason: string) {
  return { code: "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE", details: { reason } };
}

const test = serverTest({ humanSso: true });

test("saved user auth journey", async ({ server }) => {
  const config = mkdtempSync(join(tmpdir(), "wyrd-ts-saved-login-"));
  vi.stubEnv("WYRD_CONFIG_HOME", config);
  vi.stubEnv("WYRD_API_KEY", undefined);
  server.activateHumanSso();
  server.activateHumanSso(SECOND_TENANT);
  server.saveHumanLogin(config, FIXTURE_TENANT, "bob", "wyrd-test");
  server.saveHumanLogin(config, SECOND_TENANT, "alice", "alice-password");

  // Without a selector the newest login, alice's admin login, is used; a
  // selector must name a saved login.
  await cards().registerFromPath(PROMPT);
  expect(() => cards("no-such-tenant")).toThrow(expect.objectContaining(unusable("tenant_mismatch")));

  // Bob is a reader: the read is allowed and the write denied.
  const reader = cards(FIXTURE_TENANT);
  await reader.list({ kind: "Prompt" });
  await expect(reader.registerFromPath(PROMPT)).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
  await cards(SECOND_TENANT).list({ kind: "Prompt" });

  // A stale login renews through Wyrd and the renewal is saved.
  server.expireSavedLogin(config, FIXTURE_TENANT);
  expect(server.savedLoginIsStale(config, FIXTURE_TENANT)).toBe(true);
  await cards(FIXTURE_TENANT).list({ kind: "Prompt" });
  expect(server.savedLoginIsStale(config, FIXTURE_TENANT)).toBe(false);

  // An explicit machine credential overrides the saved reader; it names its
  // own tenant, so a selector beside it is refused.
  expect(() => cards(FIXTURE_TENANT, server.apiKey)).toThrow(
    expect.objectContaining({ code: "WYRD_CLIENT_400_CONFIG_INVALID" }),
  );
  await cards(undefined, server.apiKey).registerFromPath(PROMPT);

  // Once the chain is revoked the login fails closed and asks for a new login.
  server.revokeSavedLogin(config, FIXTURE_TENANT);
  server.expireSavedLogin(config, FIXTURE_TENANT);
  await expect(cards(FIXTURE_TENANT).list({ kind: "Prompt" })).rejects.toMatchObject(unusable("refresh_refused"));
});
