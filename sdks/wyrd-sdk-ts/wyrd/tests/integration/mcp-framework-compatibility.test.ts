import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { type FunctionTool, getAllMcpTools, MCPServerStreamableHttp, RunContext } from "@openai/agents-core";
import { expect, vi } from "vitest";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const test = serverTest();

/** Write tools only a caller holding every scope is shown; their presence proves the full catalog. */
const WRITE_TOOLS = [
  "principals.revoke_credential",
  "verification.start_run",
  "verification.execute",
  "operator_connections.create",
  "operator_connections.update",
  "operator_connections.disable",
];

/** Input-root keywords OpenAI function tools refuse, rejecting the whole tool list. */
const ROOT_COMBINATORS = ["oneOf", "anyOf", "allOf", "enum", "const", "not"];

/** The administrator's bearer in the header Wyrd's `/mcp` edge reads. */
function adminHeaders(token: string): Record<string, string> {
  return { "x-wyrd-access-token": `Bearer ${token}` };
}

test("the official MCP SDK lists the full catalog and validates a structured read", async ({ server }) => {
  // Wyrd serves only protocol 2026-07-28, which the SDK reaches by discovery.
  const client = new Client({ name: "wyrd-ts-journey", version: "1.0.0" }, { versionNegotiation: { mode: "auto" } });
  await client.connect(
    new StreamableHTTPClientTransport(new URL("/mcp", server.baseUrl), {
      requestInit: { headers: adminHeaders(server.token) },
    }),
  );
  try {
    const { tools } = await client.listTools();
    const names = tools.map((tool) => tool.name);
    expect(names).toEqual(expect.arrayContaining(WRITE_TOOLS));
    for (const tool of tools) {
      expect(tool.inputSchema.type, tool.name).toBe("object");
      for (const keyword of ROOT_COMBINATORS) {
        expect(tool.inputSchema, `${tool.name} input root carries ${keyword}`).not.toHaveProperty(keyword);
      }
    }

    // The SDK checks structured content against the advertised output schema.
    const guide = await client.callTool({ name: "wyrd.guide", arguments: { topic: "overview" } });
    expect(guide.isError).not.toBe(true);
    expect(guide.structuredContent).toMatchObject({ topic: "overview" });
  } finally {
    await client.close();
  }
});

test("the OpenAI Agents SDK converts every tool and calls a read tool", async ({ server }) => {
  const mcp = new MCPServerStreamableHttp({
    name: "wyrd",
    url: new URL("/mcp", server.baseUrl).href,
    requestInit: { headers: adminHeaders(server.token) },
  });
  await mcp.connect();
  try {
    const tools = (await getAllMcpTools({ mcpServers: [mcp] })) as FunctionTool[];
    expect(tools.map((tool) => tool.name)).toEqual(
      expect.arrayContaining(WRITE_TOOLS.map((name) => name.replace(".", "_"))),
    );
    for (const tool of tools) {
      // OpenAI's function tool name and parameter rules.
      expect(tool.name).toMatch(/^[a-zA-Z0-9_-]{1,64}$/);
      expect(tool.parameters.type, tool.name).toBe("object");
      for (const keyword of ROOT_COMBINATORS) {
        expect(tool.parameters, `${tool.name} parameters root carries ${keyword}`).not.toHaveProperty(keyword);
      }
    }

    const guide = tools.find((tool) => tool.name === "wyrd_guide");
    if (!guide) {
      throw new Error("the converted catalog has no wyrd_guide tool");
    }
    const output = await guide.invoke(new RunContext({}), JSON.stringify({ topic: "overview" }));
    expect(JSON.stringify(output)).toContain("overview");
  } finally {
    await mcp.close();
  }
});
