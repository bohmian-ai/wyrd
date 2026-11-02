import { fail } from '@sveltejs/kit';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { wyrdProblem, type QueryParam } from 'wyrd-ai-ui/loaders';
import { reject } from '$lib/server/auth/session';
import type { Actions } from './$types';

/** The rerun's bounds: the list ceiling and a deadline, so an inspection never reads unbounded. */
const MAX_ROWS = 500;
const DEADLINE_MS = 30_000;

export const actions: Actions = {
  /**
   * Re-run the exact SQL with its typed binds as this session, bounded. The
   * server authorizes every table it names; cancelling the submission aborts
   * the stream.
   */
  run: async ({ locals, request }) => {
    const session = locals.browserSession ?? reject('unauthenticated');
    const form = await request.formData();
    const sql = String(form.get('sql') ?? '');
    let params: QueryParam[];
    try {
      params = JSON.parse(String(form.get('params') ?? '[]')) as QueryParam[];
    } catch {
      return fail(400, { problem: wyrdProblem('validation', 'params must be a JSON array') });
    }
    if (!sql.trim() || !Array.isArray(params))
      return fail(400, { problem: wyrdProblem('validation', 'sql and a params array are required') });
    const [result] = await nodeExecutor(session.client()).run(
      [{ op: 'bifrost.query', sql, params, max_rows: MAX_ROWS, deadline_ms: DEADLINE_MS }],
      request.signal
    );
    return result.ok ? { result: result.value } : fail(result.problem.status ?? 400, { problem: result.problem });
  }
};
