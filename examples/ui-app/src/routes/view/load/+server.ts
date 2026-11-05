import { json, type RequestHandler } from '@sveltejs/kit';
import { AbsoluteRangeSchema, inputProblem, loadPanel, type Problem } from 'wyrd-ai-ui/loaders';
import { executor } from '../../../executor';

/** Load one `{component, input, range}` panel as this app's principal. */
export const POST: RequestHandler = async ({ request }) => {
  const body = (await request.json().catch(() => null)) as { component?: unknown; input?: unknown; range?: unknown } | null;
  const range = AbsoluteRangeSchema.safeParse(body?.range);
  if (typeof body?.component !== 'string' || !range.success)
    return refuse(inputProblem('Panel', { path: [], message: 'needs a catalog component and an absolute range' }));
  const loaded = await loadPanel(body.component, body.input, { executor: executor(), range: range.data, signal: request.signal });
  return 'code' in loaded ? refuse(loaded) : json(loaded);
};

function refuse(problem: Problem): Response {
  return json(problem, { status: problem.status ?? 400, headers: { 'content-type': 'application/problem+json' } });
}
