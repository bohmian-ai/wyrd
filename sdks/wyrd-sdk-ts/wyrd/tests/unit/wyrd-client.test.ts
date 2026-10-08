import { expect, test } from "vitest";

import { WyrdClient } from "@wyrd/sdk";

const CREDENTIAL = "wyrd_test_actor";

test("on behalf of an unknown audience is refused locally", async () => {
  const client = WyrdClient.connect({ serverUrl: "http://127.0.0.1:9", credential: CREDENTIAL });

  // @ts-expect-error `storage` is outside the closed audience set; JavaScript callers can still pass it.
  await expect(client.onBehalfOf("subject-token", { audience: "storage" })).rejects.toMatchObject({
    code: "WYRD_SPEC_400_VALIDATION",
  });
});

test("grpc endpoint derives from the server url unless overridden", () => {
  const derived = WyrdClient.connect({ serverUrl: "https://wyrd.example.com/", credential: CREDENTIAL });
  const overridden = WyrdClient.connect({
    serverUrl: "https://wyrd.example.com",
    credential: CREDENTIAL,
    grpcUrl: "https://grpc.example.com:443",
  });

  expect([derived.serverUrl, derived.grpcUrl]).toEqual(["https://wyrd.example.com", "https://wyrd.example.com:50051"]);
  expect(overridden.grpcUrl).toBe("https://grpc.example.com:443");
});
