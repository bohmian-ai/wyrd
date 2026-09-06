import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Badge from './Badge.svelte';

test('defaults to the neutral tone and draws no status glyph', () => {
  const { container } = render(Badge, { props: {} });
  expect(container.querySelector('.wy-badge')).toHaveAttribute('data-tone', 'neutral');
  expect(container.querySelector('.wy-badge .gl')).toBeNull();
});

test('pairs a glyph with every status tone', () => {
  for (const [tone, glyph] of [
    ['ok', '✓'],
    ['warn', '!'],
    ['danger', '✕'],
    ['running', '●']
  ] as const) {
    const { container } = render(Badge, { props: { tone } });
    const gl = container.querySelector('.wy-badge .gl');
    expect(gl?.textContent).toBe(glyph);
    expect(gl).toHaveAttribute('aria-hidden', 'true');
  }
});
