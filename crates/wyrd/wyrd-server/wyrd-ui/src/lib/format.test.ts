import { expect, test } from 'vitest';
import { fmtCost, fmtCount, fmtDuration } from './format';

test('fmtDuration switches from ms to seconds at 1000ms', () => {
  expect(fmtDuration(843)).toBe('843ms');
  expect(fmtDuration(4210)).toBe('4.21s');
});

test('fmtCount humanizes thousands and millions', () => {
  expect(fmtCount(200)).toBe('200');
  expect(fmtCount(1400)).toBe('1.4k');
  expect(fmtCount(12400)).toBe('12.4k');
  expect(fmtCount(1_200_000)).toBe('1.2M');
});

test('fmtCost renders fixed two-decimal USD', () => {
  expect(fmtCost(0.06)).toBe('$0.06');
  expect(fmtCost(42.1)).toBe('$42.10');
});
