import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Spark from './Spark.svelte';

test('plots one point per value', () => {
  const { container } = render(Spark, { props: { points: [1, 4, 2, 8] } });
  const poly = container.querySelector('.wy-spark polyline');
  expect(poly?.getAttribute('points')?.split(' ')).toHaveLength(4);
});

test('reflects sentiment', () => {
  const { container } = render(Spark, { props: { points: [1, 2], sentiment: 'danger' } });
  expect(container.querySelector('.wy-spark')).toHaveAttribute('data-sentiment', 'danger');
});
