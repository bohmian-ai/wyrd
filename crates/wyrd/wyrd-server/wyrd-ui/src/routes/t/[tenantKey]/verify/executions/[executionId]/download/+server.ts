import { redirect } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { RequestHandler } from './$types';

/**
 * Download one retained publication file as the tenant's browser session.
 * The existing download operation plans it for the exact producer attempt;
 * a server-relative plan is streamed through this session, and a storage
 * URL is followed by the browser. Bytes always leave as an attachment the
 * browser must not sniff or render, whatever the file holds.
 */
export const GET: RequestHandler = async ({ locals, params, url }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const query = url.searchParams;
  const selector = new URLSearchParams();
  for (const [from, to] of [['attempt', 'attempt'], ['item', 'item_index'], ['iteration', 'iteration_index'], ['path', 'path']]) {
    const value = query.get(from);
    if (value !== null) selector.set(to, value);
  }
  const publication = `/verification/executions/${encodeURIComponent(params.executionId)}/publications/${encodeURIComponent(query.get('step') ?? '')}/${encodeURIComponent(query.get('name') ?? '')}`;
  const planned = await session.api('POST', `${publication}/download?${selector}`);
  if (!planned.ok) return relay(planned);
  const { plan } = (await planned.json()) as { plan: { get_url: string } };
  if (!plan.get_url.startsWith('/v1/')) redirect(303, plan.get_url);
  const content = await session.api('GET', plan.get_url.slice('/v1'.length));
  if (!content.ok) return relay(content);
  const file = (query.get('path') ?? query.get('name') ?? 'download').split('/').pop() as string;
  return new Response(content.body, {
    headers: {
      'content-type': 'application/octet-stream',
      'content-disposition': `attachment; filename*=UTF-8''${encodeURIComponent(file)}`,
      'x-content-type-options': 'nosniff',
      ...(content.headers.has('content-length') ? { 'content-length': content.headers.get('content-length') as string } : {})
    }
  });
};

/** Answer with the server's own problem and status. */
const relay = async (response: Response) =>
  new Response(await response.text(), { status: response.status, headers: { 'content-type': 'application/problem+json', 'x-content-type-options': 'nosniff' } });
