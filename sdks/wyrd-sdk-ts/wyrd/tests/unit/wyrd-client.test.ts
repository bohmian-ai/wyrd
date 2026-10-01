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

  it("derives the gRPC endpoint from serverUrl unless overridden", () => {
    const derived = WyrdClient.connect({
      serverUrl: "https://wyrd.example.com/",
      credential: "wyrd_test_actor",
    });
    expect(derived.serverUrl).toBe("https://wyrd.example.com");
    expect(derived.grpcUrl).toBe("https://wyrd.example.com:50051");

    const overridden = WyrdClient.connect({
      serverUrl: "https://wyrd.example.com",
      credential: "wyrd_test_actor",
      grpcUrl: "https://grpc.example.com:443",
    });
    expect(overridden.grpcUrl).toBe("https://grpc.example.com:443");
  });
});
