import { randomUUID } from 'node:crypto';
import { mockChanges } from './changes/store';
import { mentions, newDraft, subjects, verifiers } from './changes/fixtures';
import type {
  ChangeView,
  Draft,
  ReviewInput,
  ReviseInput
} from '$lib/features/changes/types';
import type { ChangeList, ChangeSummary } from '$lib/features/changes/types';
import { mockHome } from './mock';
import type { HomeView } from '$lib/views';
import { reject, type TenantContext } from './auth/session';

/** The single server-only domain seam. Mock projections are not durable domain records. */
export class WyrdClient {
  constructor(
    private readonly context: TenantContext,
    private readonly mockData: boolean
  ) {}

  changes(filters: Record<string, string>): ChangeList {
    if (!this.context.permissions.includes('changes:read')) reject('denied');
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
    if (!this.context.permissions.includes(write ? 'changes:write' : 'changes:read'))
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
        write: canAct && this.context.permissions.includes('changes:write'),
        review: canAct && this.context.permissions.includes('changes:review'),
        run: canAct && this.context.permissions.includes('changes:run'),
        override: canAct && this.context.permissions.includes('changes:override')
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
    if (!this.context.permissions.includes('changes:run')) reject('denied');
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
    if (!this.context.permissions.includes(permission)) reject('denied');
    return mockChanges.review(
      this.context.tenant.tenantId,
      this.context.tenant.key,
      this.context.subject.id,
      id,
      input
    );
  }

  home(): HomeView {
    if (
      ['cards:read', 'bifrost_query:read', 'evals:read'].some(
        (permission) => !this.context.permissions.includes(permission)
      )
    )
      reject('denied');
    // No fallback to fixtures: the live transport must be connected here before server mode can serve data.
    if (!this.mockData) reject('upstream');
    return mockHome(this.context.tenant);
  }
}
