import { expect, test } from 'vitest';
import { projectTrace } from '$lib/server/mock/observe/project';

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

test('GenAI records join onto extracted spans and are simply absent elsewhere', () => {
  const rank = projectTrace('trace_01', 'span_rank')!;
  expect(rank.selected.genai?.table).toBe('messages');
  const rankRecord = rank.selected.genai!;
  if (rankRecord.table === 'messages') {
    expect(rankRecord.inputMessages?.[0].content).toContain('capture candidates');
    expect(rankRecord.usage.input).toBe(412);
  }
  const tool = projectTrace('trace_01', 'span_0c41')!;
  expect(tool.selected.genai?.table).toBe('tool_calls');
  expect(tool.selected.genai?.errorType).toBe('GatewayTimeout');
  const plain = projectTrace('trace_01', 'span_auth')!;
  expect(plain.selected.genai).toBeNull();
});

test('the error span carries its exception event, link and parent per the traces contract', () => {
  const detail = projectTrace('trace_01', '')!;
  expect(detail.selected.links[0]).toMatchObject({
    linkedTraceId: 'trace_04',
    linkedSpanId: 'span_1a02'
  });
  const exception = detail.selected.events.find((event) => event.name === 'exception');
  expect(exception?.attributes['exception.type']).toBe('GatewayTimeout');
  expect(detail.selected.parentSpanId).toBe('span_decide');
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
