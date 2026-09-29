import { startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Gateway, WyrdError, type ProviderDeployment } from "@wyrd/sdk";

/** Await a promise expected to reject with a structured catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/**
 * Submit or delete one provider credential over the HTTP operation the CLI uses.
 *
 * The SDK exposes no credential mutation — submitting, rotating, revoking, and
 * deleting are CLI and scoped MCP paths, because a name-only revoke or delete
 * cannot be told apart from one aimed at a managed secret. A journey that needs
 * a credential in place therefore calls the public route directly, as the CLI
 * does, and the raw response lets a caller assert a conflict or a denial.
 */
async function credentialRequest(
  baseUrl: string,
  token: string,
  name: string,
  body?: unknown,
): Promise<Response> {
  return fetch(`${baseUrl}/v1/admin/gateway/provider-credentials/${name}`, {
    method: body === undefined ? "DELETE" : "PUT",
    headers: {
      "x-wyrd-access-token": `Bearer ${token}`,
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}

describe("Gateway administration journey", () => {
  it("manages credentials, deployments, and capture policy", async () => {
    const server = startTestServer();
    try {
      const gateway = Gateway.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
      });

      const put = (await (
        await credentialRequest(server.baseUrl, server.token, "ts-openai", {
          name: "ts-openai",
          provider: "openai",
          source: { environment: { binding: "test-provider-key" } },
        })
      ).json()) as { state: string };
      expect(put.state).toBe("active");

      const view = await gateway.credential("ts-openai");
      expect(view).toEqual(put);
      expect(view.source).toEqual({ environment: { binding: "test-provider-key" } });
      expect(Object.keys(view).sort()).toEqual([
        "created_at",
        "name",
        "provider",
        "revoked_at",
        "rotated_at",
        "source",
        "state",
        "updated_at",
      ]);

      const deployment: ProviderDeployment = {
        name: "ts-primary",
        model: { provider: "openai", model: "gpt-4o" },
        adapter: "openai",
        auth: { bearer: { credential: "ts-openai" } },
        capabilities: ["chat_completions"],
        routing_weight: 1,
      };
      expect(await gateway.putDeployment(deployment)).toEqual(deployment);
      expect((await gateway.deployments()).map((item) => item.name)).toEqual(["ts-primary"]);
      expect((await gateway.credentials()).map((item) => item.name)).toEqual(["ts-openai"]);

      const conflict = (await (
        await credentialRequest(server.baseUrl, server.token, "ts-openai")
      ).json()) as { code: string };
      expect(conflict.code).toBe("WYRD_GATEWAY_409_RESOURCE_CONFLICT");

      const invalid = await rejection(
        gateway.putDeployment({ ...deployment, capabilities: [] }),
      );
      expect(invalid.status).toBe(400);

      const denied = Gateway.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_gateway_reader", ["gateway:read"]),
      });
      expect((await denied.deployment("ts-primary")).name).toBe("ts-primary");
      expect((await rejection(denied.deleteDeployment("ts-primary"))).status).toBe(403);

      await gateway.deleteDeployment("ts-primary");
      for (const _ of [0, 1]) {
        expect(
          (await credentialRequest(server.baseUrl, server.token, "ts-openai")).status,
        ).toBe(204);
      }
      const missing = await rejection(gateway.credential("ts-openai"));
      expect(missing.code).toBe("WYRD_GATEWAY_404_RESOURCE_NOT_FOUND");

      expect((await gateway.capturePolicy()).mode).toBe("disabled");
      const capture = await gateway.putCapturePolicy({
        mode: "payload",
        payload_fields: ["request"],
      });
      expect(capture.mode).toBe("payload");
      expect(await gateway.capturePolicy()).toEqual(capture);
    } finally {
      server.shutdown();
    }
  }, 60_000);
});
