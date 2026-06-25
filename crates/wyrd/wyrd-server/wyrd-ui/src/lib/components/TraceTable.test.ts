import { fireEvent, render } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import TraceTable from './TraceTable.svelte';

const rows = [
  { id: 'tr_9f2a', op: 'agent.run', kind: 'agent' as const, spans: 7, durationMs: 1840, tokens: 12400, cost: 0.18, score: 0.92, status: 'ok' as const },
  { id: 'tr_6a9e', op: 'tool.exec', kind: 'tool' as const, spans: 4, durationMs: 410, status: 'err' as const }
];

test('keys the root-op left-bar by span kind', () => {
  const { container } = render(TraceTable, { props: { rows } });
  expect(container.querySelectorAll('tbody tr')).toHaveLength(2);
  expect(container.querySelector('.op[data-k="agent"]')).toBeTruthy();
  expect(container.querySelector('.st[data-st="err"]')).toBeTruthy();
});

test('formats raw column values for display', () => {
  const { getByText } = render(TraceTable, { props: { rows } });
  expect(getByText('1.84s')).toBeTruthy();
  expect(getByText('12.4k')).toBeTruthy();
  expect(getByText('$0.18')).toBeTruthy();
  expect(getByText('0.92')).toBeTruthy();
});

test('emits the selected trace id on row click', async () => {
  let picked = '';
  const { getByText } = render(TraceTable, { props: { rows, onselect: (id: string) => (picked = id) } });
  await fireEvent.click(getByText('tr_6a9e'));
  expect(picked).toBe('tr_6a9e');
});
