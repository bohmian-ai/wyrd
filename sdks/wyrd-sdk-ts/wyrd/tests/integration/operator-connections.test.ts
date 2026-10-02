import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { OperatorConnections, WyrdError } from "@wyrd/sdk";

/** Secrets the journey sends; none may appear in any response or error. */
const SECRETS = ["xoxb-ts-sdk-secret", "xoxb-ts-sdk-rotated"];

/** Fail when a journey secret appears in `value`. */
function expectRedacted(value: unknown): void {
  const text = JSON.stringify(value) + String(value);
  for (const secret of SECRETS) {
    expect(text).not.toContain(secret);
  }
}

/** Await a promise expected to reject with a structured catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  expectRedacted(error);
  return error as WyrdError;
}

describe("operator connection journey", () => {
  it("manages redacted connections with read/write separation", async () => {
    const server = startTestServer();
    try {
      const admin = OperatorConnections.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });
      const create = {
        provider: "slack",
        name: "ts-slack",
        workspace_id: "T0001",
        bot_token: SECRETS[0] ?? "",
      } as const;
      const created = await admin.create(create);
      expectRedacted(created);
      expect(created.status).toBe("active");
      const conflict = await rejection(admin.create(create));
      expect(conflict.code).toBe("WYRD_OPERATOR_409_CONNECTION_CONFLICT");

      expect(await admin.list()).toEqual([created]);
      const id = created.connection_id;
      const rotated = await admin.update(id, {
        provider: "slack",
        bot_token: SECRETS[1],
      });
      expectRedacted(rotated);
      expect(rotated.connection_id).toBe(id);
      expect((await admin.disable(id)).status).toBe("disabled");
      expect((await admin.get(id)).status).toBe("disabled");
      expect(
        (await admin.update(id, { provider: "slack", status: "active" })).status,
      ).toBe("active");
      expect(created).toMatchObject({ provider: "slack", workspace_id: "T0001" });
      expect(created).not.toHaveProperty("config");

      const pager = await admin.create({
        provider: "pager_duty",
        name: "ts-pager",
        integration_key: SECRETS[0] ?? "",
      });
      expectRedacted(pager);
      expect(pager.provider).toBe("pager_duty");
      expect(pager).not.toHaveProperty("config");
      expect(pager).not.toHaveProperty("integration_key");

      const http = await admin.create({
        provider: "http",
        name: "ts-http",
        origin: "HTTPS://Hooks.Example.COM:443",
        auth: { scheme: "header", name: "X-Api-Key", value: SECRETS[0] ?? "" },
      });
      expectRedacted(http);
      expect(http).toMatchObject({
        provider: "http",
        origin: "https://hooks.example.com",
        auth: { scheme: "header", name: "X-Api-Key" },
      });
      expect(http).not.toHaveProperty("config");
      const basic = await admin.update(http.connection_id, {
        provider: "http",
        auth: { scheme: "basic", username: "u", password: SECRETS[1] ?? "" },
      });
      expectRedacted(basic);
      expect(basic.provider === "http" && basic.auth).toEqual({ scheme: "basic" });
      const bearer = await admin.update(http.connection_id, {
        provider: "http",
        origin: "https://other.example.com",
        auth: { scheme: "bearer", token: SECRETS[1] ?? "" },
      });
      expect(bearer).toMatchObject({
        origin: "https://other.example.com",
        auth: { scheme: "bearer" },
      });
      const mismatch = await rejection(
        admin.update(http.connection_id, { provider: "slack", bot_token: "x" }),
      );
      expect(mismatch.code).toBe("WYRD_OPERATOR_400_INVALID_CONNECTION");

      const malformed = await rejection(admin.get("not-a-uuid"));
      expect(malformed.code).toBe("WYRD_SPEC_400_VALIDATION");

      const reader = OperatorConnections.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_oc_reader", ["operators:read"]),
      });
      expect(await reader.get(id)).toEqual(await admin.get(id));
      const denied = await rejection(reader.disable(id));
      expect(denied.code).toBe("WYRD_PERMISSION_403_DENIED_RBAC");
      const outsider = OperatorConnections.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_oc_outsider", ["cards:read"]),
      });
      expect((await rejection(outsider.list())).code).toBe(
        "WYRD_PERMISSION_403_DENIED_RBAC",
      );

      // Another tenant's administrator sees none of these connections and
      // cannot read, rotate, or disable one by its exact ID.
      const foreign = OperatorConnections.connect({
        serverUrl: server.baseUrl,
        credential: server.bootstrapServiceInTenant(
          server.seedTenant("ts-oc-other"),
          ["admin"],
          "ts-oc-foreign",
        ),
      });
      expect(await foreign.list()).toEqual([]);
      for (const attempt of [
        () => foreign.get(id),
        () => foreign.update(id, { provider: "slack", bot_token: "xoxb-foreign" }),
        () => foreign.disable(id),
      ]) {
        expect((await rejection(attempt())).code).toBe("WYRD_OPERATOR_404_CONNECTION_NOT_FOUND");
      }
      expect((await admin.get(id)).status, "the foreign attempts changed nothing").toBe("active");
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
