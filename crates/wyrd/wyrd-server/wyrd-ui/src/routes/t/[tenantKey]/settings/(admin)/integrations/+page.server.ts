import type { CardTypes } from 'wyrd-ai';
import { isHttpError } from '@sveltejs/kit';
import { read } from '$lib/server/admin';
import { problem, safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

/** The tenant's integrations as this session reads them: the GitHub connection's state, or its safe problem. */
export const load: PageServerLoad = async ({ locals }) => {
  if (!locals.browserSession) return { github: null, problem: problem('upstream') };
  try {
    const github = await read<CardTypes.GithubConnectionView>(locals.browserSession, '/integrations/github');
    return { github: github.state, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { github: null, problem: safeProblem(cause) };
  }
};
