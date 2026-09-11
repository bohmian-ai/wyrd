import type { CardDetail, CardRow, CardsView } from '$lib/features/cards/core/types';
import { CARD_KINDS } from '$lib/features/cards/core/types';
import { cardDetails, cardRows, recentCards } from './fixtures';

/** One row against every filter except kind — the kind rail counts use this. */
function matchesExceptKind(row: CardRow, filters: Record<string, string>, q: string): boolean {
  return (
    (!filters.space || row.space === filters.space) &&
    (!filters.status || row.status.label === filters.status) &&
    (!filters.label || row.labels.includes(filters.label)) &&
    (!filters.owner || (filters.owner === 'unowned' ? !row.owner : row.owner === filters.owner)) &&
    (!q || [row.name, row.kind, row.uid].join(' ').toLowerCase().includes(q))
  );
}

/**
 * C-01 — project the inventory under URL-backed lookup plus kind, label,
 * status, space and owner filters, sorted most-recently-updated first. Kind
 * counts respect every other active filter so the rail numbers always agree
 * with what selecting a kind would show.
 */
export function projectCards(filters: Record<string, string>, populated = true): CardsView {
  const q = (filters.q ?? '').toLowerCase();
  const tenantRows = populated ? cardRows : [];
  const scoped = tenantRows.filter((row) => matchesExceptKind(row, filters, q));
  const rows = scoped
    .filter((row) => !filters.kind || row.kind === filters.kind)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return structuredClone({
    total: tenantRows.length,
    kinds: CARD_KINDS.map((kind) => ({
      kind,
      count: scoped.filter((row) => row.kind === kind).length
    })),
    rows,
    recent: populated ? recentCards : []
  });
}

/**
 * C-02 — project one Card detail at an exact version. A fixture without a
 * full detail gets a generated generic envelope, which is exactly the typed
 * Spec fallback path later kind tasks replace. Selecting a prior version
 * renders that version's declared Spec when the fixture carries it; a prior
 * version without one renders an absent Spec state instead of silently
 * showing the current declaration, and earned presentations never render for
 * prior versions.
 *
 * Returns `null` for an unknown uid or a version outside the version table.
 */
export function projectCard(uid: string, version?: string): CardDetail | null {
  const row = cardRows.find((card) => card.uid === uid);
  if (!row) return null;
  const fixture = cardDetails[uid];
  const detail: CardDetail = fixture ?? {
    uid,
    name: row.name,
    kind: row.kind,
    summary: `Registered ${row.kind} Card in space ${row.space}.`,
    status: row.status,
    version: row.version,
    currentVersion: row.version,
    versions: [
      { version: row.version, created: '2026-08-01 00:00Z', by: row.owner || '—', note: 'registration' }
    ],
    actions: [],
    spec: [
      {
        title: 'Spec',
        note: 'kind-owned sections',
        entries: [{ k: 'space', v: row.space }]
      }
    ],
    metadata: [
      { k: 'owner', v: row.owner || '— unowned' },
      { k: 'labels', v: row.labels.join(' · ') || 'none' },
      { k: 'updated', v: row.updated }
    ],
    relationships: []
  };
  const rendered = structuredClone(detail) as CardDetail;
  delete (rendered as { versionSpecs?: unknown }).versionSpecs;
  delete (rendered as { versionPresentations?: unknown }).versionPresentations;
  if (version && version !== rendered.currentVersion) {
    if (!rendered.versions.some((row) => row.version === version)) return null;
    rendered.version = version;
    // A version-scoped presentation (the Service workspace's historical
    // projection) replaces the current one; without it the earned
    // presentation never renders for a prior version.
    const versionPresentation = fixture?.versionPresentations?.[version];
    if (versionPresentation) rendered.presentation = structuredClone(versionPresentation);
    else delete rendered.presentation;
    const versionSpec = fixture?.versionSpecs?.[version];
    rendered.spec = versionSpec
      ? structuredClone(versionSpec)
      : [
          {
            title: 'Spec',
            entries: [],
            absent: `The ${version} declaration is not projected in this environment.`
          }
        ];
  }
  return rendered;
}
