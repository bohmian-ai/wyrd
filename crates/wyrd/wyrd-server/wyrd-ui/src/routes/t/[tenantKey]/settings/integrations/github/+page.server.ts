import { loadPanel, type GithubIntegrationProps, type Problem } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import type { PageServerLoad } from './$types';

/**
 * The tenant's GitHub connection as this browser session: the server's
 * redacted view, then the Repository Cards its repositories are joined to.
 * A refused or unconfigured connection is that panel's state and reads no Card.
 */
export const load: PageServerLoad = async ({ locals, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  return { github: (await loadPanel('GithubIntegration', {}, context)) as GithubIntegrationProps | Problem };
};
