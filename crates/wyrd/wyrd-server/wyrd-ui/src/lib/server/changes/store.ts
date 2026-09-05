import { randomUUID } from 'node:crypto';
import { reject } from '../auth/session';
import { mentions, fixtureChange, summaries, verifiers } from './fixtures';
import type { Change, Draft, ReviewInput, Anchor } from '$lib/features/changes/types';

/** Process-local development state. No persistence or durable domain contract. */
export class MockChanges {
  private readonly tenants = new Map<string, Map<string, Change>>();
  private readonly revisions = new Map<string, Change>();
  private readonly retries = new Map<string, { input: string; id: string }>();
  records(tenantId: string, key: string): Map<string, Change> {
    let records = this.tenants.get(tenantId);
    if (!records) {
      // ponytail: bounded process-local fixtures; replace with Wyrd transport for durable use.
      if (this.tenants.size >= 100) reject('upstream');
      records = new Map(
        (key === 'acme' ? summaries : []).map((summary) => [
          summary.id,
          fixtureChange(summary)
        ])
      );
      this.tenants.set(tenantId, records);
      for (const value of records.values()) {
        for (const number of [5, 6]) {
          const prior = structuredClone(value);
          prior.revisionNumber = number;
          prior.revision = `rev_0${number}`;
          prior.priorRevisions = [];
          prior.override = 'None';
          prior.approval = 'No reviews';
          prior.nextAction =
            'Historical revision — return to the current revision to act.';
          prior.verification = { label: 'Not run', tone: 'neutral' };
          prior.satisfied = 0;
          prior.created = number === 6 ? '2026-09-02T09:12:00Z' : '2026-09-01T09:12:00Z';
          prior.timeline = prior.timeline.filter((event) => event.at <= prior.created);
          prior.threads = [];
          prior.blockers = [];
          prior.subjects = prior.subjects.slice(0, 2).map((subject) => ({
            ...subject,
            candidate: subject.base,
            files: [],
            commits: []
          }));
          prior.claims.forEach((claim) => {
            claim.resolution = 'pending';
            claim.checks = [];
          });
          this.revisions.set(`${tenantId}:${prior.id}:${prior.revision}`, prior);
        }
      }
    }
    return records;
  }
  get(tenantId: string, key: string, id: string, revision?: string): Change {
    const value = this.records(tenantId, key).get(id);
    if (!value) reject('notFound');
    const selected =
      revision && revision !== value.revision
        ? this.revisions.get(`${tenantId}:${id}:${revision}`)
        : value;
    if (!selected) reject('notFound');
    return structuredClone(selected);
  }
  run(
    tenantId: string,
    key: string,
    actor: string,
    id: string,
    revision: string,
    checkId: string,
    requestKey: string,
    confirmed: boolean
  ): void {
    const token = `${tenantId}:${actor}:run:${requestKey}`;
    const input = JSON.stringify({ id, revision, checkId, confirmed });
    const prior = this.retries.get(token);
    if (prior) {
      if (prior.input !== input) reject('conflict');
      return;
    }
    if (!requestKey || requestKey.length > 100 || !confirmed) reject('validation');
    const change = this.records(tenantId, key).get(id);
    if (!change) reject('notFound');
    if (change.revision !== revision || change.lifecycle !== 'open') reject('conflict');
    const check = change.claims
      .flatMap((claim) => claim.checks)
      .find((check) => check.id === checkId);
    if (
      !check ||
      !check.eligible ||
      check.execution === 'queued' ||
      check.execution === 'running' ||
      check.evidence.some((item) => !item.present)
    )
      reject('conflict');
    check.history.push({
      execution: check.execution,
      verdict: check.verdict,
      revision: check.revision
    });
    check.execution = 'queued';
    check.verdict = null;
    check.summary = { label: 'Queued', tone: 'warn' };
    check.eligible = false;
    check.action = null;
    check.revision = revision;
    check.explanation = 'Queued after explicit run confirmation.';
    if (check.required) {
      const claim = change.claims.find((claim) =>
        claim.checks.some((item) => item.id === checkId)
      )!;
      if (claim.resolution === 'satisfied') change.satisfied -= 1;
      claim.resolution = 'pending';
      change.verification = { label: 'Verifying', tone: 'running' };
      change.blockers = change.blockers.filter(
        (blocker) => !blocker.text.startsWith(claim.id)
      );
      change.nextAction = `${check.name} is queued. Waiting for ranking test Evidence; no new verdict is available.`;
    }
    change.timeline.unshift({
      id: randomUUID(),
      kind: 'run',
      source: 'Audit',
      at: new Date().toISOString(),
      actor,
      text: `${check.name} queued`,
      destination: `/verification#check-${check.id}`
    });
    this.retries.set(token, { input, id });
  }

  review(
    tenantId: string,
    key: string,
    actor: string,
    id: string,
    input: ReviewInput
  ): { threadId: string; commentId: string } {
    const token = `${tenantId}:${actor}:review:${input.requestKey}`;
    const serialized = JSON.stringify({ id, input });
    const prior = this.retries.get(token);
    if (prior) {
      if (prior.input !== serialized) reject('conflict');
      const [threadId, commentId] = prior.id.split('/');
      return { threadId, commentId };
    }
    if (!input.requestKey || input.requestKey.length > 150 || input.body.length > 10000)
      reject('validation');
    if (this.retries.size >= 10000) reject('upstream');
    const current = this.records(tenantId, key).get(id);
    if (!current) reject('notFound');
    if (current.revision !== input.revision || current.lifecycle === 'closed')
      reject('conflict');
    const change = structuredClone(current);
    const at = new Date().toISOString();
    const author = mentions.find((person) => person.id === actor)?.name ?? actor;
    let thread = change.threads.find((thread) => thread.id === input.threadId);
    let commentId = input.commentId;
    if (['comment', 'reply', 'edit'].includes(input.operation) && !input.body.trim())
      reject('validation');
    if (input.operation === 'comment') {
      if (change.threads.length >= 500) reject('validation');
      const anchor = input.anchor ?? {
        kind: 'change',
        revision: change.revision,
        target: id
      };
      validateAnchor(change, anchor);
      thread = {
        id: randomUUID(),
        anchor,
        resolved: false,
        comments: [],
        transitions: []
      };
      change.threads.push(thread);
    }
    if (['comment', 'reply', 'edit'].includes(input.operation)) {
      if (!thread) reject('notFound');
      if (thread.comments.length >= 500) reject('validation');
      const previous =
        input.operation === 'edit'
          ? thread.comments.find((comment) => comment.id === input.commentId)
          : undefined;
      if (input.operation === 'edit' && (!previous || previous.author !== actor))
        reject('denied');
      if (previous && previous.revisions.at(-1)?.id !== input.expected)
        reject('conflict');
      const revision = {
        id: randomUUID(),
        body: input.body,
        editor: actor,
        at,
        predecessor: previous?.revisions.at(-1)?.id ?? null,
        mentions: mentions.filter((mention) =>
          [...input.body.matchAll(/@([a-zA-Z0-9_.-]+)/g)].some(
            (match) => match[1] === mention.name
          )
        )
      };
      if (previous) {
        if (previous.revisions.length >= 100) reject('validation');
        previous.revisions.push(revision);
      } else {
        if (
          input.commentId &&
          !thread.comments.some((comment) => comment.id === input.commentId)
        )
          reject('validation');
        commentId = randomUUID();
        thread.comments.push({
          id: commentId,
          author: actor,
          replyTo: input.commentId || null,
          revisions: [revision]
        });
      }
    } else if (input.operation === 'resolve' || input.operation === 'reopen') {
      if (!thread) reject('notFound');
      thread.resolved = input.operation === 'resolve';
      thread.transitions.push({ resolved: thread.resolved, actor: author, at });
    } else if (input.operation === 'decision') {
      if (!['comment', 'approve', 'request-changes'].includes(input.decision))
        reject('validation');
      if (input.decision === 'approve')
        change.approval = `Approved by ${author} · revision ${change.revisionNumber}`;
      if (input.decision === 'request-changes') {
        change.approval = `Changes requested by ${author}`;
        change.attention = [{ reason: 'Changes requested', actor: change.owner }];
      }
    } else if (input.operation === 'close') {
      if (!['completed', 'cancelled'].includes(input.decision)) reject('validation');
      change.lifecycle = 'closed';
      change.closure = input.decision as 'completed' | 'cancelled';
      change.attention = [];
      change.nextAction = `Closed — ${input.decision}. Provider pull requests remain unchanged.`;
    } else if (input.operation === 'override') {
      if (
        !change.claims.some((claim) => claim.id === input.decision) ||
        !input.body.trim()
      )
        reject('validation');
      change.override = `${input.decision} · authorized by ${author} · does not verify`;
    } else reject('validation');
    change.timeline.unshift({
      id: randomUUID(),
      kind:
        input.operation === 'close'
          ? 'lifecycle'
          : ['decision', 'override'].includes(input.operation)
            ? 'decision'
            : 'discussion',
      source: ['decision', 'override', 'close'].includes(input.operation)
        ? 'Audit'
        : 'Review activity',
      at,
      actor: author,
      text: `${input.operation}${input.decision ? ': ' + input.decision : ''}${input.body ? ' — ' + input.body : ''}`,
      destination:
        input.operation === 'close'
          ? '/timeline'
          : `/review${thread ? '#' + thread.id : '#submit-review'}`
    });
    this.records(tenantId, key).set(id, change);
    this.retries.set(token, {
      input: serialized,
      id: `${thread?.id ?? ''}/${commentId}`
    });
    return { threadId: thread?.id ?? '', commentId };
  }

  save(
    tenantId: string,
    key: string,
    actor: string,
    draft: Draft,
    requestKey: string,
    id?: string,
    revision?: string
  ): string {
    const records = this.records(tenantId, key);
    const token = `${tenantId}:${actor}:save:${requestKey}`;
    const input = JSON.stringify({ draft, id, revision });
    const prior = this.retries.get(token);
    if (prior) {
      if (prior.input !== input) reject('conflict');
      return prior.id;
    }
    if (!requestKey || requestKey.length > 100) reject('validation');
    const existing = id ? records.get(id) : undefined;
    if (id && !existing) reject('notFound');
    if (existing && (existing.lifecycle !== 'draft' || existing.revision !== revision))
      reject('conflict');
    if (records.size >= 1000 || this.retries.size >= 10000) reject('upstream');
    if (existing)
      this.revisions.set(
        `${tenantId}:${id}:${existing.revision}`,
        structuredClone(existing)
      );
    const target =
      existing ??
      fixtureChange({
        ...summaries[2],
        id: `change_${String(records.size + 1).padStart(2, '0')}`,
        title: draft.title
      });
    if (existing) target.priorRevisions = [existing.revision, ...existing.priorRevisions];
    else target.priorRevisions = [];
    target.draft = structuredClone(draft);
    target.title = draft.title || 'Untitled Change Request';
    target.intent = draft.intent;
    target.impact = draft.impact;
    target.owner = draft.owner;
    target.owners = draft.teams;
    target.team = draft.teams;
    target.service = '';
    const savedAt = new Date().toISOString();
    if (!existing)
      target.author = mentions.find((person) => person.id === actor)?.name ?? actor;
    target.created = savedAt;
    target.activity = 'Just now';
    target.attention = [
      { reason: 'Complete draft', actor: draft.owner || target.author }
    ];
    target.subjects = draft.subjects;
    target.repositories = [
      ...new Set(draft.subjects.map((subject) => subject.repository))
    ];
    target.prs = draft.subjects.map((subject) => subject.pr);
    target.revisionNumber = existing ? existing.revisionNumber + 1 : 1;
    target.revision = `rev_${String(target.revisionNumber).padStart(2, '0')}`;
    target.claims = draft.claims.map((claim) => ({
      id: claim.id,
      title: claim.title,
      resolution: 'pending',
      checks: claim.checks.map((check, index) => ({
        ...check,
        id: `${claim.id}-${index}`,
        verifier: check.name,
        version:
          verifiers.find((verifier) => verifier.name === check.name)?.version ?? '',
        execution: 'not_run',
        verdict: null,
        provenance: 'current',
        revision: target.revision,
        summary: { label: 'Not ready', tone: 'neutral' },
        explanation: 'Evidence has not arrived for this draft.',
        eligible: false,
        action: null,
        evidence: [
          {
            id: `${claim.id}-evidence-${index}`,
            name: 'TestRunEvidence',
            present: false,
            digest: '',
            detail: 'Awaiting an exact completed revision and Evidence.'
          }
        ],
        history: []
      }))
    }));
    target.total = draft.claims.length;
    target.satisfied = 0;
    target.approval = 'No reviews';
    target.override = 'None';

    target.nextAction = 'Complete the draft subjects, Claims and Verifier requirements.';
    target.blockers = [];
    target.verification = { label: 'Not ready', tone: 'neutral' };
    target.timeline.unshift({
      id: randomUUID(),
      kind: 'revision',
      source: 'Audit',
      at: savedAt,
      actor: target.author,
      text: `Draft revision ${target.revisionNumber} saved`,
      destination: `?revision=${target.revision}`
    });
    records.set(target.id, target);
    this.retries.set(token, { input, id: target.id });
    return target.id;
  }
}
export const mockChanges = new MockChanges();

function validateAnchor(change: Change, anchor: Anchor): void {
  if (!anchor || anchor.revision !== change.revision) reject('conflict');
  const checks = change.claims.flatMap((claim) => claim.checks);
  if (anchor.kind === 'change' && anchor.target === change.id) return;
  if (
    anchor.kind === 'claim' &&
    change.claims.some((claim) => claim.id === anchor.target)
  )
    return;
  if (
    anchor.kind === 'evidence' &&
    checks.some((check) =>
      check.evidence.some((evidence) => evidence.id === anchor.target)
    )
  )
    return;
  if (anchor.kind === 'result' && checks.some((check) => check.id === anchor.target))
    return;
  const subject = change.subjects.find((subject) => subject.id === anchor.target);
  if (anchor.kind === 'subject' && subject) return;
  if (
    anchor.kind === 'source' &&
    subject?.files.some(
      (file) =>
        file.path === anchor.file &&
        file.lines.some(
          (line) => line.number === anchor.line && line.side === anchor.side
        )
    )
  )
    return;
  reject('validation');
}
