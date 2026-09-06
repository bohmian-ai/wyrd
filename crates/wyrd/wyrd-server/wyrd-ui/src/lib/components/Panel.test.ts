import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Panel from './Panel.svelte';

test('defaults to the quiet wyrd panel', () => {
  const { container } = render(Panel, { props: {} });
  const panel = container.querySelector('.wy-panel');
  expect(panel).toHaveAttribute('data-variant', 'quiet');
});

test('renders a head only when a title or head snippet is supplied', () => {
  const bare = render(Panel, { props: {} });
  expect(bare.container.querySelector('.wy-panel-head')).toBeNull();

  const titled = render(Panel, { props: { title: 'Verification' } });
  expect(titled.container.querySelector('.wy-panel-head .t')?.textContent).toBe('Verification');
});

test('applies the raised altitude', () => {
  const { container } = render(Panel, { props: { variant: 'raised' } });
  const panel = container.querySelector('.wy-panel');
  expect(panel).toHaveAttribute('data-variant', 'raised');
});
