import { expect, test } from 'vitest';
import { projectTrace } from '$lib/server/observe/project';

test('an unknown trace projects nothing rather than a fabricated detail', () => {
  expect(projectTrace('trace_nope', '')).toBeNull();
});

test('the default span selection is the error span, never an arbitrary row', () => {
  const detail = projectTrace('trace_01', '');
  expect(detail).not.toBeNull();
  expect(detail!.selected.error).toBe(true);
  expect(detail!.selected.name).toBe('ledger.capture');
});

test('span selection restores from the URL and falls back safely for unknown spans', () => {
  const rank = projectTrace('trace_01', 'span_rank')!;
  expect(rank.selected.name).toBe('rank.candidates');
  const unknown = projectTrace('trace_01', 'span_missing')!;
  expect(unknown.selected.error).toBe(true);
});

test('AI content is present on model spans and explicitly absent on tool spans', () => {
  const detail = projectTrace('trace_01', 'span_rank')!;
  expect(detail.selected.aiContent?.prompt).toContain('capture candidates');
  const tool = projectTrace('trace_01', 'span_0c41')!;
  expect(tool.selected.aiContent).toBeNull();
  expect(tool.selected.aiAbsentReason).toContain('absent, not hidden');
});

test('the error span links its retry sibling and the graph names the error edge', () => {
  const detail = projectTrace('trace_01', '')!;
  expect(detail.selected.link?.href).toBe('trace_04');
  const errorEdges = detail.graph.edges.filter((edge) => edge.error);
  expect(errorEdges).toEqual([
    { from: 'checkout-agent', to: 'ledger-api', error: true, label: '504' }
  ]);
  expect(detail.graph.nodes.map((node) => node.cardHref)).toContain('card_service_02');
});

test('projection returns copies — callers cannot mutate the stored fixture', () => {
  const first = projectTrace('trace_01', '')!;
  first.spans[0].name = 'mutated';
  expect(projectTrace('trace_01', '')!.spans[0].name).toBe('POST /checkout/capture');
});
