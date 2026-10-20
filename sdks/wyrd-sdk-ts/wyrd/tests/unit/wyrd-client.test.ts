import { expect, test } from "vitest";

import { WyrdClient } from "wyrd-ai";

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

test("a browser-session client is validated locally and has no bearer", async () => {
  const session = WyrdClient.forBrowserSession({
    serverUrl: "http://127.0.0.1:9",
    cookie: "wyrd_session_acme=opaque.1",
    origin: "https://ui.example.com",
  });

  expect(session.serverUrl).toBe("http://127.0.0.1:9");
  await expect(session.accessToken()).rejects.toMatchObject({ code: "WYRD_CLIENT_400_CONFIG_INVALID" });
  for (const cookie of ["theme=dark", "wyrd_session_acme=a; wyrd_session_beta=b"]) {
    expect(() =>
      WyrdClient.forBrowserSession({ serverUrl: "http://127.0.0.1:9", cookie, origin: "https://ui.example.com" }),
    ).toThrow(expect.objectContaining({ code: "WYRD_CLIENT_400_CONFIG_INVALID" }));
  }
});
