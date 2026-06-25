import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Card from './Card.svelte';

test('defaults to the quiet variant', () => {
  const { container } = render(Card, { props: {} });
  expect(container.querySelector('.wy-card')).toHaveAttribute('data-variant', 'quiet');
});

test('applies the raised variant', () => {
  const { container } = render(Card, { props: { variant: 'raised' } });
  expect(container.querySelector('.wy-card')).toHaveAttribute('data-variant', 'raised');
});

test('omits the head region when no head snippet is given', () => {
  const { container } = render(Card, { props: {} });
  expect(container.querySelector('.wy-card-head')).toBeNull();
});
