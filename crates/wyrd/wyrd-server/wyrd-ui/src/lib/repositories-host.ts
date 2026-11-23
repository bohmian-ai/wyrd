/**
 * The Repositories pages' URL state: the index search and Show choice, one
 * exact Repository version with its declaration tab, one pull request with
 * its pinned review set, and the index state a detail page returns to. Every field round-trips through the URL.
 */
import type { CardRef } from 'wyrd-ai-ui';
import type { RepositoryIndexInput } from 'wyrd-ai-ui/loaders';
import { readState, writeState, type HostFilter } from './observe-host';

/** The index input a URL names; the catalog schema validates it before any read. */
export function readIndex(params: URLSearchParams): Record<string, unknown> {
  return Object.fromEntries(Object.entries({ query: params.get('q') ?? undefined, show: params.get('show') ?? undefined }).filter(([, value]) => value !== undefined));
}

/** The index URL search for `input`. */
export function writeIndex(input: RepositoryIndexInput): string {
  const params = new URLSearchParams();
  if (input.query) params.set('q', input.query);
  if (input.show && input.show !== 'all') params.set('show', input.show);
  return params.size ? `?${params}` : '';
}

/** The index search a page was opened from: only a search string returns, never another path. */
export function readBack(params: URLSearchParams): string {
  const back = params.get('back');
  return back?.startsWith('?') ? back : '';
}

const withBack = (params: URLSearchParams, back?: string) => {
  if (back) params.set('back', back);
  return params.size ? `?${params}` : '';
};

/** The page of one exact Repository Card version. */
export function repositoryHref(base: string, ref: CardRef, extra: { declaration?: string; back?: string } = {}): string {
  const params = new URLSearchParams({ version: ref.version });
  if (extra.declaration && extra.declaration !== 'yaml') params.set('declaration', extra.declaration);
  return `${base}/repositories/${encodeURIComponent(ref.name)}${withBack(params, extra.back)}`;
}

/** One pull request's review sets: the plain link follows the latest set, and `generation` pins one exact set. */
export function pullHref(base: string, name: string, number: number, generation?: string): string {
  return `${base}/repositories/${encodeURIComponent(name)}/pulls/${number}${generation ? `?${new URLSearchParams({ set: generation })}` : ''}`;
}

/** The page of one reachable GitHub repository that has no Card. */
export function undeclaredHref(base: string, repositoryId: string, back?: string): string {
  return `${base}/repositories/github/${encodeURIComponent(repositoryId)}${withBack(new URLSearchParams(), back)}`;
}

/** The range a Repository's activity reads, and Verify opens with. */
export const ACTIVITY_LAST = '30d';
/** The executions of one Repository Card, every version. */
export const activityWhere = (name: string): HostFilter[] => [{ column: 'subject_kind', value: 'Repository' }, { column: 'subject_name', value: name }];

/** Verify, filtered to one Repository's executions over the same range. */
export function verifyHref(base: string, name: string): string {
  return `${base}/verify?${writeState({ ...readState(new URLSearchParams()), signal: 'traces', last: ACTIVITY_LAST, where: activityWhere(name) })}`;
}
