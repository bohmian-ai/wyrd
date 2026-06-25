import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Badge from './Badge.svelte';

test('defaults to the neutral tone', () => {
  const { container } = render(Badge, { props: {} });
  expect(container.querySelector('.wy-badge')).toHaveAttribute('data-tone', 'neutral');
});

test('applies the danger tone', () => {
  const { container } = render(Badge, { props: { tone: 'danger' } });
  expect(container.querySelector('.wy-badge')).toHaveAttribute('data-tone', 'danger');
});
