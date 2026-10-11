import { startTestServer } from "@wyrd/testing";
import { afterAll, beforeAll, vi } from "vitest";

/** One running in-process Wyrd test server. */
export type TestServer = ReturnType<typeof startTestServer>;

/**
 * Start one test server for the calling test file and export it the way a
 * deployment does, so `Bifrost.connect()` and `TableConfig.describe()` resolve
 * the server and key without arguments.
 *
 * Returns a getter, because the server starts in `beforeAll`.
 */
export function useTestServer(): () => TestServer {
  let server: TestServer | undefined;
  beforeAll(() => {
    server = startTestServer();
    vi.stubEnv("WYRD_SERVER_URL", server.baseUrl);
    vi.stubEnv("WYRD_GRPC_URL", server.grpcUrl);
    vi.stubEnv("WYRD_API_KEY", server.apiKey);
  });
  afterAll(() => {
    vi.unstubAllEnvs();
    server?.shutdown();
  });
  return () => {
    if (server === undefined) {
      throw new Error("the test server starts in beforeAll");
    }
    return server;
  };
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
