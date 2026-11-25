import { loadPanel, type Problem, type RepositoryIndexProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { readIndex } from '$lib/repositories-host';
import type { PageServerLoad } from './$types';

/**
 * The Repositories index as this tenant's browser session: Repository Cards
 * joined to the GitHub connection's reachable repositories. The URL's search
 * and Show choice pass the catalog schema first; an invalid one issues no read.
 * `search` is the state every opened row carries so the way back restores it.
 */
export const load: PageServerLoad = async ({ locals, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  return { search: url.search, index: (await loadPanel('RepositoryIndex', readIndex(url.searchParams), context)) as RepositoryIndexProps | Problem };
};
