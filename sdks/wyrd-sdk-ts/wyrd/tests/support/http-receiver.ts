import { once } from "node:events";
import { createServer, type IncomingHttpHeaders, type Server } from "node:http";
import type { AddressInfo } from "node:net";

/** One request a {@link Receiver} served. */
export interface ReceivedRequest {
  readonly method: string;
  readonly path: string;
  readonly headers: IncomingHttpHeaders;
  readonly body: string;
}

/** The answer a {@link Receiver} sends: JSON for an object, verbatim for a string. */
export interface Reply {
  readonly status: number;
  readonly body?: unknown;
  readonly contentType?: string;
}

/**
 * A local HTTP endpoint standing in for a provider, a judge, or an Operator
 * hook. It answers each request with `reply` and records it, and
 * {@link Receiver.requests} waits on arrival events rather than polling.
 */
export class Receiver {
  readonly received: ReceivedRequest[] = [];

  private constructor(
    readonly url: string,
    readonly server: Server,
  ) {}

  /** Listen on a loopback port, answering every request with `reply`. */
  static async start(
    reply: (request: ReceivedRequest) => Reply = () => ({ status: 204 }),
  ): Promise<Receiver> {
    let receiver: Receiver | undefined;
    const server = createServer((request, response) => {
      const chunks: Buffer[] = [];
      request.on("data", (chunk: Buffer) => chunks.push(chunk));
      request.on("end", () => {
        const served: ReceivedRequest = {
          method: request.method ?? "",
          path: request.url ?? "",
          headers: request.headers,
          body: Buffer.concat(chunks).toString(),
        };
        const { status, body, contentType } = reply(served);
        const text = body === undefined || typeof body === "string" ? body : JSON.stringify(body);
        response.writeHead(status, { "content-type": contentType ?? "application/json" });
        response.end(text);
        receiver?.received.push(served);
        server.emit("served");
      });
    });
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    receiver = new Receiver(`http://127.0.0.1:${(server.address() as AddressInfo).port}`, server);
    return receiver;
  }

  /** Resolve with the first `count` requests once that many have arrived. */
  async requests(count: number): Promise<ReceivedRequest[]> {
    while (this.received.length < count) {
      await once(this.server, "served");
    }
    return this.received.slice(0, count);
  }

  /** Stop listening and drop open connections. */
  async close(): Promise<void> {
    this.server.closeAllConnections();
    this.server.close();
    await once(this.server, "close");
  }
}
