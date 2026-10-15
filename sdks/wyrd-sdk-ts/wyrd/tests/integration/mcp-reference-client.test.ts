/**
 * Server conformance journey through the official MCP SDK, the reference
 * client, plus the OpenAI Agents SDK's MCP server: both reach Wyrd's `/mcp`
 * authenticated only by `WyrdClient.httpAuth`.
 */
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { MCPServerStreamableHttp } from "@openai/agents-core";
import { expect, vi } from "vitest";

import { WyrdClient } from "wyrd-ai";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const test = serverTest();

test("the official MCP SDK lists the full catalog and validates a structured read", async ({ server }) => {
  // Wyrd serves only protocol 2026-07-28, which the SDK reaches by discovery.
  const client = new Client({ name: "wyrd-ts-journey", version: "1.0.0" }, { versionNegotiation: { mode: "auto" } });
  await client.connect(
    new StreamableHTTPClientTransport(new URL("/mcp", server.baseUrl), {
      fetch: WyrdClient.connect().httpAuth,
    }),
  );
  try {
    // The SDK rejects the whole list when its result or any tool is malformed,
    // and drops any tool it cannot call; none may be dropped.
    const { tools } = await client.listTools();
    // The raw single-page server result is the independent reference.
    const advertised = await client.request({ method: "tools/list" });
    expect(advertised.nextCursor).toBeUndefined();
    const names = (list: { name: string }[]) => list.map((tool) => tool.name).sort();
    expect(names(tools)).toEqual(names(advertised.tools));

    // The SDK checks structured content against the advertised output schema.
    const guide = await client.callTool({ name: "wyrd.guide", arguments: { topic: "overview" } });
    expect(guide.isError).not.toBe(true);
    expect(guide.structuredContent).toMatchObject({ topic: "overview" });
  } finally {
    await client.close();
  }
});

test("the OpenAI Agents SDK reaches /mcp and calls a read tool", async ({ server }) => {
  const mcp = new MCPServerStreamableHttp({
    name: "wyrd",
    url: new URL("/mcp", server.baseUrl).href,
    fetch: WyrdClient.connect().httpAuth,
  });
  await mcp.connect();
  try {
    expect((await mcp.listTools()).map((tool) => tool.name)).toContain("wyrd.guide");
    const guide = await mcp.callTool("wyrd.guide", { topic: "overview" });
    expect(JSON.stringify(guide)).toContain("overview");
  } finally {
    await mcp.close();
  }
});
