/**
 * The Node executor: runs each read through the `wyrd-ai` SDK as the client's
 * principal or forwarded browser session. Server-only; the client holds the credential.
 */
import { Bifrost, Cards, WyrdError, verify, type WyrdClient } from 'wyrd-ai';
import { wyrdProblem, type Executor, type Problem, type WireRequest, type WireResult } from './contract.js';

/** A `/v1` response that refused the read, carrying its problem. */
class Refused extends Error {
  constructor(readonly problem: Problem) {
    super(problem.code);
  }
}

/** Project an SDK failure or a refused response onto a stable problem; anything else is rethrown. */
function problem(error: unknown): Problem {
  if (error instanceof Refused) return error.problem;
  if (error instanceof WyrdError) return { code: error.code, title: error.title, status: error.status, detail: error.detail };
  throw error;
}

/** Share one in-flight or successful connection; a failed one is dropped so the next read connects again. */
function retained<T>(connect: () => Promise<T>): () => Promise<T> {
  let handle: Promise<T> | undefined;
  return () =>
    (handle ??= connect().catch((error) => {
      handle = undefined;
      throw error;
    }));
}

/** The stable problem a refused `/v1` response carries, or an upstream failure when it carries none. */
async function refusal(response: Response): Promise<Problem> {
  const body = (await response.json().catch(() => null)) as Partial<Problem> | null;
  if (typeof body?.code === 'string') return { code: body.code, title: body.title ?? response.statusText, status: response.status, detail: body.detail };
  return wyrdProblem('upstream', `The server answered ${response.status}`);
}

/**
 * Build an executor over one SDK client.
 *
 * `get` reads one `/v1` path as the same principal, for the reads the SDK has
 * no method for. A host whose client forwards a browser session passes its
 * session's own GET; absent, the client's authenticated fetch is used.
 */
export function nodeExecutor(client: WyrdClient, get: (path: string) => Promise<Response> = (path) => client.httpAuth(`${client.serverUrl}/v1${path}`)): Executor {
  const cards = retained(() => Cards.connect({ client }));
  const bifrost = retained(() => Bifrost.connect({ client }));
  const execution = (id: string) => verify.Execution.connect(id, { client });

  async function read(request: WireRequest, signal?: AbortSignal): Promise<unknown> {
    switch (request.op) {
      case 'cards.get_by_ref':
        return (await cards()).get(request.card_ref);
      case 'cards.list': {
        const { op: _, ...query } = request;
        return (await cards()).list(query);
      }
      case 'cards.list_artifacts':
        return (await cards()).listArtifacts(request.card_ref);
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
      case 'bifrost.describe_table': {
        // The SDK widens the version to a bigint; the wire keeps the JSON-safe decimal string.
        const { table_version, ...table } = await (await bifrost()).describeTable(request.namespace, request.name);
        return { ...table, table_version: String(table_version) };
      }
      case 'bifrost.validate_predicate':
        return { expression: await (await bifrost()).validatePredicate(request.namespace, request.name, request.expression) };
      case 'verification.get_execution':
        return (await execution(request.execution_id)).status();
      case 'cards.get_by_uid': {
        const response = await get(`/cards/by-uid/${request.kind}/${encodeURIComponent(request.card_uid)}`);
        if (!response.ok) throw new Refused(await refusal(response));
        return ((await response.json()) as { card: unknown }).card;
      }
      case 'verification.get_execution_details':
        return (await execution(request.execution_id)).details();
      case 'verification.list_invocations':
        return (await execution(request.execution_id)).invocations({ cursor: request.cursor, limit: request.limit });
      case 'verification.list_publications':
        return (await execution(request.execution_id)).publications({ cursor: request.cursor, limit: request.limit });
      case 'verification.get_publication':
        return (await execution(request.execution_id)).publication(request.step_id, request.name, {
          attempt: request.attempt,
          item_index: request.item_index,
          iteration_index: request.iteration_index,
          path: request.path
        });
      case 'verification.read_publication_text': {
        const { op: _, execution_id, step_id, name, max_bytes, ...selector } = request;
        const { plan } = await (await execution(execution_id)).download(step_id, name, selector);
        // A server-relative plan is read as this principal; a storage URL carries its own grant.
        const response = plan.get_url.startsWith('/v1/') ? await get(plan.get_url.slice('/v1'.length)) : await fetch(plan.get_url, { signal });
        if (!response.ok) throw new Refused(await refusal(response));
        // ponytail: the whole body is buffered; loaders ask only for files within the preview bound. Stream and stop at max_bytes if that changes.
        const bytes = new Uint8Array(await response.arrayBuffer());
        const truncated = bytes.length > max_bytes;
        try {
          const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, max_bytes), { stream: truncated });
          return { text: text.includes('\0') ? null : text, truncated };
        } catch {
          return { text: null, truncated };
        }
      }
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
