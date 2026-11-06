import { loadPanel, type CardInventoryProps, type Problem } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { readInventory } from '$lib/cards-host';
import type { PageServerLoad } from './$types';

/**
 * The registry inventory as this tenant's browser session. The URL's filters
 * pass the catalog schema first; an invalid one is refused with no read.
 */
export const load: PageServerLoad = async ({ locals, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const context = { executor: nodeExecutor(session.client()), signal: request.signal };
  return { inventory: (await loadPanel('CardInventory', readInventory(url.searchParams), context)) as CardInventoryProps | Problem };
};
