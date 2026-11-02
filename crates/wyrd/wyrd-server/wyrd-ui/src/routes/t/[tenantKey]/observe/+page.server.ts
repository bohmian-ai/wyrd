import { fail, redirect } from '@sveltejs/kit';
import { SIGNALS, loadPanel, type Problem, type RecordDetailProps, type TraceWaterfallProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { DETAIL_PARAMS, closed, listSignal, readState, writeState } from '$lib/observe-host';
import type { Actions, PageServerLoad } from './$types';

/**
 * The open record as this tenant's browser session: the exact trace and its
 * selected span, or the exact log record. Only the record's own params are
 * read, so opening, switching or closing a record reloads nothing else. Each
 * input passes its catalog schema; a malformed id is refused with no read.
 */
export const load: PageServerLoad = async ({ locals, url, request }) => {
  const [trace, span, log] = DETAIL_PARAMS.map((name) => url.searchParams.get(name));
  if (!trace && !log) return {};
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  if (log) {
    let key: unknown = log;
    try {
      key = JSON.parse(log);
    } catch {
      // The catalog schema refuses the malformed key below.
    }
    return { record: (await loadPanel('RecordDetail', { log: key }, context)) as RecordDetailProps | Problem };
  }
  const [waterfall, record] = (await Promise.all([
    loadPanel('TraceWaterfall', { traceId: trace, ...(span ? { spanId: span } : {}) }, context),
    span ? loadPanel('RecordDetail', { traceId: trace, spanId: span }, context) : undefined
  ])) as [TraceWaterfallProps | Problem, RecordDetailProps | Problem | undefined];
  return { waterfall, record };
};

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
