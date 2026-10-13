import { randomUUID } from 'node:crypto';
import { mockChanges } from './mock/changes/store';
import { mentions, newDraft, subjects, verifiers } from './mock/changes/fixtures';
import type {
  ChangeView,
  Draft,
  ReviewInput,
  ReviseInput
} from '$lib/features/changes/types';
import type { ChangeList, ChangeSummary } from '$lib/features/changes/types';
import { mockHome } from './mock/home';
import type { HomeView } from '$lib/views';
import { reject, type TenantContext } from './auth/session';
import type { BrowserSession } from './auth/browser-sessions';
import { problemKind } from './problem';
import type { components, operations } from './generated/wyrd-http';
import type {
  DashboardDetail,
  DashboardsView,
  DriftView,
  EvalDetail,
  EvalsView,
  LogsView,
  MetricsView,
  ObserveOverview,
  TraceDetail,
  TracesView,
  GenAiView
} from '$lib/features/observe/core/types';
import { CARD_KINDS, type CardDetail, type CardKind, type CardsView } from '$lib/features/cards/core/types';
import { projectCard, projectCards } from './mock/cards/project';
import {
  projectDashboard,
  projectDashboards,
  projectDrift,
  projectEval,
  projectEvals,
  projectLogs,
  projectMetrics,
  projectOverview,
  projectGenAi,
  projectTrace,
  projectTraces
} from './mock/observe/project';

/** The single server-only domain seam. Mock projections are not durable domain records. */
export class WyrdClient {
  constructor(
    private readonly context: TenantContext,
    private readonly mockData: boolean,
    /** The signed-in principal's server session; live reads go through it. */
    private readonly session?: BrowserSession
  ) {}

  /** Whether the server-issued `resource:action` grants allow `permission`, honoring `wildcard`. */
  private can(permission: string): boolean {
    const [resource, action] = permission.split(':');
    return [resource, 'wildcard'].some((r) =>
      [action, 'wildcard'].some((a) => this.context.permissions.includes(`${r}:${a}`))
    );
  }

  changes(filters: Record<string, string>): ChangeList {
    if (!this.can('changes:read')) reject('denied');
    if (!this.mockData) reject('upstream');
    const records = [
      ...mockChanges
        .records(this.context.tenant.tenantId, this.context.tenant.key)
        .values()
    ];
    const matches = (record: ChangeSummary, view: string) =>
      view === 'needs-attention'
        ? record.attention.length > 0
        : view === 'verified'
          ? record.lifecycle === 'open' && record.verification.label === 'Verified'
          : record.lifecycle === view;
    return {
      counts: Object.fromEntries(
        ['open', 'needs-attention', 'verified', 'closed'].map((view) => [
          view,
          records.filter((record) => matches(record, view)).length
        ])
      ),
      records: records
        .filter(
          (record) =>
            matches(record, filters.view) &&
            [
              record.title,
              record.owner,
              record.service,
              ...record.repositories,
              ...record.prs.map((pr) => `#${pr}`)
            ]
              .join(' ')
              .toLowerCase()
              .includes((filters.q ?? '').toLowerCase()) &&
            ['owner', 'team', 'lifecycle'].every(
              (key) =>
                !filters[key] ||
                record[key as 'owner' | 'team' | 'lifecycle'] === filters[key]
            ) &&
            (!filters.repository || record.repositories.includes(filters.repository))
        )
        .map(
          ({
            id,
            title,
            owner,
            team,
            service,
            lifecycle,
            closure,
            verification,
            satisfied,
            total,
            repositories,
            prs,
            activity,
            attention
          }) =>
            structuredClone({
              id,
              title,
              owner,
              team,
              service,
              lifecycle,
              closure,
              verification,
              satisfied,
              total,
              repositories,
              prs,
              activity,
              attention
            })
        )
    };
  }

  private changeAccess(write = false): void {
    if (!this.can(write ? 'changes:write' : 'changes:read'))
      reject('denied');
    if (!this.mockData) reject('upstream');
  }
  change(id: string, revision?: string): ChangeView {
    this.changeAccess();
    const change = mockChanges.get(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      id,
      revision
    );
    const current = mockChanges.get(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      id
    );
    const canAct = change.revision === current.revision && change.lifecycle !== 'closed';
    return {
      change,
      verifiers: structuredClone(verifiers),
      capabilities: {
        write: canAct && this.can('changes:write'),
        review: canAct && this.can('changes:review'),
        run: canAct && this.can('changes:run'),
        override: canAct && this.can('changes:override')
      },
      mentions: structuredClone(mentions),
      requestKey: randomUUID()
    };
  }
  newChange() {
    this.changeAccess(true);
    return {
      draft: structuredClone(newDraft),
      verifiers: structuredClone(verifiers),
      requestKey: randomUUID()
    };
  }
  saveChange(draft: Draft, requestKey: string, id?: string, revision?: string): string {
    this.changeAccess(true);
    draft = structuredClone(draft);
    for (const claim of draft.claims)
      for (const check of claim.checks) {
        const configured = verifiers.find((verifier) => verifier.name === check.name);
        if (check.name && !configured) reject('validation');
        check.billable = configured?.billable ?? false;
      }
    return mockChanges.save(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      this.context.subject.id,
      draft,
      requestKey,
      id,
      revision
    );
  }
  resolveSubject(url: string) {
    this.changeAccess(true);
    const subject = subjects.find((subject) => subject.url === url);
    if (!subject) reject('validation');
    return structuredClone(subject);
  }

  runCheck(
    id: string,
    revision: string,
    checkId: string,
    requestKey: string,
    confirmed: boolean
  ): void {
    this.changeAccess();
    if (!this.can('changes:run')) reject('denied');
    mockChanges.run(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      this.context.subject.id,
      id,
      revision,
      checkId,
      requestKey,
      confirmed
    );
  }

  reviseChange(id: string, input: ReviseInput): string {
    this.changeAccess(true);
    for (const added of input.addClaims)
      if (added.verifier && !verifiers.some((verifier) => verifier.name === added.verifier))
        reject('validation');
    return mockChanges.revise(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      this.context.subject.id,
      id,
      input
    );
  }

  reviewChange(id: string, input: ReviewInput) {
    this.changeAccess();
    const permission =
      input.operation === 'override'
        ? 'changes:override'
        : input.operation === 'close'
          ? 'changes:write'
          : 'changes:review';
    if (!this.can(permission)) reject('denied');
    return mockChanges.review(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      this.context.subject.id,
      id,
      input
    );
  }

  /**
   * Authorize an Observe read. Telemetry signals and dashboards read through
   * Bifrost query; evaluation and drift results read through Vala evals.
   */
  /**
   * Authorize an Observe read and report whether this tenant has fixture
   * telemetry. Telemetry signals and dashboards read through Bifrost query;
   * evaluation and drift results read through Vala evals. Fixture data exists
   * for tenant `acme` only — other tenants see truthful empty results, never
   * another tenant's signals.
   */
  private observeAccess(domain: 'telemetry' | 'evals'): boolean {
    const permission = domain === 'telemetry' ? 'bifrost_query:read' : 'evals:read';
    if (!this.can(permission)) reject('denied');
    if (!this.mockData) reject('upstream');
    return this.context.tenant.key === 'acme';
  }

  /** Filters that can never match a fixture record, for tenants without data. */
  private static readonly noMatch = { q: ' ' };

  observe(scope: { service: string; range: string }): ObserveOverview {
    const populated = this.observeAccess('telemetry');
    return projectOverview(`/t/${encodeURIComponent(this.context.tenant.key)}`, scope, populated);
  }

  observeLogs(filters: Record<string, string>): LogsView {
    const populated = this.observeAccess('telemetry');
    if (!populated) filters = { ...filters, ...WyrdClient.noMatch };
    return projectLogs(filters, populated);
  }

  observeMetrics(filters: Record<string, string>): MetricsView {
    return projectMetrics(filters, this.observeAccess('telemetry'));
  }

  observeTraces(filters: Record<string, string>): TracesView {
    const populated = this.observeAccess('telemetry');
    if (!populated) filters = { ...filters, ...WyrdClient.noMatch };
    return projectTraces(filters, populated);
  }

  observeGenAi(filters: Record<string, string>): GenAiView {
    const populated = this.observeAccess('telemetry');
    if (!populated) filters = { ...filters, ...WyrdClient.noMatch };
    return projectGenAi(filters, populated);
  }

  observeTrace(id: string, span: string): TraceDetail {
    const detail = this.observeAccess('telemetry') ? projectTrace(id, span) : null;
    if (!detail) reject('notFound');
    return detail;
  }

  observeDashboards(filters: Record<string, string>): DashboardsView {
    if (!this.observeAccess('telemetry')) filters = { ...filters, ...WyrdClient.noMatch };
    return projectDashboards(filters);
  }

  observeDashboard(id: string, variables: Record<string, string>): DashboardDetail {
    const detail = this.observeAccess('telemetry') ? projectDashboard(id, variables) : null;
    if (!detail) reject('notFound');
    return detail;
  }

  observeEvals(filters: Record<string, string>): EvalsView {
    const populated = this.observeAccess('evals');
    if (!populated) filters = { ...filters, ...WyrdClient.noMatch };
    return projectEvals(filters, populated);
  }

  observeEval(recordId: string, task: string): EvalDetail {
    const detail = this.observeAccess('evals') ? projectEval(recordId, task) : null;
    if (!detail) reject('notFound');
    return detail;
  }

  observeDrift(filters: Record<string, string>): DriftView {
    const populated = this.observeAccess('evals');
    const view = projectDrift(filters);
    if (!populated) view.report = null;
    return view;
  }

  /**
   * Authorize a Card registry read and report whether this tenant has
   * fixture Cards. Fixture data exists for tenant `acme` only — other
   * tenants see truthful empty results, never another tenant's registry.
   */
  private cardsAccess(): boolean {
    if (!this.can('cards:read')) reject('denied');
    if (!this.mockData) reject('upstream');
    return this.context.tenant.key === 'acme';
  }

  /**
   * The Card inventory under URL filters. Live reads page the registry by its
   * opaque keyset `cursor`; `q` is a case-insensitive name match, or an exact
   * Card UID resolved through bounded kind-qualified reads.
   */
  async cards(filters: Record<string, string>, cursor = ''): Promise<CardsView> {
    if (this.mockData) return projectCards(filters, this.cardsAccess());
    if (!this.session) reject('upstream');
    const q = (filters.q ?? '').trim();
    const uid = cardUid.test(q) ? q.toLowerCase() : null;
    const query = {
      kind: filters.kind || undefined,
      space: filters.space || undefined,
      status: filters.status || undefined,
      filter:
        [
          !uid && q && `name =~ ${quoted(`(?i)${q.replace(/[\\^$.|?*+()[\]{}]/g, '\\$&')}`)}`,
          filters.label && labelClause(filters.label),
          filters.owner && labelClause(`owner=${filters.owner}`)
        ]
          .filter(Boolean)
          .join(' and ') || undefined
    };
    let page: ListCardsResponse;
    if (uid) {
      const found = await this.cardByUid(uid, filters.kind ? [filters.kind] : CARD_KINDS);
      // The UID's identity line under the same filters; the other versions drop out.
      page = found
        ? await this.listCards({
            ...query,
            kind: found.kind,
            name: found.metadata.name,
            include_prerelease: true,
            limit: 200
          })
        : { items: [], next_cursor: null };
      page = { items: page.items.filter((card) => card.card_uid === uid), next_cursor: null };
    } else {
      page = await this.listCards({ ...query, limit: cardsPageSize, cursor: cursor || undefined });
    }
    const scoped = Object.values(filters).some(Boolean) || Boolean(cursor);
    return liveCards(page, !scoped && !page.items.length);
  }

  /** One registry page; the server's refusal is the page's refusal. */
  private async listCards(
    query: NonNullable<operations['list_cards_http']['parameters']['query']>
  ): Promise<ListCardsResponse> {
    const response = await this.session!.api('get', '/cards', { query });
    if (!response.ok) reject(await problemKind(response));
    return response.json();
  }

  /**
   * Resolve a bare UID to its envelope by trying each candidate kind in
   * parallel; a kind that does not own the UID answers 404.
   */
  private async cardByUid(uid: string, kinds: readonly string[]) {
    const found = await Promise.all(
      kinds.map(async (kind) => {
        const response = await this.session!.api('get', '/cards/by-uid/{kind}/{card_uid}', {
          path: { kind, card_uid: uid }
        });
        if (response.status === 404) return null;
        if (!response.ok) reject(await problemKind(response));
        return (await response.json()).card;
      })
    );
    return found.find(Boolean) ?? null;
  }

  card(uid: string, version?: string): CardDetail {
    const detail = this.cardsAccess() ? projectCard(uid, version) : null;
    if (!detail) reject('notFound');
    return detail;
  }

  async home(): Promise<HomeView> {
    if (this.mockData) {
      if (
        ['cards:read', 'bifrost_query:read', 'evals:read'].some(
          (permission) => !this.can(permission)
        )
      )
        reject('denied');
      return mockHome(this.context.tenant);
    }
    // No fallback to fixtures: without a server session there is no live data.
    if (!this.session) reject('upstream');
    // The server decides authority; its refusal is the page's refusal.
    const response = await this.session.api('get', '/cards');
    if (!response.ok) reject(await problemKind(response));
    return liveHome(`/t/${encodeURIComponent(this.context.tenant.key)}`, await response.json());
  }
}

type Lifecycle = components['schemas']['CardLifecycleStatus'];
type ListCardsResponse = components['schemas']['ListCardsResponse'];

/** Rows per inventory page; the registry caps a page at 200. */
const cardsPageSize = 20;
const cardUid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A metadata-query string literal. */
const quoted = (value: string) => `"${value.replace(/[\\"]/g, '\\$&')}"`;

/** `key=value` selects a label value; a bare `key` selects Cards declaring it. */
function labelClause(label: string): string {
  const at = label.indexOf('=');
  return at < 0
    ? `labels.${quoted(label)} exists`
    : `labels.${quoted(label.slice(0, at))} = ${quoted(label.slice(at + 1))}`;
}

/**
 * Project one registry page onto the inventory. The wire has no population
 * counts, so `total` is known only for an unscoped empty tenant.
 */
function liveCards(page: ListCardsResponse, emptyTenant: boolean): CardsView {
  return {
    total: emptyTenant ? 0 : null,
    kinds: CARD_KINDS.map((kind) => ({ kind, count: null })),
    rows: page.items.map((card) => ({
      uid: card.card_uid,
      name: card.name,
      kind: card.kind as CardKind,
      version: card.version,
      space: card.space,
      status: { label: card.status, tone: lifecycleTone[card.status] },
      updated: `${card.updated_at.slice(0, 16).replace('T', ' ')}Z`,
      updatedAt: card.updated_at
    })),
    recent: [],
    order: 'oldest registered first',
    next: page.next_cursor ?? null
  };
}
const lifecycleTone: Record<Lifecycle, HomeView['cards'][number]['tone']> = {
  active: 'ok',
  pending: 'running',
  deprecated: 'warn',
  expired: 'warn',
  failed: 'danger',
  deleted: 'neutral'
};

/**
 * Project the server's first Card page onto Home. Operational sections have no
 * live source until their pages land, so they are `null` (unavailable), not empty.
 */
function liveHome(base: string, page: components['schemas']['ListCardsResponse']): HomeView {
  // ponytail: one default-size page, so "newest" is within it; page the cursor when Home needs exact totals.
  const more = Boolean(page.next_cursor);
  const kinds = [...new Set(page.items.map((card) => card.kind))].sort();
  return {
    attention: null,
    changes: null,
    recent: null,
    cards: page.items
      .toSorted((a, b) => b.created_at.localeCompare(a.created_at))
      .slice(0, 5)
      .map((card) => ({
        name: card.name,
        kind: card.kind,
        version: card.version,
        space: card.space,
        status: card.status,
        tone: lifecycleTone[card.status],
        registered: card.created_at.slice(0, 10),
        href: `${base}/cards/${encodeURIComponent(card.card_uid)}`
      })),
    summaries: [
      {
        label: 'Cards',
        value: page.items.length,
        more,
        unit: 'registered',
        detail: more
          ? 'More are registered than one page shows.'
          : kinds.length
            ? kinds.join(' · ')
            : 'No Cards are registered in this tenant.',
        href: `${base}/cards`
      },
      {
        label: 'Observe',
        value: null,
        unit: '',
        detail: 'Signal summaries are not available on Home yet.',
        href: `${base}/observe`
      }
    ]
  };
}
