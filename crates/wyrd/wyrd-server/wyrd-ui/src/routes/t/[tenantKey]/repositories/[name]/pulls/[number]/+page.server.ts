import { loadPanel, type Problem, type ReviewSetsProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import type { PageServerLoad } from './$types';

/**
 * One pull request's review sets as this tenant's browser session. The path
 * names the Repository Card and the pull request, and `set` in the URL pins
 * one exact review set; the catalog schema validates them before any read.
 * The sets are read from retained records alone, so neither the Repository
 * Card nor the GitHub connection has to be readable. Verifier names are one
 * list read on their own: refused, each member shows its Verifier's UID.
 */
export const load: PageServerLoad = async ({ locals, params, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const executor = nodeExecutor(session.client());
  const set = url.searchParams.get('set');
  const pull = { name: params.name, number: Number(params.number) };
  const input = { repository: pull.name, number: pull.number, ...(set === null ? {} : { generation: set }) };
  const sets = (await loadPanel('ReviewSets', input, { executor, signal: request.signal })) as ReviewSetsProps | Problem;
  if ('code' in sets || sets.result.state !== 'populated' || !sets.result.data.selected) return { ...pull, sets, labels: {} };
  // ponytail: one page of 200 Verifier versions is named; past that a member shows its exact UID, as on Verify.
  const [verifiers] = await executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], request.signal);
  return { ...pull, sets, labels: verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name}@${card.version}`])) : {} };
};
