import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Trend from './Trend.svelte';

test('flags only the points above threshold', () => {
  const { container } = render(Trend, { props: { points: [0.1, 0.25, 0.15, 0.3], threshold: 0.2 } });
  expect(container.querySelectorAll('.wy-trend .node')).toHaveLength(4);
  expect(container.querySelectorAll('.wy-trend .node.over')).toHaveLength(2);
});
