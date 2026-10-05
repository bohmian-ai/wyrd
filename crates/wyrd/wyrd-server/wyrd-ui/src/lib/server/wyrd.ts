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
import type { CardDetail, CardsView } from '$lib/features/cards/core/types';
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
    private readonly mockData: boolean
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

  cards(filters: Record<string, string>): CardsView {
    return projectCards(filters, this.cardsAccess());
  }

  card(uid: string, version?: string): CardDetail {
    const detail = this.cardsAccess() ? projectCard(uid, version) : null;
    if (!detail) reject('notFound');
    return detail;
  }

  home(): HomeView {
    if (
      ['cards:read', 'bifrost_query:read', 'evals:read'].some(
        (permission) => !this.can(permission)
      )
    )
      reject('denied');
    // No fallback to fixtures: the live transport must be connected here before server mode can serve data.
    if (!this.mockData) reject('upstream');
    return mockHome(this.context.tenant);
  }
}
