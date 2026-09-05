import { reject } from '$lib/server/auth/session';
import { reviewChange } from '$lib/server/changes/actions';
import type { PageServerLoad, Actions } from './$types';
export const load: PageServerLoad = ({ locals, params, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const view = locals.wyrd.change(
    params.id,
    url.searchParams.get('revision') ?? undefined
  );
  const subject = view.change.subjects.find((subject) => subject.id === params.subjectId);
  if (!subject) reject('notFound');
  const selected = url.searchParams.get('file') ?? subject.files[0]?.path;
  const file = subject.files.find((file) => file.path === selected);
  if (selected && !file) reject('notFound');
  return { ...view, subject, file: file ?? null };
};
export const actions: Actions = { review: reviewChange };
