import { json } from '@sveltejs/kit';
import { AbsoluteRangeSchema, inputProblem, loadPanel, type Problem } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import type { RequestHandler } from './$types';

/**
 * Load one catalog panel as this tenant's browser session. The body is
 * `{component, input, range}`; the panel is validated against its catalog
 * schema and read through the SDK with only this session's cookie, so the
 * browser never holds a credential and cannot name another tenant.
 */
export const POST: RequestHandler = async ({ locals, request }) => {
  const session = locals.browserSession;
  if (!session) reject('unauthenticated');
  const body = (await request.json().catch(() => null)) as { component?: unknown; input?: unknown; range?: unknown } | null;
  if (typeof body?.component !== 'string')
    return refuse(inputProblem('Panel', { path: ['component'], message: 'must name a catalog component' }));
  const range = AbsoluteRangeSchema.safeParse(body.range);
  if (!range.success) {
    const [issue] = range.error.issues;
    return refuse(inputProblem('Panel', { path: ['range', ...issue.path], message: issue.message }));
  }
  const loaded = await loadPanel(body.component, body.input, {
    executor: nodeExecutor(session.client()),
    range: range.data,
    signal: request.signal
  });
  return 'code' in loaded ? refuse(loaded) : json(loaded);
};

/** Answer a refused panel with its stable problem. */
function refuse(problem: Problem): Response {
  return json(problem, { status: problem.status ?? 400, headers: { 'content-type': 'application/problem+json' } });
}
