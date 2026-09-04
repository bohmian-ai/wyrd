import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Line from './Line.svelte';

const series = [
  { label: 'p50', points: [1, 2, 3] },
  { label: 'p95', points: [3, 2, 1] }
];

test('draws one polyline per series', () => {
  const { container } = render(Line, { props: { series, labels: ['a', '', 'c'] } });
  expect(container.querySelectorAll('.wy-line svg polyline')).toHaveLength(2);
});

test('renders only non-empty x labels', () => {
  const { container } = render(Line, { props: { series: [series[0]], labels: ['a', '', 'c'] } });
  expect(container.querySelectorAll('.wy-line .xl')).toHaveLength(2);
});
