import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Bars from './Bars.svelte';

test('renders one bar per datum', () => {
  const { container } = render(Bars, {
    props: { data: [{ label: 'a', value: 3 }, { label: 'b', value: 6 }, { label: 'c', value: 1 }] }
  });
  expect(container.querySelectorAll('.wy-bars .bar')).toHaveLength(3);
});

test('applies a per-bar color', () => {
  const { container } = render(Bars, {
    props: { data: [{ label: 'a', value: 3, color: 'var(--client-bar)' }] }
  });
  expect(container.querySelector('.wy-bars .bar')?.getAttribute('style')).toContain('var(--client-bar)');
});
