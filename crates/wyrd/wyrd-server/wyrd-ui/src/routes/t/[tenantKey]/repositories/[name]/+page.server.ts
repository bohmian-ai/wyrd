import { loadPanel, resolveRange, type Problem, type RecordTableProps, type RepositoryPolicyProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { ACTIVITY_LAST, activityWhere, readBack } from '$lib/repositories-host';
import type { PageServerLoad } from './$types';

/** How many of the Repository's newest executions the page lists; Verify reads the rest. */
const ACTIVITY_ROWS = 25;

/**
 * One exact Repository Card version as this tenant's browser session. The
 * path names the Card and the URL its exact version; the catalog schema
 * refuses anything else before a read. The Card's read gates the page, so a
 * denied or missing Card issues no other read. Activity is one list read of
 * the shared executions population, on its own: its refusal leaves policy
 * and access readable, and no execution's detail is read here.
 */
export const load: PageServerLoad = async ({ locals, params, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const back = readBack(url.searchParams);
  const executor = nodeExecutor(session.client());
  const input = {
    cardRef: { kind: 'Repository', name: params.name, version: url.searchParams.get('version') ?? '' },
    ...(url.searchParams.has('declaration') ? { declaration: url.searchParams.get('declaration') } : {})
  };
  const policy = (await loadPanel('RepositoryPolicy', input, { executor, signal: request.signal })) as RepositoryPolicyProps | Problem;
  if ('code' in policy || policy.result.state !== 'populated') return { back, policy };
  const [activity, [verifiers]] = await Promise.all([
    loadPanel(
      'RecordTable',
      { signal: 'executions', where: activityWhere(params.name), columns: ['recorded_at', 'pull_request', 'verifier_card_uid', 'subject_version', 'status', 'verdict'], limit: ACTIVITY_ROWS },
      { executor, range: resolveRange({ last: ACTIVITY_LAST }), signal: request.signal }
    ) as Promise<RecordTableProps | Problem>,
    // ponytail: one page of 200 Verifier versions is named; past that a row shows its exact UID, as on Verify.
    executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], request.signal)
  ]);
  const labels = verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name}@${card.version}`])) : {};
  return { back, policy, activity, labels };
};
