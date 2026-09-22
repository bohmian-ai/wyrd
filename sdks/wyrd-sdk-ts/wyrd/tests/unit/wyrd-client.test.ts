import { describe, expect, it } from "vitest";

import { WyrdClient, WyrdError } from "@wyrd/sdk";

const UNREACHABLE = { serverUrl: "http://127.0.0.1:9", credential: "wyrd_test_actor" };

describe("WyrdClient", () => {
  it("onBehalfOf rejects an unknown audience with the validation WyrdError", async () => {
    const client = WyrdClient.connect(UNREACHABLE);
    const caught: unknown = await client
      .onBehalfOf("subject-token", { audience: "storage" as "wyrd" })
      .catch((error: unknown) => error);
    expect(caught).toBeInstanceOf(WyrdError);
    expect((caught as WyrdError).code).toBe("WYRD_SPEC_400_VALIDATION");
  });

  it("onBehalfOf runs the exchange in Rust", async () => {
    const client = WyrdClient.connect(UNREACHABLE);
    const caught: unknown = await client
      .onBehalfOf("subject-token")
      .catch((error: unknown) => error);
    expect(caught).toBeInstanceOf(WyrdError);
    expect((caught as WyrdError).code).not.toBe("WYRD_SPEC_400_VALIDATION");
  });
});
