import { json } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { RequestHandler } from './$types';

/** Relay the server's answer: its body on success, else its stable problem. */
async function relay(response: Response, title: string): Promise<Response> {
  const body = (await response.json().catch(() => null)) as { code?: unknown; title?: unknown; detail?: unknown } | null;
  if (response.ok) return json(body);
  return json(
    { code: typeof body?.code === 'string' ? body.code : 'WYRD_SPEC_500_INTERNAL', title: typeof body?.title === 'string' ? body.title : title, status: response.status, ...(typeof body?.detail === 'string' ? { detail: body.detail } : {}) },
    { status: response.status, headers: { 'content-type': 'application/problem+json' } }
  );
}

/**
 * Begin connecting this tenant to GitHub as the browser session, through the
 * existing connect operation, and answer the GitHub URL the browser continues
 * at. GitHub returns to the server's own setup and callback; this host holds
 * no App secret and completes nothing.
 */
export const POST: RequestHandler = async ({ locals }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  return relay(await session.api('POST', '/integrations/github/connect', {}), 'Connect failed');
};

/** Disconnect this tenant from GitHub through the existing operation and relay the connection as it now reads. */
export const DELETE: RequestHandler = async ({ locals }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  return relay(await session.api('DELETE', '/integrations/github'), 'Disconnect failed');
};
