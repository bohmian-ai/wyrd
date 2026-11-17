/**
 * The Verify dashboard's loader inputs and intent steps over Observe's URL
 * state, and the execution page's URL state: the selected node, the exact invocation
 * attempt, the opened output, completed details and page cursors. Every field
 * round-trips through the URL, so reload, a copied link and Back restore the
 * same exact selection.
 */
import type { ActionIntent } from 'wyrd-ai-ui';
import { sortColumns, type ExecutionInput } from 'wyrd-ai-ui/loaders';
import { step, type HostStep, type ObserveState } from './observe-host';

/** The columns the Verify rail counts. */
export const VERIFY_FACETS = ['status', 'verdict', 'verifier_card_uid', 'service_name', 'subject_name', 'run_id', 'group_id'] as const;
/** The shown columns the Verify list may group by. Grouping only arranges rows; it is never a combined judgment. */
export const VERIFY_GROUPS = ['run_id', 'subject_name', 'service_name', 'group_id'] as const;

/**
 * The shared loader inputs of the Verify dashboard, read from the same URL
 * state Observe keeps: the facets and the list filter by the typed WHERE
 * clauses only, so rows, counts and charts describe one population.
 */
export function verifyInputs(state: ObserveState) {
  const group = VERIFY_GROUPS.find((each) => each === state.group);
  const sort = state.sort && sortColumns('executions').includes(state.sort.column) ? state.sort : undefined;
  return {
    where: state.where,
    facets: { signal: 'executions', facets: [...VERIFY_FACETS], where: state.where },
    list: {
      signal: 'executions',
      where: state.where,
      ...(group ? { groupBy: group } : {}),
      ...(sort ? { sort } : {}),
      ...(sort || !state.sort ? (state.page ?? {}) : {}),
      ...(state.live ? { live: true } : {})
    }
  };
}

/** The read-only WHERE text the applied clauses describe. */
export function whereText(state: ObserveState): string {
  const literal = (value: unknown) => (typeof value === 'string' ? `'${value.replaceAll("'", "''")}'` : String(value));
  return state.where
    .map((clause) => (clause.value === null ? `${clause.column} IS ${clause.not ? 'NOT ' : ''}NULL` : `${clause.column} ${clause.not ? '<>' : '='} ${literal(clause.value)}`))
    .join(' AND ');
}

/** Where a Verify intent leads: one exact execution's page, or the step Observe's grammar gives the same intent. */
export function verifyStep(state: ObserveState, intent: ActionIntent, back: string): HostStep | { readonly open: string } {
  const input = intent.input as Record<string, unknown>;
  if (intent.name === 'record.open') return typeof input.execution_id === 'string' ? { open: input.execution_id } : { unavailable: 'Verify opens executions only' };
  return step(state, intent, back);
}

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
