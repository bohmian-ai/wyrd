/**
 * The Cards pages' URL state: inventory filters and pages, and one exact
 * Card's reference, tab and comparison. Every field round-trips through the
 * URL, so reload and Back restore the same view.
 */
import type { CardInventoryInput, CardRef } from 'wyrd-ai-ui/loaders';

/** The inventory input a URL names; the catalog schema validates it before any read. */
export function readInventory(params: URLSearchParams): Record<string, unknown> {
  const pages = params.get('pages');
  return Object.fromEntries(
    Object.entries({
      query: params.get('q') ?? undefined,
      kind: params.get('kind') ?? undefined,
      status: params.get('status') ?? undefined,
      include_prerelease: params.get('prerelease') === '1' ? true : undefined,
      pages: pages === null ? undefined : Number(pages)
    }).filter(([, value]) => value !== undefined)
  );
}

/** The inventory URL search for `input`; filters without `pages` start a fresh cursor chain. */
export function writeInventory(input: CardInventoryInput): string {
  const params = new URLSearchParams();
  if (input.query) params.set('q', input.query);
  if (input.kind) params.set('kind', input.kind);
  if (input.status) params.set('status', input.status);
  if (input.include_prerelease) params.set('prerelease', '1');
  if (input.pages && input.pages > 1) params.set('pages', String(input.pages));
  const search = params.toString();
  return search ? `?${search}` : '';
}

/** Detail tabs every Card has, the Prompt's added Versions tab, and the Verifier's Graph in place of Overview. */
export const CARD_TABS = [
  { id: 'overview', label: 'Overview' },
  { id: 'files', label: 'Files' },
  { id: 'yaml', label: 'YAML' }
] as const;
export const PROMPT_TABS = [CARD_TABS[0], { id: 'versions', label: 'Versions' }, CARD_TABS[1], CARD_TABS[2]] as const;
export const VERIFIER_TABS = [{ id: 'overview', label: 'Graph' }, CARD_TABS[1], CARD_TABS[2]] as const;

/** The detail URL search for one exact Card, each value encoded on its own. */
export function writeDetail(ref: CardRef, extra: { tab?: string; compare?: string } = {}): string {
  const params = new URLSearchParams({ kind: ref.kind, name: ref.name, version: ref.version });
  if (ref.uid) params.set('uid', ref.uid);
  if (extra.tab && extra.tab !== 'overview') params.set('tab', extra.tab);
  if (extra.compare) params.set('compare', extra.compare);
  return `?${params}`;
}
