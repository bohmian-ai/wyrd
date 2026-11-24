/**
 * Browser-side helpers for the onboarding pages: workspace creation, email
 * proofs, discovery and recovery contacts. These pages call the browser's
 * `/auth/*` leg from the page itself, because Rust scopes each flow cookie to
 * its `/auth/*` path and the BFF only forwards it; Rust judges every answer.
 * The wording here is the approved mock's; codes are the server's.
 */
import type { WyrdProblem } from '$lib/server/problem';

/** One `/auth/*` answer: its status and parsed JSON body, if any. */
export type AuthAnswer = { status: number; body: Record<string, unknown> | null };

/**
 * GET `path`, or POST `body` to it as JSON, on this origin. A network
 * failure answers 503 so the page shows the unavailable state.
 */
export async function auth(path: string, body?: unknown): Promise<AuthAnswer> {
  try {
    const response = await fetch(path, {
      method: body === undefined ? 'GET' : 'POST',
      headers: body === undefined ? {} : { 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    return { status: response.status, body: await response.json().catch(() => null) };
  } catch {
    return { status: 503, body: null };
  }
}

/** A problem in the mock's words for one server code. */
function worded(status: number, code: string, title: string, remediation: string): WyrdProblem {
  return { type: 'about:blank', status, code, title, remediation, detail: '', details: null };
}

/** Display names of the shared sign-in providers. */
export const PROVIDERS: Record<string, string> = { google: 'Google', github: 'GitHub' };

/** The refusals the onboarding pages explain in their own words. */
export const PROBLEMS = {
  failed: worded(401, '', 'Sign-in was not confirmed', 'The provider did not confirm who you are, so no workspace was created. Try again.'),
  taken: worded(409, 'WYRD_SPEC_409_CONFLICT', 'That URL is already taken', 'Choose a different URL slug and create the workspace again.'),
  limit: worded(403, 'WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT', 'Workspace limit reached', 'This account already owns the maximum number of workspaces. Sign in to an existing workspace, or contact the operator to raise the limit.'),
  challenge: worded(403, 'WYRD_AUTH_403_SIGNUP_CHALLENGE', 'Challenge not passed', 'Complete the challenge, then create the workspace again.'),
  disabled: worded(401, 'WYRD_AUTH_401_INVALID_TOKEN', 'Workspace signup is no longer available', 'The operator turned off signup, or this sign-in method, after you started. No workspace was created.'),
  expired: worded(400, 'WYRD_AUTH_400_INVALID_STATE', 'This signup expired', 'Start again to confirm who you are, then name your workspace.'),
  contact: worded(409, 'WYRD_SPEC_409_CONFLICT', 'Confirm an email address to continue', 'GitHub did not share a verified email address. Confirm one by email; you still sign in with GitHub and set no password.'),
  contactNoMail: worded(409, 'WYRD_SPEC_409_CONFLICT', 'GitHub did not share a verified email address', 'This deployment cannot send email, so this account cannot create a workspace. Verify an email address at GitHub, then start again.'),
  created: worded(409, 'WYRD_SPEC_409_CONFLICT', 'This signup already created a workspace', 'Sign in to the workspace it created from that workspace’s sign-in page.'),
  proofInvalid: worded(401, 'WYRD_AUTH_401_ONE_TIME_CODE_INVALID', 'This link was not accepted', 'Each link works once, and only in the browser that asked for it. Open the newest message in that browser, or start again.'),
  readExpired: worded(400, 'WYRD_AUTH_400_INVALID_STATE', 'This list expired', 'The list stays readable for five minutes. Request a new link to see it again.'),
  discoveryOff: worded(403, 'WYRD_AUTH_403_METHOD_UNAVAILABLE', 'Finding workspaces by email is not available', 'This deployment does not send email. Use a saved workspace URL, an invitation link, or ask your administrator.'),
  proofExpired: worded(400, 'WYRD_AUTH_400_INVALID_STATE', 'This link expired', 'Links last fifteen minutes. Start again to get a new one.'),
  unavailable: worded(502, 'WYRD_SPEC_502_UPSTREAM_FAILURE', 'Upstream dependency failed', 'Please try again later.')
};

/** Throttle titles by server code; Entry tells the person when to retry. */
const THROTTLES: Record<string, string> = {
  WYRD_AUTH_429_SIGNUP_ATTEMPTS: 'Too many workspace signups',
  WYRD_AUTH_429_EMAIL_ATTEMPTS: 'Too many email requests'
};

/**
 * The problem a refused answer shows: `known` when it names the server's
 * code, a throttle with its wait, the server's own public title and
 * remediation for another 4xx, else the unavailable state.
 */
export function refusal(answer: AuthAnswer, known: Record<string, WyrdProblem> = {}): WyrdProblem {
  const code = typeof answer.body?.code === 'string' ? answer.body.code : '';
  if (known[code]) return known[code];
  if (THROTTLES[code]) {
    const value = worded(429, code, THROTTLES[code], '');
    value.details = answer.body?.details ?? null;
    return value;
  }
  if (answer.status >= 400 && answer.status < 500 && typeof answer.body?.title === 'string')
    return worded(answer.status, code, answer.body.title, String(answer.body.remediation ?? ''));
  return PROBLEMS.unavailable;
}

/**
 * POST a signup email's proof from this browser. Answers null once Rust
 * holds the proof (and set the completion cookie), else the problem to show.
 */
export async function verifySignup(proof: string): Promise<WyrdProblem | null> {
  const answer = await auth('/auth/signup/verify', { proof });
  return answer.status === 200 ? null : refusal(answer, PROOF_REFUSALS);
}

/** The refusals of a pasted or linked email proof. */
export const PROOF_REFUSALS = {
  WYRD_AUTH_401_ONE_TIME_CODE_INVALID: PROBLEMS.proofInvalid,
  WYRD_AUTH_400_INVALID_STATE: PROBLEMS.proofExpired
};

/**
 * Open `page` with a pasted code as its proof fragment, exactly as the
 * message's link would, so one page verifies both.
 */
export function openProof(page: string, code: string): void {
  location.assign(`${page}#proof=${encodeURIComponent(code)}`);
}

/** The URL slug a workspace name suggests. */
export function slugify(name: string): string {
  return name
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '')
    .slice(0, 63)
    .replace(/-+$/, '');
}

/** Why `slug` is not a valid URL slug, or '' when it is. */
export function slugError(slug: string): string {
  if (!slug) return 'Enter a URL slug.';
  if (/[^a-z0-9-]/.test(slug)) return 'Use only lowercase letters, digits and hyphens.';
  if (slug.startsWith('-') || slug.endsWith('-')) return 'Start and end with a letter or digit, not a hyphen.';
  if (slug.length < 3) return 'Use at least 3 characters.';
  if (slug.length > 63) return 'Use at most 63 characters.';
  return '';
}

/** The password policy every local password path shares: 15–128 characters after NFC. */
export function passwordError(password: string): string {
  const length = [...password.normalize('NFC')].length;
  return length < 15 || length > 128 ? 'Use 15 to 128 characters.' : '';
}

/** An address the server would parse; the server stays the judge. */
export function emailError(email: string): string {
  return /^[^\s@]+@[^\s@]+$/.test(email.trim()) ? '' : 'Enter an email address.';
}
