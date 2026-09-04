import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Bars from './Bars.svelte';

test('draws one bar per datum with its value and category label', () => {
  const { container } = render(Bars, {
    props: { data: [{ label: 'a', value: 3 }, { label: 'b', value: 6 }] }
  });
  expect(container.querySelectorAll('.wy-bars .bar')).toHaveLength(2);
  expect(container.querySelectorAll('.wy-bars .cat')).toHaveLength(2);
  expect(container.querySelectorAll('.wy-bars .val')).toHaveLength(2);
});

test('names every category and value in the accessible label', () => {
  const { container } = render(Bars, {
    props: { data: [{ label: 'unit', value: 3 }], unit: 'records' }
  });
  expect(container.querySelector('.wy-bars')).toHaveAttribute('aria-label', 'unit 3 records');
});
