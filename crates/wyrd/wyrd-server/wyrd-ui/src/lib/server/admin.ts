/**
 * The calls every tenant Settings section makes as the signed-in principal.
 * The server authorizes each one; a refusal becomes its safe problem, and an
 * action's refusal is returned to the form with nothing assumed changed.
 */
import { fail, isHttpError, type RequestEvent } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import { retryAfter, safeProblem, upstreamProblem } from '$lib/server/problem';

/** Call `/v1` as the signed-in principal; a non-2xx answer becomes its safe problem. */
export async function call(session: BrowserSession, method: string, path: string, body?: unknown): Promise<Response> {
  const response = await session.api(method, path, body);
  if (!response.ok) reject(await upstreamProblem(response));
  return response;
}

/** Read one `/v1` JSON answer as the signed-in principal. */
export const read = async <T>(session: BrowserSession, path: string) => (await (await call(session, 'GET', path)).json()) as T;

/** A path segment, encoded. */
export const id = (value: string) => encodeURIComponent(value);

/** The signed-in Rust session; the development identity has none. */
export function signedIn(locals: App.Locals): BrowserSession {
  if (!locals.browserSession) reject('unauthenticated');
  return locals.browserSession;
}

/** Run one form action as the signed-in principal and render its refusal as the server's safe problem. */
export async function act<T>(event: RequestEvent, run: (form: FormData, session: BrowserSession) => Promise<T>) {
  try {
    const form = await event.request.formData();
    return await run(form, signedIn(event.locals));
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const value = safeProblem(cause);
    const wait = retryAfter(value);
    if (wait) event.setHeaders({ 'retry-after': String(wait) });
    return fail(value.status, { problem: value });
  }
}
