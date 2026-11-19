import { once } from "node:events";
import { createServer, type IncomingMessage } from "node:http";
import { createServer as createGrpcServer } from "node:http2";

import { expect, test, vi } from "vitest";

import { Bifrost, Cards, WyrdClient } from "wyrd-ai";

/**
 * A loopback server that never answers: each request it receives waits, and
 * resolves `closed` once the client closes it. A Bifrost handle also needs a
 * gRPC endpoint to connect to; it accepts the connection and nothing else.
 */
async function silentServer() {
  const received: { url: string; closed: Promise<void> }[] = [];
  const server = createServer((req: IncomingMessage) => {
    received.push({ url: req.url ?? "", closed: once(req.socket, "close").then(() => undefined) });
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const grpc = createGrpcServer();
  grpc.listen(0, "127.0.0.1");
  await once(grpc, "listening");
  const { port } = server.address() as { port: number };
  const client = WyrdClient.connect({
    serverUrl: `http://127.0.0.1:${port}`,
    grpcUrl: `http://127.0.0.1:${(grpc.address() as { port: number }).port}`,
    credential: "access-token-1",
  });
  /** The first request for `path`, once it arrives. */
  const arrived = (path: string) =>
    vi.waitFor(() => {
      const request = received.find((sent) => sent.url.startsWith(path));
      if (request === undefined) {
        throw new Error(`no request for ${path} yet`);
      }
      return request;
    });
  const close = () => {
    server.closeAllConnections();
    server.close();
    grpc.close();
  };
  return { client, arrived, received, close };
}

test.for<readonly [string, string, (client: WyrdClient, signal: AbortSignal) => Promise<unknown>]>([
  [
    "a starting query",
    "/v1/query",
    async (client, signal) => (await Bifrost.connect({ client })).stream("SELECT 1", [], { signal }),
  ],
  [
    "a pending Card list",
    "/v1/cards",
    async (client, signal) => (await Cards.connect({ client })).list({}, { signal }),
  ],
])("aborting %s closes its request and throws the signal's reason", async ([, path, call]) => {
  const server = await silentServer();
  try {
    const controller = new AbortController();
    const pending = call(server.client, controller.signal);
    const request = await server.arrived(path);

    controller.abort(new Error("superseded"));

    await expect(pending).rejects.toThrow("superseded");
    await request.closed;
  } finally {
    server.close();
  }
});

test("an already aborted signal sends nothing", async () => {
  const server = await silentServer();
  try {
    const signal = AbortSignal.abort(new Error("left"));
    await expect((await Cards.connect({ client: server.client })).list({}, { signal })).rejects.toThrow("left");
    expect(server.received).toEqual([]);
  } finally {
    server.close();
  }
});
