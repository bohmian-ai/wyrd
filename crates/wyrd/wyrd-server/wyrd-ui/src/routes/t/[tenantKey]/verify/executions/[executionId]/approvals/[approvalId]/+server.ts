import { json } from '@sveltejs/kit';
import { z } from 'zod';
import { reject } from '$lib/server/auth/session';
import { problem, relayAnswer } from '$lib/server/problem';
import type { RequestHandler } from './$types';

/** The decision body the existing operation takes; nothing else is forwarded. */
const Decision = z.object({ decision: z.enum(['approved', 'rejected']), reason: z.string().min(1).max(4096).optional() }).strict();

/**
 * Decide one exact approval occurrence of this execution as the tenant's
 * browser session through the existing decision operation, and relay its
 * answer: the durable receipt, or the server's stable problem. The server
 * checks the caller's current grants on every decision.
 */
export const POST: RequestHandler = async ({ locals, params, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const body = Decision.safeParse(await request.json().catch(() => null));
  if (!body.success) {
    const value = problem('validation');
    return json(value, { status: value.status, headers: { 'content-type': 'application/problem+json' } });
  }
  const path = `/verification/executions/${encodeURIComponent(params.executionId)}/approvals/${encodeURIComponent(params.approvalId)}`;
  return relayAnswer(await session.api('POST', path, body.data), 'Decision failed');
};
