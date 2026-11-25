import { loadPanel, type Problem, type VerificationExecutionProps } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { readExecution } from '$lib/verify-host';
import type { PageServerLoad } from './$types';

/**
 * One exact execution as this tenant's browser session. The path names the
 * execution and the URL its selection; the catalog schema validates both
 * before any read. The load reads recorded state once and never waits for
 * the execution to progress.
 */
export const load: PageServerLoad = async ({ locals, params, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const loaded = await loadPanel('VerificationExecution', readExecution(params.executionId, url.searchParams), {
    executor: nodeExecutor(session.client(), (path) => session.api('GET', path)),
    signal: request.signal
  });
  return { executionId: params.executionId, ...('code' in loaded ? { problem: loaded as Problem } : { execution: loaded as VerificationExecutionProps }) };
};
