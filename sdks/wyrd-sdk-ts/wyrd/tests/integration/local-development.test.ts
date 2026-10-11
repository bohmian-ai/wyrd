import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider } from "@opentelemetry/sdk-trace-base";
import { afterEach, expect, test as bootstrapTest, vi } from "vitest";

import { Cards, WyrdClient, WyrdState } from "@wyrd/sdk";

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

/** A fresh Node runtime uses only the installed CLI's saved deployment. */
bootstrapTest.skipIf(!process.env.WYRD_LOCAL_DEV_CONFIG_HOME)("cli_bootstrap_saved_config", async () => {
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
  try {
    const cards = Cards.connect({ client: WyrdClient.connect() });
    const receipt = await cards.registerFromPath(fixture("cards/gateway_inference/ask-prompt.yaml"));
    expect((await cards.get(receipt.root)).metadata.name).toBe("ask-prompt");
  } finally {
    vi.unstubAllEnvs();
  }
});
