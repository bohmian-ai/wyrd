import { loadPanel, type Problem, type RepositoryDeclareProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { readBack } from '$lib/repositories-host';
import type { PageServerLoad } from './$types';

/**
 * One reachable GitHub repository without a Repository Card, as this
 * tenant's browser session. An id the tenant's connection does not reach is
 * not found and names no repository.
 */
export const load: PageServerLoad = async ({ locals, params, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  return { back: readBack(url.searchParams), declare: (await loadPanel('RepositoryDeclare', { repositoryId: params.repositoryId }, context)) as RepositoryDeclareProps | Problem };
};
