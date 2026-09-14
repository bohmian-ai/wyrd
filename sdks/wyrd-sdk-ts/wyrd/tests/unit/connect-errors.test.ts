import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Bifrost, Cards, TableConfig, WyrdError } from "@wyrd/sdk";

const CREDENTIAL_VARS = [
  "WYRD_ACCESS_TOKEN",
  "WYRD_WORKLOAD_TOKEN",
  "WYRD_TENANT",
  "WYRD_API_KEY",
] as const;

let saved: Record<string, string | undefined> = {};

beforeEach(() => {
  saved = {};
  for (const name of [...CREDENTIAL_VARS, "HOME"]) {
    saved[name] = process.env[name];
    delete process.env[name];
  }
  process.env.HOME = "/nonexistent-wyrd-home";
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

function expectCatalogError(error: unknown, code: string, status: number): void {
  expect(error).toBeInstanceOf(WyrdError);
  const wyrd = error as WyrdError;
  expect(wyrd.code).toBe(code);
  expect(wyrd.status).toBe(status);
  expect(wyrd.title).not.toBe("");
  expect(wyrd.remediation).toBeTruthy();
}

describe("native construction failures", () => {
  it("Cards.connect throws the no-credentials WyrdError", () => {
    let caught: unknown;
    try {
      Cards.connect({ serverUrl: "http://127.0.0.1:1" });
    } catch (error) {
      caught = error;
    }
    expectCatalogError(caught, "WYRD_CLIENT_401_NO_CREDENTIALS", 401);
  });

  it("Bifrost.connect rejects with the no-credentials WyrdError", async () => {
    const caught: unknown = await Bifrost.connect({
      serverUrl: "http://127.0.0.1:1",
      grpcUrl: "http://127.0.0.1:1",
    }).catch((error: unknown) => error);
    expectCatalogError(caught, "WYRD_CLIENT_401_NO_CREDENTIALS", 401);
  });

  it("TableConfig.describe rejects with the no-credentials WyrdError", async () => {
    const caught: unknown = await TableConfig.describe("unit.missing", {
      serverUrl: "http://127.0.0.1:1",
    }).catch((error: unknown) => error);
    expectCatalogError(caught, "WYRD_CLIENT_401_NO_CREDENTIALS", 401);
  });

  it("TableConfig.describe rejects an unreachable server with the transport WyrdError", async () => {
    const caught: unknown = await TableConfig.describe("unit.missing", {
      serverUrl: "http://127.0.0.1:1",
      credential: "wyrd_sk_t_v_s",
    }).catch((error: unknown) => error);
    expectCatalogError(caught, "WYRD_CLIENT_503_TRANSPORT_DOWN", 503);
    expect((caught as WyrdError).details).toEqual({ transport: "http" });
  });

  it("TableConfig.describe rejects an empty server URL with config-invalid details", async () => {
    const caught: unknown = await TableConfig.describe("unit.missing", {
      serverUrl: "",
      credential: "wyrd_sk_t_v_s",
    }).catch((error: unknown) => error);
    expectCatalogError(caught, "WYRD_CLIENT_400_CONFIG_INVALID", 400);
    expect((caught as WyrdError).details).toEqual({
      field: "server_url",
      reason: "must not be empty",
    });
  });
});
