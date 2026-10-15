import { expect, test, vi } from "vitest";

import { Gateway, WyrdClient } from "wyrd-ai";

/** Sits where a number is required, so a serde message would quote it verbatim. */
const SENTINEL = "sk-live-typescript-surface-sentinel";

const options = { client: WyrdClient.connect({ serverUrl: "http://127.0.0.1:1", credential: "wyrd_unit_test_key" }) };

// The restriction is structural, so it must hold for a JavaScript caller with
// no type checker.
test("offers no provider credential mutation", async () => {
  const gateway = (await Gateway.connect(options)) as unknown as Record<string, unknown>;

  for (const method of ["putCredential", "revokeCredential", "deleteCredential"]) {
    expect(gateway[method], method).toBeUndefined();
  }
});

test("rejects an invalid credential name locally", async () => {
  await expect((await Gateway.connect(options)).credential("Not A Name!")).rejects.toMatchObject({
    code: "WYRD_SPEC_400_VALIDATION",
  });
});

// A gateway write body can carry a provider key: the rejection names the
// argument and repeats none of the body.
test("rejects a malformed body without echoing it", async () => {
  const refusal = (await Gateway.connect(options)).putDeployment({
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

// The token is read per request so a long-lived client survives expiry, and
// the placeholder provider keys are dropped so the gateway sees one credential.
test("httpAuth replaces provider key headers with a fresh access token on every request", async () => {
  const client = options.client;
  let minted = 0;
  vi.spyOn(client, "accessToken").mockImplementation(async () => `token-${++minted}`);
  const sent: Record<string, string>[] = [];
  vi.stubGlobal("fetch", async (_input: string | URL | Request, init?: RequestInit) => {
    sent.push(Object.fromEntries(new Headers(init?.headers)));
    return new Response("{}");
  });
  try {
    await client.httpAuth("http://wyrd/v1/chat/completions", {
      headers: {
        authorization: "Bearer wyrd",
        "x-api-key": "wyrd",
        "x-goog-api-key": "wyrd",
        accept: "application/json",
      },
    });
    await client.httpAuth(new Request("http://wyrd/mcp", { headers: { authorization: "Bearer wyrd" } }));
  } finally {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  }

  expect(sent).toEqual([
    { accept: "application/json", "x-wyrd-access-token": "Bearer token-1" },
    { "x-wyrd-access-token": "Bearer token-2" },
  ]);
});
