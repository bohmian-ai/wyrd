import { expect, test, vi } from "vitest";

import { Gateway, WyrdClient, accessTokenSource, wyrdFetch } from "wyrd-ai";

/** Sits where a number is required, so a serde message would quote it verbatim. */
const SENTINEL = "sk-live-typescript-surface-sentinel";

const options = { client: WyrdClient.connect({ serverUrl: "http://127.0.0.1:1", credential: "wyrd_unit_test_key" }) };

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

/** Make the ambient client one that mints a new token on each ask, counting how often it is resolved. */
function mintingAmbientClient(): { connects: () => number } {
  let minted = 0;
  const client = { accessToken: async () => `token-${++minted}` } as unknown as WyrdClient;
  const connect = vi.spyOn(WyrdClient, "connect").mockReturnValue(client);
  return { connects: () => connect.mock.calls.length };
}

test("accessTokenSource resolves the ambient client once and asks it on every call", async () => {
  const ambient = mintingAmbientClient();
  try {
    const source = accessTokenSource();

    expect([await source(), await source()]).toEqual(["token-1", "token-2"]);
    expect(ambient.connects()).toBe(1);
  } finally {
    vi.restoreAllMocks();
  }
});

test("wyrdFetch resolves the ambient client once and sends a fresh access token on every request", async () => {
  const ambient = mintingAmbientClient();
  const sent: (string | null)[][] = [];
  vi.stubGlobal("fetch", async (_input: string | URL | Request, init?: RequestInit) => {
    const headers = new Headers(init?.headers);
    sent.push([headers.get("x-wyrd-access-token"), headers.get("accept")]);
    return new Response("{}");
  });
  try {
    const authenticated = wyrdFetch();
    await authenticated("http://wyrd/mcp", { headers: { accept: "application/json" } });
    await authenticated("http://wyrd/mcp");

    expect(ambient.connects()).toBe(1);
  } finally {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  }

  expect(sent).toEqual([
    ["Bearer token-1", "application/json"],
    ["Bearer token-2", null],
  ]);
});
