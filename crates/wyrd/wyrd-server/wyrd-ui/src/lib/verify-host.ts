/**
 * The execution page's URL state: the selected node, the exact invocation
 * attempt, the opened output, completed details and page cursors. Every field
 * round-trips through the URL, so reload, a copied link and Back restore the
 * same exact selection.
 */
import type { ExecutionInput } from 'wyrd-ai-ui/loaders';

const index = (value: string | null) => (value === null ? null : Number(value));

/** The execution input a URL names; the catalog schema validates it before any read. */
export function readExecution(executionId: string, params: URLSearchParams): Record<string, unknown> {
  const attempt = params.get('attempt');
  return Object.fromEntries(
    Object.entries({
      executionId,
      node: params.get('node') ?? undefined,
      producer: attempt === null ? undefined : { item_index: index(params.get('item')), iteration_index: index(params.get('iteration')), attempt: Number(attempt) },
      output: params.get('output') ?? undefined,
      details: params.get('details') === '1' ? true : undefined,
      invocations: params.get('invocations') ?? undefined,
      publications: params.get('publications') ?? undefined
    }).filter(([, value]) => value !== undefined)
  );
}

/** The URL search for an execution selection, each value encoded on its own. */
export function writeExecution(input: Omit<ExecutionInput, 'executionId'>): string {
  const params = new URLSearchParams();
  if (input.node) params.set('node', input.node);
  if (input.producer) {
    if (input.producer.item_index != null) params.set('item', String(input.producer.item_index));
    if (input.producer.iteration_index != null) params.set('iteration', String(input.producer.iteration_index));
    params.set('attempt', String(input.producer.attempt));
  }
  if (input.output) params.set('output', input.output);
  if (input.details) params.set('details', '1');
  if (input.invocations) params.set('invocations', input.invocations);
  if (input.publications) params.set('publications', input.publications);
  const search = params.toString();
  return search ? `?${search}` : '';
}
