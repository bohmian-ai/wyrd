import { expect, test } from 'vitest';
import { flushSync } from 'svelte';
import { Streamed } from './streamed.svelte';

test('a streamed section keeps the newest load: older results and failures never replace it, its own failure does', async () => {
  const section = new Streamed<string>();
  let older = (_: string) => {};
  let fail = (_: Error) => {};
  section.follow('first');
  section.follow(new Promise<string>((resolve) => (older = resolve)));
  const newest = new Promise<string>((_, reject) => (fail = reject));
  section.follow(newest);
  // Until the newest settles, the previous value stays; a superseded result is ignored.
  older('stale');
  await Promise.resolve();
  flushSync();
  expect(section.value).toBe('first');
  fail(new Error('connection reset'));
  await newest.catch(() => {});
  await Promise.resolve();
  expect(section.value).toBeUndefined();
  expect(section.problem).toMatchObject({ detail: 'connection reset' });
});
