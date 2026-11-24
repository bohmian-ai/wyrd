import { reject } from '$lib/server/auth/session';
import { relayAnswer } from '$lib/server/problem';
import type { RequestHandler } from './$types';

/**
 * Cancel this execution as the tenant's browser session through the existing
 * cancel operation, and relay its answer: the status after the cancel, which
 * a terminal execution keeps, or the server's stable problem.
 */
export const POST: RequestHandler = async ({ locals, params }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  return relayAnswer(await session.api('POST', `/verification/executions/${encodeURIComponent(params.executionId)}/cancel`), 'Cancel failed');
};
