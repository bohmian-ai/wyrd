import { describe, expect, it } from 'vitest';
import { pathD, pointAt } from './geometry';

describe('geometry', () => {
  it('walks a polyline by length', () => {
    const pts: [number, number][] = [[0, 0], [10, 0], [10, 30]];
    expect(pathD(pts)).toBe('M0 0 L10 0 L10 30');
    expect(pointAt(pts, 0)).toEqual([0, 0]);
    expect(pointAt(pts, 0.25)).toEqual([10, 0]);
    expect(pointAt(pts, 0.5)).toEqual([10, 10]);
    expect(pointAt(pts, 1)).toEqual([10, 30]);
  });
});
