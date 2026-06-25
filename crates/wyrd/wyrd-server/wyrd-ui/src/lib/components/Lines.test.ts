import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Lines from './Lines.svelte';

test('draws one polyline per series', () => {
  const { container } = render(Lines, {
    props: { series: [{ points: [1, 2, 3] }, { points: [3, 2, 1] }], labels: ['a', '', 'c'] }
  });
  expect(container.querySelectorAll('.wy-lines polyline')).toHaveLength(2);
});

test('renders only non-empty x labels', () => {
  const { container } = render(Lines, {
    props: { series: [{ points: [1, 2, 3] }], labels: ['a', '', 'c'] }
  });
  expect(container.querySelectorAll('.wy-lines .xl')).toHaveLength(2);
});
