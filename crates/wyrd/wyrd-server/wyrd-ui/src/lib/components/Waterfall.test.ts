import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Waterfall from './Waterfall.svelte';

const spans = [
  { depth: 0, name: 'agent.run', kind: 'agent' as const, start: 0, duration: 1840, status: 'ok' as const },
  { depth: 1, name: 'tool.web_fetch', kind: 'tool' as const, start: 1020, duration: 160, status: 'err' as const }
];

test('positions one bar per span and colors errors danger', () => {
  const { container } = render(Waterfall, { props: { spans } });
  const bars = container.querySelectorAll<HTMLElement>('.span');
  expect(bars).toHaveLength(2);
  const errBar = bars[1];
  expect(errBar.getAttribute('style')).toContain('var(--danger)');
});
