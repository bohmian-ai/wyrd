import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider } from "@opentelemetry/sdk-trace-base";
import { afterEach, expect, test as bootstrapTest, vi } from "vitest";

import { Bifrost, Cards, WyrdClient, WyrdState } from "wyrd-ai";

import { COMPLETION, work } from "../support/local-development.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 120_000 });

/** A local deployment that verifies in real time and routes the Gateway to an upstream answering `hi`. */
const test = serverTest({ provider: () => ({ status: 200, body: COMPLETION }), verificationRuntime: true });

afterEach(() => {
  trace.disable();
  context.disable();
});

test("admin key completes the local workflow", async ({ server }) => {
  const client = WyrdClient.connect({
    serverUrl: server.baseUrl,
    credential: server.tenantAdminKey(),
    grpcUrl: server.grpcUrl,
  });

  const worked = await work(client);

  trace.disable();
  const own = new BasicTracerProvider();
  trace.setGlobalTracerProvider(own);
  await expect(WyrdState.fromPath(worked.bundle, { client }).startTelemetry()).rejects.toMatchObject({
    code: "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS",
  });
});

/** Stub the environment so a fresh Node runtime authenticates with only the installed CLI's saved deployment. */
function useSavedDeployment(): void {
  vi.stubEnv("WYRD_CONFIG_HOME", process.env.WYRD_LOCAL_DEV_CONFIG_HOME);
  for (const name of [
    "WYRD_API_KEY",
    "WYRD_ACCESS_TOKEN",
    "WYRD_WORKLOAD_TOKEN",
    "WYRD_TENANT",
    "WYRD_SERVER_URL",
    "WYRD_GRPC_URL",
  ]) {
    vi.stubEnv(name, undefined);
  }
}

/** A fresh Node runtime uses only the installed CLI's saved deployment. */
bootstrapTest.skipIf(!process.env.WYRD_LOCAL_DEV_CONFIG_HOME)("cli_bootstrap_saved_config", async () => {
  useSavedDeployment();
  try {
    const cards = await Cards.connect({ client: WyrdClient.connect() });
    const receipt = await cards.registerFromPath(fixture("cards/gateway_inference/ask-prompt.yaml"));
    expect((await cards.get(receipt.root)).metadata.name).toBe("ask-prompt");
  } finally {
    vi.unstubAllEnvs();
  }
});

/** One supported native call per deployment of `examples/gateway-setup/gateway.json`, by deployment name. */
const INFERENCES = {
  "openai-prod": ["/v1/chat/completions", { model: "openai-prod/gpt-4o", messages: [{ role: "user", content: "hi" }] }],
  "claude-prod": [
    "/v1/messages",
    { model: "claude-prod/claude-sonnet-4-5", max_tokens: 16, messages: [{ role: "user", content: "hi" }] },
  ],
  "gemini-team": [
    "/v1beta/models/gemini-team/gemini-2.5-flash:generateContent",
    { contents: [{ role: "user", parts: [{ text: "hi" }] }] },
  ],
  "judge-pool": [
    "/v1/systemone",
    {
      model: "judge-pool/jev-latest",
      state: "hi",
      questions: { helpful: { type: "noul", instructions: "Is it helpful?" } },
    },
  ],
} as const;

/** POST `deployment`'s inference to the Gateway with the saved deployment's access token. */
async function infer(deployment: keyof typeof INFERENCES): Promise<{ status: number; reply: unknown }> {
  const [route, body] = INFERENCES[deployment];
  const client = WyrdClient.connect();
  const response = await fetch(`${client.serverUrl}${route}`, {
    method: "POST",
    headers: { authorization: `Bearer ${await client.accessToken()}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: response.status, reply: await response.json() };
}

/**
 * Every deployment the installed CLI wrote answers once through the saved
 * deployment, and the retained calls name each resolved deployment with the
 * caller's model unchanged.
 */
bootstrapTest.skipIf(!process.env.WYRD_LOCAL_DEV_CONFIG_HOME)("cli_gateway_setup", async () => {
  useSavedDeployment();
  try {
    for (const deployment of Object.keys(INFERENCES) as Array<keyof typeof INFERENCES>) {
      const { status, reply } = await infer(deployment);
      expect(status, `${deployment}: ${JSON.stringify(reply)}`).toBe(200);
    }
    const bifrost = await Bifrost.connect();
    const retained = async () =>
      (
        await bifrost.sql(
          "SELECT DISTINCT resolved_deployment, requested_model['model'] AS model FROM vala.gateway.calls WHERE outcome = 'succeeded' ORDER BY resolved_deployment",
        )
      )
        .toArrow()
        .toArray()
        .map((row) => row.toJSON());
    await expect.poll(retained, { timeout: 60_000, interval: 500 }).toEqual([
      { resolved_deployment: "claude-prod", model: "claude-sonnet-4-5" },
      { resolved_deployment: "gemini-team", model: "gemini-2.5-flash" },
      { resolved_deployment: "judge-pool", model: "jev-latest" },
      { resolved_deployment: "openai-prod", model: "gpt-4o" },
    ]);
  } finally {
    vi.unstubAllEnvs();
  }
});

/**
 * After `wyrd gateway credential revoke openai-key`, its deployment refuses
 * with the Gateway's redacted upstream-unavailable code, which names no
 * credential state.
 */
bootstrapTest.skipIf(!process.env.WYRD_LOCAL_DEV_CONFIG_HOME)("cli_gateway_revoked", async () => {
  useSavedDeployment();
  try {
    expect(await infer("openai-prod")).toMatchObject({
      status: 502,
      reply: { error: { code: "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE" } },
    });
  } finally {
    vi.unstubAllEnvs();
  }
});
