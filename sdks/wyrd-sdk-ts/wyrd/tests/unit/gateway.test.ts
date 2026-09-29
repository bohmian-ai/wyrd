import { describe, expect, it } from "vitest";

import { Gateway, WyrdError } from "@wyrd/sdk";
import type { ProviderCredentialSourceView } from "@wyrd/sdk";
import { createRequire } from "node:module";

const { connectGateway } = createRequire(import.meta.url)(
  "../../index.cjs",
) as typeof import("../../index.cjs");

const METHODS = [
  "credential",
  "credentials",
  "putDeployment",
  "deployment",
  "deployments",
  "deleteDeployment",
  "putFallbackPolicy",
  "fallbackPolicy",
  "deleteFallbackPolicy",
  "putGovernancePolicy",
  "governancePolicy",
  "deleteGovernancePolicy",
  "putCapturePolicy",
  "capturePolicy",
] as const;

const MUTATIONS = ["putCredential", "revokeCredential", "deleteCredential"] as const;

const SENTINEL = "sk-live-typescript-surface-sentinel";

const options = {
  serverUrl: "http://127.0.0.1:1",
  credential: "wyrd_unit_test_key",
};

describe("Gateway", () => {
  it("exposes every administration operation", () => {
    const gateway = Gateway.connect(options);
    for (const method of METHODS) {
      expect(typeof gateway[method]).toBe("function");
    }
  });

  // The restriction is structural rather than a runtime source check, so it
  // has to hold for a JavaScript caller with no type checker, and on the
  // exported native class as well as the wrapper.
  it("offers no provider credential mutation on either surface", () => {
    const gateway = Gateway.connect(options) as unknown as Record<string, unknown>;
    const native = connectGateway(options.serverUrl, options.credential) as unknown as Record<
      string,
      unknown
    >;
    for (const method of MUTATIONS) {
      expect(gateway[method]).toBeUndefined();
      expect(native[method]).toBeUndefined();
    }
  });

  it("reads a managed secret and exports no write type", () => {
    const view: ProviderCredentialSourceView = "managed_secret";
    expect(view).toBe("managed_secret");
  });

  it("rejects an invalid name in Rust before any request", async () => {
    const gateway = Gateway.connect(options);
    const error = await gateway.credential("Not A Name!").catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(WyrdError);
    expect((error as WyrdError).code).toBe("WYRD_SPEC_400_VALIDATION");
  });

  // A gateway write body can carry a provider key. The sentinel below sits
  // where a number is required, so serde's own message would quote it
  // verbatim; the rejection names the argument and its decode position and
  // repeats none of the body.
  it("rejects a malformed body without echoing it", async () => {
    const gateway = Gateway.connect(options);
    const error = (await gateway
      .putDeployment({
        name: "openai-primary",
        model: { provider: "openai", model: "gpt-4o" },
        adapter: "openai",
        auth: { bearer: { credential: "openai-key" } },
        capabilities: ["chat_completions"],
        routing_weight: SENTINEL,
      } as never)
      .catch((reason: unknown) => reason)) as WyrdError;
    expect(error).toBeInstanceOf(WyrdError);
    expect(error.code).toBe("WYRD_SPEC_400_VALIDATION");
    expect(JSON.stringify(error.details)).toContain("deployment");
    expect(error.message).not.toContain(SENTINEL);
    expect(JSON.stringify(error.details)).not.toContain(SENTINEL);
  });
});
