/**
 * Server conformance journey through the official MCP SDK, the reference
 * client: it drives Wyrd's `/mcp` and does not exercise `wyrd-sdk-ts`.
 */
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { expect, vi } from "vitest";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const test = serverTest();

test("the official MCP SDK lists the full catalog and validates a structured read", async ({ server }) => {
  // Wyrd serves only protocol 2026-07-28, which the SDK reaches by discovery.
  const client = new Client({ name: "wyrd-ts-journey", version: "1.0.0" }, { versionNegotiation: { mode: "auto" } });
  await client.connect(
    new StreamableHTTPClientTransport(new URL("/mcp", server.baseUrl), {
      requestInit: { headers: { "x-wyrd-access-token": `Bearer ${server.token}` } },
    }),
  );
  try {
    // The SDK rejects the whole list when its result or any tool is malformed.
    const { tools } = await client.listTools();
    // An administrator is shown write tools as well as reads.
    expect(new Set(tools.map((tool) => tool.annotations?.readOnlyHint))).toEqual(new Set([true, false]));

    // The SDK checks structured content against the advertised output schema.
    const guide = await client.callTool({ name: "wyrd.guide", arguments: { topic: "overview" } });
    expect(guide.isError).not.toBe(true);
    expect(guide.structuredContent).toMatchObject({ topic: "overview" });
  } finally {
    await client.close();
  }
});
