import { expect, test } from "vitest";

import { Gateway } from "@wyrd/sdk";

/** Sits where a number is required, so a serde message would quote it verbatim. */
const SENTINEL = "sk-live-typescript-surface-sentinel";

const options = { serverUrl: "http://127.0.0.1:1", credential: "wyrd_unit_test_key" };

// The restriction is structural, so it must hold for a JavaScript caller with
// no type checker.
test("offers no provider credential mutation", () => {
  const gateway = Gateway.connect(options) as unknown as Record<string, unknown>;

  for (const method of ["putCredential", "revokeCredential", "deleteCredential"]) {
    expect(gateway[method], method).toBeUndefined();
  }
});

test("rejects an invalid credential name locally", async () => {
  await expect(Gateway.connect(options).credential("Not A Name!")).rejects.toMatchObject({
    code: "WYRD_SPEC_400_VALIDATION",
  });
});

// A gateway write body can carry a provider key: the rejection names the
// argument and repeats none of the body.
test("rejects a malformed body without echoing it", async () => {
  const refusal = Gateway.connect(options).putDeployment({
    name: "openai-primary",
    model: { provider: "openai", model: "gpt-4o" },
    adapter: "openai",
    auth: { bearer: { credential: "openai-key" } },
    capabilities: ["chat_completions"],
    routing_weight: SENTINEL,
  } as never);

  await expect(refusal).rejects.toMatchObject({
    code: "WYRD_SPEC_400_VALIDATION",
    details: { field: "deployment" },
  });
  const error = await refusal.catch((reason: unknown) => reason);
  expect(JSON.stringify(error) + String(error)).not.toContain(SENTINEL);
});
