import { expect, test } from 'vitest';
import { flushSync } from 'svelte';
import { Streamed } from './streamed.svelte';

test('a streamed section keeps the newest load: older results and failures never replace it, its own failure does', async () => {
  const section = new Streamed<string>();
  let older = (_: string) => {};
  let fail = (_: Error) => {};
  section.follow('first', '/t/acme/verify');
  section.follow(new Promise<string>((resolve) => (older = resolve)), '/t/acme/verify');
  const newest = new Promise<string>((_, reject) => (fail = reject));
  section.follow(newest, '/t/acme/verify');
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

test('a remounted page shows the last section its address settled until the newest load settles', async () => {
  const remembered = new Map<string, string>();
  const first = new Streamed<string>(remembered);
  first.follow(Promise.resolve('dashboard'), '/t/acme/verify');
  await Promise.resolve();
  await Promise.resolve();
  // Back remounts the page: it shows the remembered section at once, never another address's.
  let fresh = (_: string) => {};
  const newest = new Promise<string>((resolve) => (fresh = resolve));
  const back = new Streamed<string>(remembered);
  back.follow(newest, '/t/acme/verify');
  expect(back.value).toBe('dashboard');
  const elsewhere = new Streamed<string>(remembered);
  elsewhere.follow(new Promise<string>(() => {}), '/t/other/verify');
  expect(elsewhere.value).toBeUndefined();
  fresh('current');
  await newest;
  await Promise.resolve();
  expect(back.value).toBe('current');
});

test('a read whose successful answer is not JSON fails the section instead of clearing it', async () => {
  const section = new Streamed<string>();
  section.follow('first', '/t/acme/verify');
  const fetched = globalThis.fetch;
  globalThis.fetch = async () => new Response('<html>proxy page</html>', { status: 200 });
  try {
    await section.read('/t/acme/verify/support', '/t/acme/verify');
  } finally {
    globalThis.fetch = fetched;
  }
  flushSync();
  expect(section.value).toBeUndefined();
  expect(section.problem).toMatchObject({ code: 'WYRD_SPEC_502_UPSTREAM_FAILURE' });
});

test('a section is pending from a newer population\'s load until that load settles, whatever it still shows', async () => {
  const section = new Streamed<string>();
  section.follow('first', '/t/acme/verify');
  expect(section.pending).toBe(false);
  let settle = (_: string) => {};
  const newer = new Promise<string>((resolve) => (settle = resolve));
  section.follow(newer, '/t/acme/verify?status=failed');
  // The old value is still shown, and marked as waiting for its successor.
  expect([section.value, section.pending]).toEqual(['first', true]);
  settle('second');
  await newer;
  await Promise.resolve();
  expect([section.value, section.pending]).toEqual(['second', false]);
  // A failure settles it too.
  let fail = (_: Error) => {};
  const failing = new Promise<string>((_, reject) => (fail = reject));
  section.follow(failing, '/t/acme/verify?status=queued');
  expect(section.pending).toBe(true);
  fail(new Error('reset'));
  await failing.catch(() => {});
  await Promise.resolve();
  expect([section.pending, section.problem?.detail]).toEqual([false, 'reset']);
  // A value given as it is never waits.
  section.follow(newer, '/t/acme/verify?status=passed');
  section.follow('third', '/t/acme/verify?status=passed');
  expect(section.pending).toBe(false);
});

test('a refresh of the population a section shows is not pending; Back to a remembered one is', async () => {
  const remembered = new Map<string, string>();
  const section = new Streamed<string>(remembered);
  section.follow('counted', '/t/acme/verify?status=failed');
  // Live rereads the population it shows: the section is current until the reread settles.
  section.follow(new Promise<string>(() => {}), '/t/acme/verify?status=failed');
  expect(section.pending).toBe(false);
  // Back remounts with the remembered value of an earlier read, which the newest read replaces.
  const back = new Streamed<string>(remembered);
  back.follow(new Promise<string>(() => {}), '/t/acme/verify?status=failed');
  expect([back.value, back.pending]).toEqual(['counted', true]);
});
