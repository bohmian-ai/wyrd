import { json } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { RequestHandler } from './$types';

/**
 * Cancel this execution as the tenant's browser session through the existing
 * cancel operation, and relay its answer: the status after the cancel, which
 * a terminal execution keeps, or the server's stable problem.
 */
export const POST: RequestHandler = async ({ locals, params }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const response = await session.api('POST', `/verification/executions/${encodeURIComponent(params.executionId)}/cancel`);
  const body = (await response.json().catch(() => null)) as { code?: unknown; title?: unknown; detail?: unknown; status?: unknown } | null;
  if (response.ok) return json(body);
  return json(
    { code: typeof body?.code === 'string' ? body.code : 'WYRD_SPEC_500_INTERNAL', title: typeof body?.title === 'string' ? body.title : 'Cancel failed', status: response.status, ...(typeof body?.detail === 'string' ? { detail: body.detail } : {}) },
    { status: response.status, headers: { 'content-type': 'application/problem+json' } }
  );
};
