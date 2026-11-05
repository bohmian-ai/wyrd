// @vitest-environment node
import { readFileSync } from 'node:fs';
import { expect, test } from 'vitest';
import { problem, safeProblem } from './problem';

const source = readFileSync(
  new URL('../../../../../../wyrd-spec/src/error.rs', import.meta.url),
  'utf8'
);

test('every BFF problem preserves the canonical Rust derive metadata', () => {
  for (const kind of [
    'unauthenticated',
    'expired',
    'denied',
    'internal',
    'upstream'
  ] as const) {
    const value = problem(kind);
    const attributes = source.match(
      new RegExp(
        `code = "${value.code}",\\s*status = (\\d+),\\s*title = "([^"\\n]+)",\\s*remediation = "([^"\\n]+)"`
      )
    );
    expect(attributes, value.code).not.toBeNull();
    expect(value.status).toBe(Number(attributes![1]));
    expect(value.title).toBe(attributes![2]);
    expect(value.remediation).toBe(attributes![3]);
    expect(value.type).toBe(`https://wyrd.dev/problems/${value.code}`);
  }
});

test('known upstream codes retain canonical metadata while diagnostics and unknown codes are discarded', () => {
  expect(
    safeProblem({
      ...problem('expired'),
      title: 'secret',
      detail: 'token',
      details: { password: 'secret' }
    })
  ).toEqual(problem('expired'));
  expect(safeProblem({ code: 'unknown', detail: 'secret' })).toEqual(problem('internal'));
  const mutable = problem('internal');
  mutable.details = { secret: 'value' };
  expect(problem('internal').details).toEqual({});
});

test('a throttled password attempt keeps only its positive whole retry wait', () => {
  const throttled = { ...problem('passwordAttempts'), detail: 'secret', details: { retry_after_seconds: 840, password: 'secret' } };
  expect(safeProblem(throttled)).toEqual({ ...problem('passwordAttempts'), details: { retry_after_seconds: 840 } });
  for (const wait of [0, -1, 1.5, '840', null]) {
    expect(safeProblem({ ...throttled, details: { retry_after_seconds: wait } })).toEqual(problem('passwordAttempts'));
  }
  expect(safeProblem({ ...problem('validation'), details: { retry_after_seconds: 840 } })).toEqual(problem('validation'));
});
