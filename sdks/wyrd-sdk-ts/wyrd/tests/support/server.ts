import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { type CardRef, type RegistrationReceipt, wyrdFetch } from "wyrd-ai";
import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { test as base, vi } from "vitest";

import { type ReceivedRequest, Receiver, type Reply } from "./http-receiver.js";

/** Path of a checked-in file under the repository-root `fixtures/` tree. */
export function fixture(path: string): string {
  return resolve(import.meta.dirname, "../../../../../fixtures", path);
}

/**
 * One Arrow column's values as plain JSON, so a column read back from the
 * server compares with the column written whatever its chunking or integer
 * width. Every number becomes its exact decimal string, so a 32-bit `number`
 * written equals the 64-bit `bigint` it is stored and read back as.
 */
export function columnValues(column: { toJSON(): unknown } | null): unknown {
  return JSON.parse(
    JSON.stringify(column, (_key, value: unknown) =>
      typeof value === "bigint" || typeof value === "number" ? String(value) : value,
    ),
  );
}

/** A Card reference the registry resolved, so it always carries the Card's `uid`. */
export type RegisteredRef = CardRef & { readonly uid: string };

/**
 * The registered reference of the Card named `name` in `receipt`, the root by
 * default. Throws when the receipt registered no such Card, so a story never
 * proceeds with a missing reference.
 */
export function registered(receipt: RegistrationReceipt, name: string = receipt.root.name): RegisteredRef {
  const ref = receipt.outcomes.find((outcome) => outcome.card_ref.name === name)?.card_ref;
  if (!ref?.uid) {
    throw new Error(`the receipt registered no Card named ${name}`);
  }
  return { ...ref, uid: ref.uid };
}

/** How a story file starts its server. */
export interface ServerOptions {
  /** Answers the Gateway adapters and the Eval judge send to the server's provider upstream. */
  readonly provider?: (request: ReceivedRequest) => Reply;
  /** Run Drift baseline fitting, scheduled Verifier runs, and Operators. */
  readonly verificationRuntime?: boolean;
  /** Access-token lifetime in seconds, verified with no clock-skew allowance. */
  readonly accessTtlSeconds?: number;
}

/**
 * Export `server`'s address and writer key the way a deployment's environment
 * does, so every SDK and CLI call resolves them without arguments. An empty
 * config home keeps the developer's saved logins out of every story.
 */
function deploy(server: NativeWyrdTestServer): void {
  vi.stubEnv("WYRD_CONFIG_HOME", mkdtempSync(join(tmpdir(), "wyrd-ts-config-")));
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
 * server; with `options.provider` set, its address is the server's provider
 * base URL, and without it the server keeps its default upstreams. Each test starts
 * from the deployment environment, so a test that removes the ambient
 * credential with `vi.stubEnv` leaves the next test unaffected.
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
        const server = startTestServer({
          providerBaseUrl: options.provider && provider.url,
          verificationRuntime: options.verificationRuntime,
          accessTtlSeconds: options.accessTtlSeconds,
        });
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

/**
 * The API key of a fresh service principal named `name` that the ambient
 * administrator has already revoked, so exchanging it is refused.
 *
 * Both calls go through {@link wyrdFetch}, as an operator's script would.
 */
export async function revokedApiKey(name: string): Promise<string> {
  const admin = wyrdFetch();
  const json = { "content-type": "application/json" };
  const created = await admin(`${process.env.WYRD_SERVER_URL}/v1/principals`, {
    method: "POST",
    headers: json,
    body: JSON.stringify({ name, roles: [] }),
  });
  if (!created.ok) {
    throw new Error(`creating ${name} failed with ${created.status}`);
  }
  const { principal_id, credential } = (await created.json()) as { principal_id: string; credential: string };
  const revoked = await admin(`${process.env.WYRD_SERVER_URL}/v1/principals/${principal_id}/revoke`, {
    method: "POST",
    headers: json,
    body: JSON.stringify({ principal_kind: "service", reason: "journey revokes this key" }),
  });
  if (!revoked.ok) {
    throw new Error(`revoking ${name} failed with ${revoked.status}`);
  }
  return credential;
}
