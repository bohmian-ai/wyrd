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
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
