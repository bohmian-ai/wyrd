import type { WyrdProblem } from '$lib/server/problem';

/** Console page wording for the refusals an operator meets, keyed by where they meet them; the code stays Rust's. */
const titles = {
  signIn: { title: 'Sign-in not accepted', remediation: 'Check your operator username and password. Workspace accounts and API keys do not sign in here.' },
  identity: { title: 'That identity is not a platform operator', remediation: 'Only operators registered on this deployment sign in here. Try again, or ask another operator to register you.' },
  reauth: { title: 'Password not accepted', remediation: "Enter your own operator password. Another operator's password or a bootstrap key does not confirm this session." },
  throttled: { title: 'Too many sign-in attempts' },
  noSso: { title: 'Platform SSO is not configured', remediation: 'Sign in with your operator username and password.' },
  conflict: { title: 'This provider changed', remediation: 'A newer revision was saved since this page loaded. Reload, then test that revision.' }
} as const;

/** `issue` with the console's wording for `where` when it is that refusal (`status`), else unchanged. */
export function worded(issue: WyrdProblem, where: keyof typeof titles, status: number): WyrdProblem {
  return issue.status === status ? { ...issue, ...titles[where] } : issue;
}

/** A Rust timestamp as the console shows it: `2026-11-24 02:04 UTC`. */
export function utc(at: string): string {
  return `${new Date(at).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}
