/**
 * The MCP App executor: runs each read as the existing Wyrd MCP tool through
 * the host (`App.callServerTool`). The host holds the connection and its
 * credential; this module holds neither.
 */
import type { App } from '@modelcontextprotocol/ext-apps';
import { wyrdProblem, type Executor, type Problem, type WireRequest, type WireResult } from './contract.js';

/** The tool and arguments serving one read, and how its structured answer maps back. */
function tool(request: WireRequest): { name: string; arguments: Record<string, unknown>; value?: (content: Record<string, unknown>) => unknown } {
  const { op, ...rest } = request;
  switch (op) {
    case 'cards.get_by_ref':
      return { name: 'cards.get', arguments: rest, value: (content) => content.card };
    case 'verification.get_execution':
    case 'cards.list':
    case 'bifrost.query':
    case 'bifrost.list_tables':
    case 'bifrost.describe_table':
      return { name: op, arguments: JSON.parse(JSON.stringify(rest)) as Record<string, unknown> };
  }
}

/** A tool's structured refusal or a JSON-RPC error's problem `data`, as a stable problem; anything else is an upstream failure. */
function problem(source: unknown, fallback: string): Problem {
  const body = source as Partial<Problem> | undefined;
  if (typeof body?.code === 'string')
    return { code: body.code, title: body.title ?? fallback, status: body.status, detail: body.detail };
  return wyrdProblem('upstream', fallback);
}

/** Build an executor over a connected MCP App. */
export function mcpExecutor(app: Pick<App, 'callServerTool'>): Executor {
  async function read(request: WireRequest, signal?: AbortSignal): Promise<WireResult> {
    const call = tool(request);
    try {
      const result = await app.callServerTool({ name: call.name, arguments: call.arguments }, { signal });
      const content = (result.structuredContent ?? {}) as Record<string, unknown>;
      if (result.isError) return { ok: false, problem: problem(content, `${call.name} returned an error`) };
      return { ok: true, value: (call.value ? call.value(content) : content) as never };
    } catch (error) {
      signal?.throwIfAborted();
      return { ok: false, problem: problem((error as { data?: unknown }).data, String(error)) };
    }
  }

  return {
    async run(requests, signal) {
      signal?.throwIfAborted();
      return Promise.all(requests.map((request) => read(request, signal))) as never;
    }
  };
}
