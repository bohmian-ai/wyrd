import { render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import SpanPanel from './SpanPanel.svelte';

const base = {
  name: 'llm.synthesize',
  kind: 'llm' as const,
  attributes: [
    { label: 'span_id', value: 'span_7c2a4f' },
    { label: 'trace_id', value: 'tr_9f2a', link: true }
  ],
  tokens: { input: 1400, output: 1000, cache: 200 },
  outputPreview: 'Based on the retrieved policy docs…'
};

test('renders attributes, token stats, and the output preview', () => {
  const { container, getByText } = render(SpanPanel, { props: base });
  expect(getByText('llm.synthesize')).toBeTruthy();
  expect(container.querySelectorAll('.d-mstat')).toHaveLength(3);
  expect(getByText('1.4k')).toBeTruthy();
  expect(container.querySelector('pre code')?.textContent).toContain('retrieved policy docs');
});

test('an errored span turns the drawer accent danger', () => {
  const { container } = render(SpanPanel, { props: { ...base, status: 'err' } });
  expect(container.querySelector('.wy-drawer')?.getAttribute('style')).toContain('var(--danger)');
});
