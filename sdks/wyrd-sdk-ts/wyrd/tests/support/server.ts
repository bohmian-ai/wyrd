import { resolve } from "node:path";

import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { test as base, vi } from "vitest";

import { type ReceivedRequest, Receiver, type Reply } from "./http-receiver.js";

/** Path of a checked-in file under the repository-root `fixtures/` tree. */
export function fixture(path: string): string {
  return resolve(import.meta.dirname, "../../../../../fixtures", path);
}

/** How a story file starts its server. */
export interface ServerOptions {
  /** Answers the Gateway adapters and the Eval judge send to the server's provider upstream. */
  readonly provider?: (request: ReceivedRequest) => Reply;
  /** Run Drift baseline fitting, scheduled Verifier runs, and Operators. */
  readonly verificationRuntime?: boolean;
}

/**
 * Export `server`'s address and writer key the way a deployment's environment
 * does, so every SDK and CLI call resolves them without arguments.
 */
function deploy(server: NativeWyrdTestServer): void {
  vi.stubEnv("WYRD_SERVER_URL", server.baseUrl);
  vi.stubEnv("WYRD_GRPC_URL", server.grpcUrl);
  vi.stubEnv("WYRD_API_KEY", server.apiKey);
  for (const name of ["WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"]) {
    vi.stubEnv(name, undefined);
  }
}

/**
 * `test` with one real Wyrd server per story file, started with `options`.
 *
 * `provider` is the local upstream the server calls, started before the
 * server so its address is the server's provider base URL. Each test starts
 * from the deployment environment, so a test that switches principal with
 * `vi.stubEnv` leaves the next test unaffected.
 */
export function serverTest(options: ServerOptions = {}) {
  return base.extend<{ provider: Receiver; server: NativeWyrdTestServer; deployment: void }>({
    provider: [
      async ({}, use) => {
        const provider = await Receiver.start(options.provider);
        await use(provider);
        await provider.close();
      },
      { scope: "file" },
    ],
    server: [
      async ({ provider }, use) => {
        const server = startTestServer(provider.url, true, options.verificationRuntime);
        deploy(server);
        await use(server);
        vi.unstubAllEnvs();
        server.shutdown();
      },
      { scope: "file" },
    ],
    deployment: [
      async ({ server }, use) => {
        deploy(server);
        await use();
      },
      { auto: true },
    ],
  });
}
