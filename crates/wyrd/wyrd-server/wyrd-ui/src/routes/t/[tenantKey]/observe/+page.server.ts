import { fail, redirect } from '@sveltejs/kit';
import { SIGNALS } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { closed, listSignal, readState, writeState } from '$lib/observe-host';
import type { Actions } from './$types';

export const actions: Actions = {
  /**
   * Apply a WHERE draft. The server validates it against the signal's own
   * table as this session; only a valid draft reaches the URL, in its
   * normalized form, so an invalid draft never replaces the applied view.
   */
  where: async ({ locals, request, url }) => {
    const session = locals.browserSession ?? reject('unauthenticated');
    const form = await request.formData();
    const draft = String(form.get('where_sql') ?? '');
    const state = closed(readState(url.searchParams));
    if (!draft.trim()) redirect(303, `${url.pathname}?${writeState({ ...state, where_sql: '' })}`);
    const table = SIGNALS[listSignal(state)].table;
    const at = table.lastIndexOf('.');
    const [result] = await nodeExecutor(session.client()).run(
      [{ op: 'bifrost.validate_predicate', namespace: table.slice(0, at), name: table.slice(at + 1), expression: draft }],
      request.signal
    );
    if (!result.ok) return fail(result.problem.status ?? 400, { draft, problem: result.problem });
    redirect(303, `${url.pathname}?${writeState({ ...state, where_sql: result.value.expression })}`);
  }
};
