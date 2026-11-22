// Sample data and simulated server answers. Codes and titles marked `catalog` are the server's; the rest is this
// mock's wording (see README "Guesses").

export const PROVIDERS = { google: 'Google', github: 'GitHub' };

export const IDENTITIES = {
  google: { provider: 'google', email: 'sam@example.com' },
  // GitHub may prove an identity without a verified public email.
  github: { provider: 'github', email: null },
};

/** Slugs the simulated server already holds. */
export const TAKEN = ['acme', 'wyrd', 'demo'];
export const WORKSPACE_LIMIT = 3;

export const PROBLEMS = {
  // POST /auth/signup and /auth/signup/complete, 429 with Retry-After. Code and title: catalog.
  throttled: { status: 429, code: 'WYRD_AUTH_429_SIGNUP_ATTEMPTS', title: 'Too many workspace signups', remediation: '', details: { retry_after_seconds: 1380 } },
  // GET /signup?signup=failed: a redirect, so no problem body and no code.
  failed: { status: 401, code: null, title: 'Sign-in was not confirmed', remediation: 'The provider did not confirm who you are, so no workspace was created. Try again.' },
  taken: { status: 409, code: 'WYRD_SPEC_409_CONFLICT', title: 'That URL is already taken', remediation: 'Choose a different URL slug and create the workspace again.' },
  limit: { status: 403, code: 'WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT', title: 'Workspace limit reached', remediation: `This account already owns the maximum number of workspaces (${WORKSPACE_LIMIT}). Sign in to an existing workspace, or contact the operator to raise the limit.` },
  challenge: { status: 403, code: null, title: 'Challenge not passed', remediation: 'Complete the challenge below, then create the workspace again.' },
  disabled: { status: 401, code: 'WYRD_AUTH_401_INVALID_TOKEN', title: 'Workspace signup is no longer available', remediation: 'The operator turned off signup, or this sign-in method, after you started. No workspace was created.' },
  expired: { status: 400, code: 'WYRD_AUTH_400_INVALID_STATE', title: 'This signup expired', remediation: 'Start again to confirm who you are, then name your workspace.' },
};

/** As wyrd-ui's Entry.svelte `guidance`: when to retry a throttled attempt, else the remediation. */
export function guidance(issue) {
  const wait = issue.details?.retry_after_seconds;
  if (!wait) return issue.remediation;
  const minutes = Math.ceil(wait / 60);
  return `Try again in ${wait < 60 ? `${wait} seconds` : `${minutes} minute${minutes === 1 ? '' : 's'}`}.`;
}

/** The URL slug a workspace name suggests. */
export function slugify(name) {
  return name.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+/, '').slice(0, 63).replace(/-+$/, '');
}

/** Why `slug` is not a valid URL slug, or '' when it is. */
export function slugError(slug) {
  if (!slug) return 'Enter a URL slug.';
  if (/[^a-z0-9-]/.test(slug)) return 'Use only lowercase letters, digits and hyphens.';
  if (slug.startsWith('-') || slug.endsWith('-')) return 'Start and end with a letter or digit, not a hyphen.';
  if (slug.length < 3) return 'Use at least 3 characters.';
  if (slug.length > 63) return 'Use at most 63 characters.';
  return '';
}

/** Every preview state, as the strip lists them: [label, hash]. */
export const STATES = [
  ['Signup · Google and GitHub', '#/signup'],
  ['Signup · one provider', '#/signup?providers=github'],
  ['Signup · disabled by operator', '#/signup?preview=disabled'],
  ['Signup · provider proof refused', '#/signup?signup=failed'],
  ['Signup · too many attempts (429)', '#/signup?preview=throttled'],
  ['Workspace · Google, empty form', '#/signup/workspace'],
  ['Workspace · GitHub, no email', '#/signup/workspace?identity=github'],
  ['Workspace · with Turnstile', '#/signup/workspace?challenge=on'],
  ['Workspace · invalid slug', '#/signup/workspace?preview=invalid'],
  ['Workspace · slug taken (409)', '#/signup/workspace?preview=taken'],
  ['Workspace · too many attempts (429)', '#/signup/workspace?preview=throttled'],
  ['Workspace · workspace limit (403)', '#/signup/workspace?preview=limit'],
  ['Workspace · challenge failed', '#/signup/workspace?challenge=on&preview=challenge'],
  ['Workspace · signup no longer enabled', '#/signup/workspace?preview=disabled'],
  ['Workspace · proof expired', '#/signup/workspace?preview=expired'],
  ['Workspace · creating', '#/signup/workspace?preview=creating'],
  ['Workspace · created', '#/signup/workspace?preview=created'],
  ['Landing · /t/acme-research', '#/t/acme-research'],
  ['Sign-in (existing page, placeholder)', '#/'],
];
