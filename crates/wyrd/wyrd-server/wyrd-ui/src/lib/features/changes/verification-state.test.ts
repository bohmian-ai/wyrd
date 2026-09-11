// @vitest-environment node
import { expect, test } from 'vitest';
import { fixtureChange } from '$lib/server/mock/changes/fixtures';
test('server fixtures separate judgment, execution, provenance and authorization', () => {
  const change = fixtureChange();
  const checks = change.claims.flatMap((claim) => claim.checks);
  expect(checks.length).toBeGreaterThanOrEqual(7);
  expect(change.override).toContain('does not verify');
  expect(change.claims[2].resolution).toBe('not_satisfied');
  expect(new Set(checks.map((check) => check.provenance))).toEqual(
    new Set(['current', 'stale', 'carried_forward'])
  );
  for (const check of checks) {
    expect(check.verdict !== null).toBe(check.execution === 'completed');
    if (
      check.execution === 'running' ||
      check.execution === 'queued' ||
      check.evidence.some((item) => !item.present)
    )
      expect(check.eligible).toBe(false);
  }
});
