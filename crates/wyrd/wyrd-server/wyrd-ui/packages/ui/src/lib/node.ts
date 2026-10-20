/**
 * The Node executor: runs each read through the `wyrd-ai` SDK as the client's
 * principal or forwarded browser session. Server-only; the client holds the credential.
 */
import { Bifrost, Cards, WyrdError, verify, type WyrdClient } from 'wyrd-ai';
import type { Executor, Problem, WireRequest, WireResult } from './contract.js';

/** Project an SDK failure onto a stable problem; anything else is rethrown. */
function problem(error: unknown): Problem {
  if (error instanceof WyrdError) return { code: error.code, title: error.title, status: error.status, detail: error.detail };
  throw error;
}

/** Build an executor over one SDK client. */
export function nodeExecutor(client: WyrdClient): Executor {
  let cardsHandle: Promise<Cards> | undefined;
  let bifrostHandle: Promise<Bifrost> | undefined;
  const cards = () => (cardsHandle ??= Cards.connect({ client }));
  const bifrost = () => (bifrostHandle ??= Bifrost.connect({ client }));

  async function read(request: WireRequest, signal?: AbortSignal): Promise<unknown> {
    switch (request.op) {
      case 'cards.get_by_ref':
        return (await cards()).get(request.card_ref);
      case 'cards.list': {
        const { op: _, ...query } = request;
        return (await cards()).list(query);
      }
      case 'bifrost.query': {
        const stream = await (await bifrost()).stream(request.sql, request.params ?? [], { deadlineMs: request.deadline_ms });
        const abort = () => void stream.return();
        // Cancelled while the stream started: close it before reading anything.
        if (signal?.aborted) abort();
        signal?.throwIfAborted();
        signal?.addEventListener('abort', abort, { once: true });
        try {
          const result = await stream.collect({ maxRows: request.max_rows, maxBytes: request.max_bytes });
          // Rows that arrive after cancellation are never a result.
          signal?.throwIfAborted();
          return result;
        } finally {
          signal?.removeEventListener('abort', abort);
        }
      }
      case 'bifrost.list_tables':
        return { tables: (await (await bifrost()).listTables()).map(({ namespace, name, status }) => ({ namespace, name, status })) };
      case 'bifrost.describe_table':
        return (await bifrost()).describeTable(request.namespace, request.name);
      case 'verification.get_execution':
        return (await verify.Execution.connect(request.execution_id, { client })).status();
    }
  }

  return {
    async run(requests, signal) {
      return Promise.all(
        requests.map(async (request) => {
          signal?.throwIfAborted();
          try {
            return { ok: true, value: await read(request, signal) } as WireResult;
          } catch (error) {
            signal?.throwIfAborted();
            return { ok: false, problem: problem(error) } as WireResult;
          }
        })
      ) as never;
    }
  };
}
