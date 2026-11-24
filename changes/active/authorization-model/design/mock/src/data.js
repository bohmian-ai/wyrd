// Sample data and simulated server answers. Codes and titles marked `catalog` are the server's; the rest is this
// mock's wording (see README "Guesses").

export const PROVIDERS = { google: 'Google', github: 'GitHub' };

export const IDENTITIES = {
  // An email proof: the creator then chooses a tenant-local username and password.
  email: { provider: 'email', email: null },
  google: { provider: 'google', email: 'sam@example.test' },
  // GitHub may prove an identity without a verified public email.
  github: { provider: 'github', email: null },
};

/** Slugs the simulated server already holds. */
export const TAKEN = ['acme', 'wyrd', 'demo'];

export const PROBLEMS = {
  // POST /auth/signup/complete for a provider proof without a verified email: 409 with details.reason contact_required.
  contact: { status: 409, code: 'WYRD_SPEC_409_CONFLICT', title: 'Confirm an email address to continue', remediation: 'GitHub did not share a verified email address. Confirm one by email; you still sign in with GitHub and set no password.' },
  // POST /auth/signup and /auth/signup/complete, 429 with Retry-After. Code and title: catalog.
  throttled: { status: 429, code: 'WYRD_AUTH_429_SIGNUP_ATTEMPTS', title: 'Too many workspace signups', remediation: '', details: { retry_after_seconds: 1380 } },
  // GET /signup?signup=failed: a redirect, so no problem body and no code.
  failed: { status: 401, code: null, title: 'Sign-in was not confirmed', remediation: 'The provider did not confirm who you are, so no workspace was created. Try again.' },
  taken: { status: 409, code: 'WYRD_SPEC_409_CONFLICT', title: 'That URL is already taken', remediation: 'Choose a different URL slug and create the workspace again.' },
  limit: { status: 403, code: 'WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT', title: 'Workspace limit reached', remediation: `This account already owns the maximum number of workspaces. Sign in to an existing workspace, or contact the operator to raise the limit.` },
  challenge: { status: 403, code: 'WYRD_AUTH_403_SIGNUP_CHALLENGE', title: 'Challenge not passed', remediation: 'Complete the challenge below, then create the workspace again.' },
  disabled: { status: 401, code: 'WYRD_AUTH_401_INVALID_TOKEN', title: 'Workspace signup is no longer available', remediation: 'The operator turned off signup, or this sign-in method, after you started. No workspace was created.' },
  expired: { status: 400, code: 'WYRD_AUTH_400_INVALID_STATE', title: 'This signup expired', remediation: 'Start again to confirm who you are, then name your workspace.' },
  // Any 5xx; Entry words it as wyrd-ui's Entry.svelte does ("Sign-in unavailable"). Code: catalog.
  unavailable: { status: 502, code: 'WYRD_SPEC_502_UPSTREAM_FAILURE', title: 'Upstream dependency failed', remediation: 'Please try again later.' },
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
  ['Signup · email, Google and GitHub', '#/signup'],
  ['Signup · email and one provider', '#/signup?providers=github'],
  ['Signup · no mail relay: providers only', '#/signup?local=off'],
  ['Signup · disabled by operator', '#/signup?preview=disabled'],
  ['Signup · provider proof refused', '#/signup?signup=failed'],
  ['Signup · too many attempts (429)', '#/signup?preview=throttled'],
  ['Signup · check your email', '#/signup?preview=sent'],
  ['Signup verify · paste a code', '#/signup/verify'],
  ['Signup verify · confirming the link', '#/signup/verify?preview=checking'],
  ['Signup verify · email confirmed', '#/signup/verify?preview=proven'],
  ['Signup verify · link not accepted (401)', '#/signup/verify?preview=invalid'],
  ['Signup verify · link expired (400)', '#/signup/verify?preview=expired'],
  ['Workspace · reading the proof', '#/signup/workspace?preview=checking'],
  ['Workspace · Google, empty form', '#/signup/workspace'],
  ['Workspace · email proof, administrator sign-in', '#/signup/workspace?identity=email'],
  ['Workspace · GitHub, no email', '#/signup/workspace?identity=github'],
  ['Workspace · provider email unverified (409)', '#/signup/workspace?identity=github&preview=contact'],
  ['Workspace · provider email unverified, no relay', '#/signup/workspace?identity=github&local=off&preview=contact'],
  ['Workspace · with Turnstile', '#/signup/workspace?challenge=on'],
  ['Workspace · invalid slug', '#/signup/workspace?preview=invalid'],
  ['Workspace · slug taken (409)', '#/signup/workspace?preview=taken'],
  ['Workspace · too many attempts (429)', '#/signup/workspace?preview=throttled'],
  ['Workspace · workspace limit (403)', '#/signup/workspace?preview=limit'],
  ['Workspace · challenge failed', '#/signup/workspace?challenge=on&preview=challenge'],
  ['Workspace · signup no longer enabled', '#/signup/workspace?preview=disabled'],
  ['Workspace · proof expired', '#/signup/workspace?preview=expired'],
  ['Workspace · server failure (5xx)', '#/signup/workspace?preview=unavailable'],
  ['Workspace · creating', '#/signup/workspace?preview=creating'],
  ['Workspace · created', '#/signup/workspace?preview=created'],
  ['Landing · /t/acme-research', '#/t/acme-research'],
  ['Find workspaces · request', '#/workspaces'],
  ['Find workspaces · check your email', '#/workspaces?preview=sent'],
  ['Find workspaces · too many requests (429)', '#/workspaces?preview=throttled'],
  ['Find workspaces · no mail relay (403)', '#/workspaces?preview=unavailable'],
  ['Workspaces verify · paste a code', '#/workspaces/verify'],
  ['Workspaces verify · first page, more follow', '#/workspaces/verify?preview=proven'],
  ['Workspaces verify · none found', '#/workspaces/verify?preview=empty'],
  ['Workspaces verify · link not accepted (401)', '#/workspaces/verify?preview=invalid'],
  ['Workspaces verify · list expired (400)', '#/workspaces/verify?preview=read-expired'],
  ['Recovery email · request', '#/t/acme-research/settings/account'],
  ['Recovery email · check your email', '#/t/acme-research/settings/account?preview=sent'],
  ['Recovery email · recent sign-in required (403)', '#/t/acme-research/settings/account?preview=recent'],
  ['Recovery email · no mail relay (403)', '#/t/acme-research/settings/account?preview=unavailable'],
  ['Recovery email · too many requests (429)', '#/t/acme-research/settings/account?preview=throttled'],
  ['Contact verify · verified', '#/t/acme-research/contact/verify?preview=proven'],
  ['Contact verify · link not accepted (401)', '#/t/acme-research/contact/verify?preview=invalid'],
  ['Contact verify · signed out (401)', '#/t/acme-research/contact/verify?preview=signed-out'],
  ['Reset · request a code or paste one', '#/t/acme-research/reset'],
  ['Reset · code requested (same for every account)', '#/t/acme-research/reset?preview=sent'],
  ['Reset · too many requests (429)', '#/t/acme-research/reset?preview=throttled'],
  ['Reset · no mail relay: paste a code only', '#/t/acme-research/reset?preview=nomail'],
  ['Reset · code not accepted (401)', '#/t/acme-research/reset?preview=invalid'],
  ['Reset · company SSO required (403)', '#/t/acme-research/reset?preview=sso'],
  ['Reset · password set, back at sign-in', '#/t/acme-research/login?reset=done'],
  ['Platform · sign-in', '#/platform'],
  ['Platform · sign-in refused (401)', '#/platform?preview=refused'],
  ['Platform · too many attempts (429)', '#/platform?preview=throttled'],
  ['Platform · no platform SSO configured', '#/platform?sso=off'],
  ['Platform · SSO removed since the page loaded (404)', '#/platform?preview=nosso'],
  ['Platform · server failure (5xx)', '#/platform?preview=unavailable'],
  ['Platform · session ended', '#/platform?preview=ended'],
  ['Platform · SSO return refused (401)', '#/platform/ui/callback?preview=refused'],
  ['Platform · SSO re-authentication confirmed', '#/platform/ui/callback?preview=reauth'],
  ['Platform · SSO re-authentication pop-up returning', '#/platform/ui/callback?preview=popup'],
  ['Console · overview', '#/platform/ui'],
  ['Console · re-authenticate with password', '#/platform/ui?preview=reauth'],
  ['Console · re-authenticate with platform SSO', '#/platform/ui?preview=reauth&method=oidc'],
  ['Console · re-authentication refused (401)', '#/platform/ui?preview=reauth-refused'],
  ['Console · social sign-in providers', '#/platform/ui/providers'],
  ['Console · provider changed (409)', '#/platform/ui/providers?preview=conflict'],
  ['Console · mail relay', '#/platform/ui/mail'],
  ['Console · no mail relay', '#/platform/ui/mail?preview=none'],
  ['Console · relay refused the test', '#/platform/ui/mail?preview=test-failed'],
  ['Console · signup settings', '#/platform/ui/signup'],
  ['Sign-in (existing page, placeholder)', '#/'],
];

// Cycle 15: email proofs, discovery, recovery contact, reset and the platform console. Codes are the server's; titles
// and remediations are this mock's wording unless the README says otherwise.
export const P = {
  proofInvalid: { status: 401, code: 'WYRD_AUTH_401_ONE_TIME_CODE_INVALID', title: 'This link was not accepted', remediation: 'Each link works once, and only in the browser that asked for it. Open the newest message in that browser, or start again.' },
  proofExpired: { status: 400, code: 'WYRD_AUTH_400_INVALID_STATE', title: 'This link expired', remediation: 'Links last fifteen minutes. Start again to get a new one.' },
  emailThrottled: { status: 429, code: 'WYRD_AUTH_429_EMAIL_ATTEMPTS', title: 'Too many email requests', remediation: '', details: { retry_after_seconds: 540 } },
  contactNoMail: { status: 409, code: 'WYRD_SPEC_409_CONFLICT', title: 'GitHub did not share a verified email address', remediation: 'This deployment cannot send email, so this account cannot create a workspace. Verify an email address at GitHub, then start again.' },
  discoveryOff: { status: 403, code: 'WYRD_AUTH_403_METHOD_UNAVAILABLE', title: 'Finding workspaces by email is not available', remediation: 'This deployment does not send email. Use a saved workspace URL, an invitation link, or ask your administrator.' },
  readExpired: { status: 400, code: 'WYRD_AUTH_400_INVALID_STATE', title: 'This list expired', remediation: 'The list stays readable for five minutes. Request a new link to see it again.' },
  recent: { status: 403, code: 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED', title: 'Sign in again first', remediation: 'Changing your recovery email needs a password or provider sign-in from the last five minutes.' },
  contactOff: { status: 403, code: 'WYRD_AUTH_403_METHOD_UNAVAILABLE', title: 'Recovery email is not available', remediation: 'This deployment does not send email. An administrator can still issue you a reset code.' },
  signedOut: { status: 401, code: 'WYRD_AUTH_401_INVALID_TOKEN', title: 'You are signed out', remediation: 'Sign in to this workspace in this browser, then open the link again.' },
  resetThrottled: { status: 429, code: 'WYRD_AUTH_429_PASSWORD_ATTEMPTS', title: 'Too many reset requests', remediation: '', details: { retry_after_seconds: 300 } },
  resetCode: { status: 401, code: 'WYRD_AUTH_401_ONE_TIME_CODE_INVALID', title: 'Reset code not accepted', remediation: 'A code works once, for fifteen minutes, and a newer request replaces it. Request a new code, or ask an administrator for one.' },
  resetOff: { status: 403, code: 'WYRD_AUTH_403_METHOD_UNAVAILABLE', title: 'Password reset is not available', remediation: 'This workspace does not offer password sign-in. Sign in with SSO from the sign-in page.' },
  platformRefused: { status: 401, code: 'WYRD_AUTH_401_UNAUTHENTICATED', title: 'Sign-in not accepted', remediation: 'Check your operator username and password. Workspace accounts and API keys do not sign in here.' },
  platformThrottled: { status: 429, code: 'WYRD_AUTH_429_SIGN_IN_ATTEMPTS', title: 'Too many sign-in attempts', remediation: '', details: { retry_after_seconds: 720 } },
  platformNoSso: { status: 404, code: 'WYRD_SPEC_404_NOT_FOUND', title: 'Platform SSO is not configured', remediation: 'Sign in with your operator username and password.' },
  platformIdentity: { status: 401, code: 'WYRD_AUTH_401_UNAUTHENTICATED', title: 'That identity is not a platform operator', remediation: 'Only operators registered on this deployment sign in here. Try again, or ask another operator to register you.' },
  reauthRefused: { status: 401, code: 'WYRD_AUTH_401_UNAUTHENTICATED', title: 'Password not accepted', remediation: "Enter your own operator password. Another operator's password or a bootstrap key does not confirm this session." },
  providerConflict: { status: 409, code: 'WYRD_SPEC_409_CONFLICT', title: 'This provider changed', remediation: 'A newer revision was saved since this page loaded. Reload, then test that revision.' },
};

/** Password policy shared by local signup, reset and platform users. */
export function passwordError(password) {
  const n = [...password.normalize('NFC')].length;
  return n < 15 || n > 128 ? 'Use 15 to 128 characters.' : '';
}

/** An address the server would parse; the server stays the judge. */
export const emailError = (email) => (/^[^\s@]+@[^\s@]+$/.test(email.trim()) ? '' : 'Enter an email address.');

/** The workspaces one verified address reaches, in route-key order; the server pages them 20 at a time. */
export const DISCOVERED = ['acme-research', 'aurora', 'birch', 'cedar', 'delta-ops', 'ember', 'fjord', 'granite', 'harbor', 'iris', 'juniper', 'kestrel', 'lumen', 'meridian', 'nimbus', 'orchid', 'pioneer', 'quartz', 'rowan', 'summit', 'tundra', 'umber', 'vale']
  .map((tenant) => ({ tenant, display_name: tenant.split('-').map((w) => w[0].toUpperCase() + w.slice(1)).join(' '), login_url: `/t/${tenant}/login` }));
export const PAGE_SIZE = 20;

/** GET /platform/tenants, newest first. */
export const TENANTS = [
  { slug: 'acme-research', display_name: 'Acme Research', status: 'active' },
  { slug: 'north-lab', display_name: 'North Lab', status: 'suspended' },
  { slug: 'trial-7', display_name: 'Trial 7', status: 'provisioning' },
  { slug: 'pilot-old', display_name: 'Pilot (old)', status: 'failed' },
];

/** GET /platform/social/providers. Secrets never appear. */
export const SOCIAL = [
  { provider: 'google', client_id: '000000000000-example.apps.googleusercontent.com', revision: 3, state: 'enabled', callback_url: 'https://wyrd.example.test/auth/social/google/callback', registration_instructions: 'In Google Cloud console, create an OAuth client of type Web application and add the callback URL as an authorized redirect URI.' },
  { provider: 'github', client_id: 'Iv1.example0000', revision: 1, state: 'untested', callback_url: 'https://wyrd.example.test/auth/social/github/callback', registration_instructions: 'In GitHub developer settings, register an OAuth app with the callback URL as its authorization callback URL. Request no repository access.' },
];

/** GET /platform/mail. */
export const RELAY = { host: 'smtp.example.test', port: 587, tls: 'starttls', username: 'relay-user', password_set: true, sender: 'no-reply@example.test' };

/** GET /platform/login-settings; the challenge secret is write-only. */
export const LOGIN_SETTINGS = { password_enabled: true, signup_enabled: true, signup_workspace_cap: 3, signup_challenge: { site_key: '0x4AAAAAAAexample' } };

/** The mock's platform session (GET /platform/browser/session). Module state survives the router's page re-creation. */
export const platformSession = { name: 'ops-oncall', method: 'password', expires_at: '2026-11-24 02:04 UTC', idle_expires_at: '2026-11-23 18:19 UTC', recent: false };
