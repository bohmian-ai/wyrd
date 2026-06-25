import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Heatmap from './Heatmap.svelte';

test('renders one cell per value with the requested column count', () => {
  const { container } = render(Heatmap, { props: { values: [0, 1, 2, 3, 4, 2], columns: 3 } });
  expect(container.querySelectorAll('.wy-heat span')).toHaveLength(6);
  expect(container.querySelector('.wy-heat')?.getAttribute('style')).toContain('repeat(3,1fr)');
});
