import type { Check, Claim } from './types';

/** One octicon vocabulary for Claim and Check status across the Change workspace. */
export type IconState = 'pass' | 'fail' | 'running' | 'queued' | 'pending' | 'attention';

/** Maps a Verifier check to its octicon: verdict first, then execution progress. */
export function checkState(check: Check): IconState {
  if (check.verdict === 'passed') return 'pass';
  if (check.verdict === 'failed') return 'fail';
  if (check.execution === 'running') return 'running';
  if (check.execution === 'queued') return 'queued';
  return 'pending';
}

/** Maps a Claim resolution to its octicon; pending claims warrant attention, not failure. */
export function claimState(claim: Claim): IconState {
  if (claim.resolution === 'satisfied') return 'pass';
  if (claim.resolution === 'not_satisfied') return 'fail';
  return 'attention';
}
