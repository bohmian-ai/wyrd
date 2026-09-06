import { fail, redirect, type RequestEvent } from '@sveltejs/kit';
import { sessions, reject } from '../auth/session';
import { safeProblem } from '../problem';
import type { Draft, Subject } from '$lib/features/changes/types';

export async function actionForm(event: RequestEvent): Promise<FormData> {
  if (!event.locals.session || !event.locals.wyrd) reject('unauthenticated');
  if (Number(event.request.headers.get('content-length') ?? 0) > 250000)
    reject('validation');
  const form = await event.request.formData();
  sessions.checkAction(event.locals.session, event.request, form.get('csrf'));
  return form;
}
function text(form: FormData, name: string, max = 10000): string {
  const value = form.get(name) ?? '';
  if (typeof value !== 'string' || value.length > max) reject('validation');
  return value;
}
export function parseDraft(form: FormData): Draft {
  let subjects: unknown, claims: unknown;
  try {
    subjects = JSON.parse(text(form, 'subjects', 100000) || '[]');
    claims = JSON.parse(text(form, 'claims', 100000) || '[]');
  } catch {
    reject('validation');
  }
  if (
    !Array.isArray(subjects) ||
    subjects.length > 30 ||
    !Array.isArray(claims) ||
    claims.length > 30
  )
    reject('validation');
  const string = (value: unknown, max = 1000) => {
    if (typeof value !== 'string' || value.length > max) reject('validation');
    return value;
  };
  const id = (value: unknown) => {
    const result = string(value, 100);
    if (!/^[A-Za-z0-9_-]+$/.test(result)) reject('validation');
    return result;
  };
  const commit = (value: unknown) => {
    const result = string(value, 64);
    if (result && !/^[a-fA-F0-9]{7,64}$/.test(result)) reject('validation');
    return result;
  };
  if (
    new Set(subjects.map((value) => value?.id)).size !== subjects.length ||
    new Set(claims.map((value) => value?.id)).size !== claims.length
  )
    reject('validation');
  return {
    title: text(form, 'title', 300),
    intent: text(form, 'intent'),
    impact: text(form, 'impact'),
    owner: text(form, 'owner', 200),
    teams: text(form, 'teams', 1000),
    subjects: subjects.map((value): Subject => {
      if (!value || typeof value !== 'object') reject('validation');
      const url = string(value.url);
      if (url) {
        try {
          const parsed = new URL(url);
          if (parsed.protocol !== 'https:' || parsed.username || parsed.password)
            reject('validation');
        } catch {
          reject('validation');
        }
      }
      return {
        id: id(value.id),
        repository: string(value.repository),
        provider: string(value.provider),
        pr: string(value.pr),
        base: commit(value.base),
        candidate: commit(value.candidate),
        url,
        relationship: '',
        commits: [],
        files: []
      };
    }),
    claims: claims.map((value) => {
      if (
        !value ||
        typeof value !== 'object' ||
        !Array.isArray(value.checks) ||
        value.checks.length > 20
      )
        reject('validation');
      return {
        id: id(value.id),
        title: string(value.title),
        checks: value.checks.map((check: Record<string, unknown>) => {
          if (
            !check ||
            !['manual', 'on-new-evidence'].includes(String(check.mode)) ||
            typeof check.required !== 'boolean' ||
            typeof check.billable !== 'boolean'
          )
            reject('validation');
          return {
            name: string(check.name),
            required: check.required,
            mode: check.mode as 'manual' | 'on-new-evidence',
            billable: check.billable
          };
        })
      };
    })
  };
}
export async function saveDraft(event: RequestEvent) {
  const form = await actionForm(event);
  let id: string;
  let draft: Draft | undefined;
  try {
    draft = parseDraft(form);
    id = event.locals.wyrd!.saveChange(
      draft,
      text(form, 'requestKey', 100),
      event.params.id,
      text(form, 'revision', 100)
    );
  } catch (cause) {
    const problem = safeProblem(cause);
    return fail(problem.status, { problem, draft });
  }
  redirect(303, `/t/${encodeURIComponent(event.params.tenantKey!)}/changes/${id}`);
}
export async function resolveDraft(event: RequestEvent) {
  const form = await actionForm(event);
  const draft = parseDraft(form);
  try {
    const subject = event.locals.wyrd!.resolveSubject(text(form, 'prUrl', 2000));
    const index = draft.subjects.findIndex((value) => value.url === subject.url);
    if (index < 0) draft.subjects.push(subject);
    else draft.subjects[index] = subject;
    return { draft };
  } catch (cause) {
    const problem = safeProblem(cause);
    return fail(problem.status, { problem, draft });
  }
}
export async function runCheck(event: RequestEvent) {
  const form = await actionForm(event);
  try {
    event.locals.wyrd!.runCheck(
      event.params.id!,
      text(form, 'revision'),
      text(form, 'checkId'),
      text(form, 'requestKey'),
      form.get('confirmed') === 'true'
    );
    return { message: 'Run queued. No verdict has been recorded.' };
  } catch (cause) {
    const problem = safeProblem(cause);
    return fail(problem.status, { problem });
  }
}
export async function reviseChange(event: RequestEvent) {
  const form = await actionForm(event);
  try {
    const removeClaimIds = form.getAll('remove').map((value) => {
      if (typeof value !== 'string' || value.length > 100 || !value) reject('validation');
      return value;
    });
    if (removeClaimIds.length > 30) reject('validation');
    const addTitle = text(form, 'addTitle', 300);
    event.locals.wyrd!.reviseChange(event.params.id!, {
      revision: text(form, 'revision', 100),
      requestKey: text(form, 'requestKey', 100),
      reason: text(form, 'reason'),
      addClaims: addTitle.trim()
        ? [{ title: addTitle, verifier: text(form, 'addVerifier', 200) }]
        : [],
      removeClaimIds
    });
  } catch (cause) {
    const problem = safeProblem(cause);
    return fail(problem.status, { problem });
  }
  redirect(
    303,
    `/t/${encodeURIComponent(event.params.tenantKey!)}/changes/${event.params.id}/verification`
  );
}
export async function reviewChange(event: RequestEvent) {
  const form = await actionForm(event);
  const body = text(form, 'body');
  const operations = [
    'comment',
    'reply',
    'edit',
    'resolve',
    'reopen',
    'decision',
    'close',
    'override'
  ] as const;
  const operation = text(form, 'operation');
  try {
    if (!operations.some((value) => value === operation)) reject('validation');
    let anchor;
    try {
      anchor = form.get('anchor') ? JSON.parse(text(form, 'anchor', 3000)) : undefined;
    } catch {
      reject('validation');
    }
    const result = event.locals.wyrd!.reviewChange(event.params.id!, {
      operation: operation as (typeof operations)[number],
      revision: text(form, 'revision'),
      requestKey: text(form, 'requestKey', 150),
      body,
      threadId: text(form, 'threadId'),
      commentId: text(form, 'commentId'),
      expected: text(form, 'expected'),
      anchor,
      decision: text(form, 'decision')
    });
    return { ...result, message: 'Recorded against the exact revision.' };
  } catch (cause) {
    const problem = safeProblem(cause);
    const current = event.locals.wyrd!.change(event.params.id!).change;
    const comment = current.threads
      .flatMap((thread) => thread.comments)
      .find((comment) => comment.id === form.get('commentId'));
    return fail(problem.status, {
      problem,
      body,
      currentRevision: current.revision,
      currentCommentRevision: comment?.revisions.at(-1)?.id,
      threadId: text(form, 'threadId'),
      commentId: text(form, 'commentId')
    });
  }
}
