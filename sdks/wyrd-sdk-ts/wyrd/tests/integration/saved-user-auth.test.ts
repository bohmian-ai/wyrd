import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { startTestServer } from "@wyrd/testing";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Cards, WyrdError } from "@wyrd/sdk";

const FIXTURE_TENANT = "test-tenant-1";
const SECOND_TENANT = "saved-login-two";
const AMBIENT = ["WYRD_API_KEY", "WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"];

/** Await a rejected SDK call and return its catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/** Capture a synchronous catalog error. */
function thrown(action: () => unknown): WyrdError {
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return error as WyrdError;
  }
  throw new Error("expected a WyrdError");
}

/** Connect `Cards` with `WYRD_TENANT` set to `tenant`, the only selector. */
function cards(serverUrl: string, tenant?: string, credential?: string): Cards {
  if (tenant === undefined) {
    delete process.env.WYRD_TENANT;
  } else {
    process.env.WYRD_TENANT = tenant;
  }
  return Cards.connect({ serverUrl, credential });
}

/** The stable reason a saved login could not be used. */
function reason(error: WyrdError): unknown {
  expect(error.code).toBe("WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
  return (error.details as { reason?: unknown }).reason;
}

describe("saved user login", () => {
  const saved: Record<string, string | undefined> = {};
  beforeEach(() => {
    for (const name of [...AMBIENT, "WYRD_CONFIG_HOME"]) {
      saved[name] = process.env[name];
      delete process.env[name];
    }
  });
  afterEach(() => {
    for (const [name, value] of Object.entries(saved)) {
      if (value === undefined) {
        delete process.env[name];
      } else {
        process.env[name] = value;
      }
    }
  });

  it("saved user auth journey", async () => {
    const config = mkdtempSync(join(tmpdir(), "wyrd-ts-saved-login-"));
    const prompt = join(config, "prompt.yaml");
    writeFileSync(
      prompt,
      "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: ts-saved-login\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\n",
    );
    process.env.WYRD_CONFIG_HOME = config;
    const server = startTestServer(undefined, undefined, undefined, true);
    try {
      const serverUrl = server.baseUrl;
      server.activateHumanSso();
      server.activateHumanSso(SECOND_TENANT);
      server.saveHumanLogin(config, FIXTURE_TENANT, "bob", "wyrd-test");
      server.saveHumanLogin(config, SECOND_TENANT, "alice", "alice-password");

      // Without a selector the newest login, alice's admin login, is used; a
      // selector must name a saved login.
      await cards(serverUrl).registerFromPath(prompt);
      expect(reason(thrown(() => cards(serverUrl, "no-such-tenant")))).toBe(
        "tenant_mismatch",
      );

      // Bob is a reader: the read is allowed and the write denied.
      const reader = cards(serverUrl, FIXTURE_TENANT);
      await reader.list({ kind: "Prompt" });
      expect((await rejection(reader.registerFromPath(prompt))).status).toBe(403);
      await cards(serverUrl, SECOND_TENANT).list({ kind: "Prompt" });

      // A stale login renews through Wyrd and the renewal is saved.
      server.expireSavedLogin(config, FIXTURE_TENANT);
      expect(server.savedLoginIsStale(config, FIXTURE_TENANT)).toBe(true);
      await cards(serverUrl, FIXTURE_TENANT).list({ kind: "Prompt" });
      expect(server.savedLoginIsStale(config, FIXTURE_TENANT)).toBe(false);

      // An explicit machine credential overrides the saved reader; it names
      // its own tenant, so a selector beside it is refused.
      const selected = thrown(() =>
        cards(serverUrl, FIXTURE_TENANT, server.apiKey),
      );
      expect(selected.code).toBe("WYRD_CLIENT_400_CONFIG_INVALID");
      expect(selected.message).toContain("already names its tenant");
      await cards(serverUrl, undefined, server.apiKey).registerFromPath(prompt);

      // Once the chain is revoked the login fails closed and asks for a new login.
      server.revokeSavedLogin(config, FIXTURE_TENANT);
      server.expireSavedLogin(config, FIXTURE_TENANT);
      const revoked = cards(serverUrl, FIXTURE_TENANT);
      expect(reason(await rejection(revoked.list({ kind: "Prompt" })))).toBe("refresh_refused");
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
