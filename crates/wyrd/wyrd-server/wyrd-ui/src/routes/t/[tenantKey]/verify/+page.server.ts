import {
  loadExecutionCharts,
  loadFacetRail,
  loadPanel,
  RangeSchema,
  inputProblem,
  resolveRange,
  validatePanel,
  type ChartProps,
  type FacetRailInput,
  type LoadContext,
  type RecordTableProps
} from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { appliedRange, readState } from '$lib/observe-host';
import { verifyInputs } from '$lib/verify-host';
import type { PageServerLoad } from './$types';

/** `chart` with each Verifier Card UID of its series read as the name the host listed for it. */
function named(chart: ChartProps, labels: Readonly<Record<string, string>>): ChartProps {
  const read = chart.result;
  if (read.state !== 'populated') return chart;
  const at = read.data.columns.findIndex((column) => column.name === 'verifier');
  return { ...chart, result: { ...read, data: { ...read.data, rows: read.data.rows.map((row) => row.map((value, index) => (index === at ? (labels[String(value)] ?? value) : value))) } } };
}

/**
 * The Verify dashboard as this tenant's browser session: facet counts, one
 * page of executions and the two charts, every one reading the same derived
 * population under the same resolved range and WHERE clauses. The inputs pass
 * their catalog schemas first; a malformed range or refused input issues no
 * read. No execution's status or details is read per row. One bounded Card
 * list names the Verifiers; when it is refused their exact UIDs are shown.
 * Live refreshes by invalidating `wyrd:verify`.
 */
export const load: PageServerLoad = async ({ locals, url, request, depends }) => {
  depends('wyrd:verify');
  const session = locals.browserSession ?? reject('unauthenticated');
  const state = readState(url.searchParams);
  const applied = RangeSchema.safeParse(appliedRange(state));
  if (!applied.success) return { state, range: undefined, verify: { problem: inputProblem('Verify', applied.error.issues[0]) } };
  const range = resolveRange(applied.data);
  const context: LoadContext = { executor: nodeExecutor(session.client()), range, signal: request.signal };
  const inputs = verifyInputs(state);
  const rail = validatePanel('FacetRail', inputs.facets);
  if ('code' in rail) return { state, range, verify: { problem: rail } };
  const listed = validatePanel('RecordTable', inputs.list);
  if ('code' in listed) return { state, range, verify: { problem: listed } };
  const checked = rail.input as FacetRailInput;
  const [facets, list, charts, [verifiers]] = await Promise.all([
    loadFacetRail(checked, context),
    loadPanel('RecordTable', listed.input, context) as Promise<RecordTableProps>,
    loadExecutionCharts({ where: checked.where }, context),
    // ponytail: one page of 200 Verifier versions is named; past that a row shows its exact UID.
    context.executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], request.signal)
  ]);
  const labels = verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name} ${card.version}`])) : {};
  return { state, range, verify: { facets, list, charts: { verdicts: charts.verdicts, verifiers: named(charts.verifiers, labels) }, labels } };
};
