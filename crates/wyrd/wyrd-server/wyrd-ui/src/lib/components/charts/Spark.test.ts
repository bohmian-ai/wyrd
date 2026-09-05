import { readFileSync } from 'node:fs';
import type { ComponentProps } from 'svelte';
import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Spark from './Spark.svelte';

test('plots one point per value', () => {
  const { container } = render(Spark, { props: { points: [1, 4, 2, 8] } });
  const poly = container.querySelector('.wy-spark polyline');
  expect(poly?.getAttribute('points')?.split(' ')).toHaveLength(4);
});

test('is neutral geometry with no status input', () => {
  const props: Record<keyof ComponentProps<typeof Spark>, true> = {
    points: true, width: true, height: true
  };
  const manifest = JSON.parse(readFileSync('brand/components.json', 'utf8'));
  expect(Object.keys(manifest.components.Spark.props).sort()).toEqual(Object.keys(props).sort());
  expect(manifest.components.Spark.tokens).toEqual(['--muted']);
  const source = readFileSync('src/lib/components/charts/Spark.svelte', 'utf8');
  expect(source).not.toMatch(/sentiment|--ok|--warn|--danger/i);
  const { container } = render(Spark, { props: { points: [1, 2] } });
  expect(container.querySelector('polyline')).toHaveAttribute('stroke', 'var(--muted)');
  expect(container.querySelector('.wy-spark')).not.toHaveAttribute('data-sentiment');
});
