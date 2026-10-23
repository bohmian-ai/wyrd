import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import {
  Bifrost,
  type CardRef,
  type Cards,
  type DeclarationReceipt,
  Principals,
  type SpanRecord,
  verify,
  type VerificationExecutionState,
  type VerificationResult,
  WyrdClient,
  type WorkloadKind,
} from "wyrd-ai";
import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { test as base, vi } from "vitest";
import { z } from "zod";

import { type ReceivedRequest, Receiver, type Reply } from "./http-receiver.js";

/** `span` and each ancestor its parent ids reach within `timeline`, nearest first. */
export function ancestry(timeline: readonly SpanRecord[], span: SpanRecord): SpanRecord[] {
  const byId = new Map(timeline.map((each) => [each.span_id, each]));
  const chain = [span];
  for (let next = byId.get(span.parent_span_id ?? ""); next !== undefined; next = byId.get(next.parent_span_id ?? "")) {
    chain.push(next);
  }
  return chain;
}

/** The span tree of `traceId` as the administrator reads it after a flush. */
export async function timeline(server: { flushBifrost(): void }, traceId: string): Promise<SpanRecord[]> {
  server.flushBifrost();
  return (await Bifrost.connect()).traceTimeline(traceId);
}

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
export type DeclaredRef = CardRef & { readonly uid: string };

/**
 * The declared reference of the Card named `name` in `receipt`, the root by
 * default. Throws when the receipt declared no such Card, so a story never
 * proceeds with a missing reference.
 */
export function declared(receipt: DeclarationReceipt, name: string = receipt.root.name): DeclaredRef {
  const ref = receipt.outcomes.find((outcome) => outcome.card_ref.name === name)?.card_ref;
  if (!ref?.uid) {
    throw new Error(`the receipt declared no Card named ${name}`);
  }
  return { ...ref, uid: ref.uid };
}

/** A runtime account's principal id and one plaintext key. */
export interface RuntimeAccount {
  readonly principalId: string;
  readonly key: string;
}

/**
 * The `kind`/`name` family's runtime account in `client`'s tenant, the
 * ambient administrator's by default, with one new key. Declaration creates
 * no principal, so the account restricted to the family is created holding
 * only `workload` on first use and issued one more key afterwards.
 */
export async function runtimeAccount(
  card: { readonly kind: WorkloadKind; readonly name: string },
  client?: WyrdClient,
): Promise<RuntimeAccount> {
  const principals = await Principals.connect({ client });
  const name = `${card.name}-runtime`;
  const [existing] = (await principals.list({ kind: "service", name })).principals;
  if (existing !== undefined) {
    const issued = await principals.issueCredential(existing.principal_id);
    return { principalId: existing.principal_id, key: issued.credential };
  }
  const created = await principals.createServicePrincipal({
    name,
    roles: ["workload"],
    workload_restriction: { mode: "family", kind: card.kind, name: card.name },
  });
  return { principalId: created.principal_id, key: created.credential };
}

/** The one verification binding `owner` declares, read from its served status. */
export async function binding(cards: Cards, owner: CardRef): Promise<string> {
  const [id] = (await cards.get(owner)).status?.verification?.binding_ids ?? [];
  if (id === undefined) {
    throw new Error(`${owner.name} declares no verification binding`);
  }
  return id;
}

/**
 * The first retained result of `bindingId`, read back as the administrator:
 * publish captured rows until `vala.verification.results` names one, then
 * wait for that execution's typed result by its ID.
 */
export async function scheduledResult(
  server: { flushBifrost(): void },
  bindingId: string,
  timeoutMs: number,
): Promise<VerificationResult> {
  const bifrost = await Bifrost.connect();
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    server.flushBifrost();
    const [row] = await bifrost.sql(
      "SELECT execution_id FROM vala.verification.results WHERE binding_id = $1",
      [bindingId],
      z.object({ execution_id: z.string() }),
    );
    if (row !== undefined) {
      return (await verify.Execution.connect(row.execution_id)).wait({ timeoutMs });
    }
    if (Date.now() >= deadline) {
      throw new Error(`binding ${bindingId} retained no result within ${timeoutMs} ms`);
    }
    await sleep(500);
  }
}

/**
 * The newest completed execution of group `bindingId` other than `after`, read
 * as the administrator: publish captured rows so the group poller discovers
 * them, then read the binding's `last_execution_id` until that execution
 * completes with a result.
 */
export async function groupExecution(
  server: { flushBifrost(): void; readonly baseUrl: string },
  bindingId: string,
  timeoutMs: number,
  after?: string,
): Promise<VerificationExecutionState> {
  const token = await WyrdClient.connect().accessToken();
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    server.flushBifrost();
    const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${bindingId}`, {
      headers: { "x-wyrd-access-token": `Bearer ${token}` },
    });
    const { last_execution_id: latest } = (await response.json()) as { last_execution_id: string | null };
    if (latest !== null && latest !== after) {
      const state = await (await verify.Execution.connect(latest)).status();
      if (state.status === "completed" && state.result) {
        return state;
      }
    }
    if (Date.now() >= deadline) {
      throw new Error(`group binding ${bindingId} completed no new execution within ${timeoutMs} ms`);
    }
    await sleep(500);
  }
}

/** How a story file starts its server. */
export interface ServerOptions {
  /** Answers the Gateway adapters and Verifier Agents send to the server's provider upstream. */
  readonly provider?: (request: ReceivedRequest) => Reply;
  /** Run Drift baseline fitting, scheduled Verifier runs, and Operators. */
  readonly verificationRuntime?: boolean;
  /** Access-token lifetime in seconds, verified with no clock-skew allowance. */
  readonly accessTtlSeconds?: number;
  /** Scribe outbox capacity in items; the production capacity by default. */
  readonly scribeOutboxCapacity?: number;
  /** Start a fresh server for every test instead of one per story file. */
  readonly perTest?: boolean;
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
 * server and scoped like it, since Vitest shares a file-scoped fixture by
 * name across every `serverTest` in the file; with `options.provider` set, its address is the server's provider
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
      { scope: options.perTest ? "test" : "file" },
    ],
    server: [
      async ({ provider }, use) => {
        const server = startTestServer({
          providerBaseUrl: options.provider && provider.url,
          verificationRuntime: options.verificationRuntime,
          accessTtlSeconds: options.accessTtlSeconds,
          scribeOutboxCapacity: options.scribeOutboxCapacity,
        });
        deploy(server);
        await use(server);
        vi.unstubAllEnvs();
        server.shutdown();
      },
      { scope: options.perTest ? "test" : "file" },
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
