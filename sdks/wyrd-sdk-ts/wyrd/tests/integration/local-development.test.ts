import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider } from "@opentelemetry/sdk-trace-base";
import { afterEach, expect, vi } from "vitest";

import { WyrdClient, WyrdState } from "@wyrd/sdk";

import { COMPLETION, work } from "../support/local-development.js";
import { serverTest } from "../support/server.js";

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
