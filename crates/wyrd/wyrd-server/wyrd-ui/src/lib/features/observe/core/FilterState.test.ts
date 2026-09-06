import { describe, expect, test } from 'vitest';
import { filterChips, readScope, scopeQuery, withScope, RANGES } from './filter-state';

test('readScope defaults the range and rejects unknown ranges', () => {
  expect(readScope(new URLSearchParams(''))).toEqual({ service: '', range: '1h' });
  expect(readScope(new URLSearchParams('service=checkout-api&range=6h'))).toEqual({
    service: 'checkout-api',
    range: '6h'
  });
  expect(readScope(new URLSearchParams('range=nonsense'))).toEqual({ service: '', range: '1h' });
  expect(readScope(new URLSearchParams(''), '30d').range).toBe('30d');
  for (const range of RANGES)
    expect(readScope(new URLSearchParams(`range=${range}`)).range).toBe(range);
});

test('scopeQuery writes the scope explicitly and omits empty values', () => {
  expect(scopeQuery({ service: 'checkout-api', range: '1h' })).toBe(
    '?service=checkout-api&range=1h'
  );
  expect(scopeQuery({ service: '', range: '1h' })).toBe('?range=1h');
  expect(scopeQuery({ service: '', range: '1h' }, { status: 'error', q: '' })).toBe(
    '?range=1h&status=error'
  );
});

test('withScope carries the scope into cross-signal links', () => {
  expect(
    withScope('/t/acme/observe/traces', { service: 'checkout-api', range: '1h' }, { status: 'error' })
  ).toBe('/t/acme/observe/traces?service=checkout-api&range=1h&status=error');
});

describe('filterChips', () => {
  const chips = filterChips('/t/acme/observe/logs', {
    service: 'checkout-api',
    level: 'error',
    range: '1h'
  });

  test('renders one removable chip per active filter', () => {
    expect(chips.map((chip) => [chip.label, chip.value])).toEqual([
      ['service', 'checkout-api'],
      ['level', 'error'],
      ['range', '1h']
    ]);
  });

  test('each remove link keeps every other filter', () => {
    expect(chips[0].removeHref).toBe('/t/acme/observe/logs?level=error&range=1h');
    expect(chips[1].removeHref).toBe('/t/acme/observe/logs?service=checkout-api&range=1h');
    expect(chips[2].removeHref).toBe('/t/acme/observe/logs?service=checkout-api&level=error');
  });

  test('empty filters produce no chips and removing the last filter is the bare route', () => {
    expect(filterChips('/t/acme/observe/logs', { service: '', level: '' })).toEqual([]);
    expect(filterChips('/t/acme/observe/logs', { level: 'error' })[0].removeHref).toBe(
      '/t/acme/observe/logs'
    );
  });
});
