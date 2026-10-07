import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { beforeEach, expect, test, vi } from "vitest";

import { Bifrost, Cards, TableConfig, WyrdClient } from "@wyrd/sdk";

/** A loopback port nothing listens on. */
const UNREACHABLE = "http://127.0.0.1:1";

// No ambient credential and no saved login: only an explicit argument can
// authenticate.
beforeEach(() => {
  for (const name of ["WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"]) {
    vi.stubEnv(name, undefined);
  }
  vi.stubEnv("WYRD_CONFIG_HOME", mkdtempSync(join(tmpdir(), "wyrd-ts-config-")));
  return () => vi.unstubAllEnvs();
});

test.for<readonly [string, () => unknown]>([
  ["WyrdClient.connect", () => WyrdClient.connect({ serverUrl: UNREACHABLE })],
  ["Cards.connect", () => Cards.connect({ serverUrl: UNREACHABLE })],
  ["Bifrost.connect", () => Bifrost.connect({ serverUrl: UNREACHABLE, grpcUrl: UNREACHABLE })],
  ["TableConfig.describe", () => TableConfig.describe("unit.missing", { serverUrl: UNREACHABLE })],
])("%s without a credential is refused", async ([, connect]) => {
  await expect(Promise.resolve().then(connect)).rejects.toMatchObject({
    code: "WYRD_CLIENT_401_NO_CREDENTIALS",
    status: 401,
    remediation: expect.any(String),
  });
});

test("unreachable server is refused with the transport error", async () => {
  await expect(
    TableConfig.describe("unit.missing", { serverUrl: UNREACHABLE, credential: "wyrd_sk_t_v_s" }),
  ).rejects.toMatchObject({ code: "WYRD_CLIENT_503_TRANSPORT_DOWN", details: { transport: "http" } });
});
