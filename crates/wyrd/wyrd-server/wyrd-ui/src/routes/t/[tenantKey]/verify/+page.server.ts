import {
  loadExecutionDiscovery,
  loadPanel,
  RangeSchema,
  inputProblem,
  resolveRange,
  validatePanel,
  type ChartProps,
  type ExecutionChartsProps,
  type FacetRailInput,
  type LoadContext,
  type RecordTableProps
} from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { appliedRange, readState } from '$lib/observe-host';
import { readShown, verifyInputs } from '$lib/verify-host';
import type { PageServerLoad } from './$types';

/**
 * The Verifier chart with its series named: each Verifier the server ranked
 * among the largest reads its name, or its name and version when two of them
 * share a name, or its Card UID when the host could not name it; the others
 * read as one `N more` series. `order` is the server's rank order.
 */
function ranked(charts: ExecutionChartsProps, names: Readonly<Record<string, string>>): { readonly chart: ChartProps; readonly order: readonly string[] } {
  const { verifiers: chart, verifierSeries } = charts;
  const read = chart.result;
  if (read.state !== 'populated') return { chart, order: [] };
  const top = verifierSeries.top.map((uid) => names[uid] ?? uid);
  const short = top.map((name) => name.split('@')[0]);
  const series = new Map(verifierSeries.top.map((uid, index) => [uid, short.indexOf(short[index]) === short.lastIndexOf(short[index]) ? short[index] : top[index]]));
  const rest = verifierSeries.more ? `${verifierSeries.more} more` : undefined;
  const verifier = read.data.columns.findIndex((column) => column.name === 'verifier');
  const rows = read.data.rows.map((row) => row.map((value, index) => (index === verifier ? (value === null ? rest : series.get(String(value))) : value)));
  return { chart: { ...chart, result: { ...read, data: { ...read.data, rows } } }, order: [...series.values(), ...(rest ? [rest] : [])] };
}

/**
 * The Verify dashboard as this tenant's browser session: facet counts, one
 * page of executions and the two charts, every one reading the same derived
 * population under the same resolved range and WHERE clauses. The inputs pass
 * their catalog schemas first; a malformed range or refused input issues no
 * read. No execution's status or details is read per row. One bounded Card
 * list names the Verifiers; when it is refused their exact UIDs are shown.
 * Live refreshes by invalidating `wyrd:verify`. The first HTML carries every
 * section; on a client navigation or refresh, the list returns as soon as it
 * is read and the facets, charts and names stream as `support`.
 */
export const load: PageServerLoad = async ({ locals, url, request, depends, isDataRequest }) => {
  depends('wyrd:verify');
  const session = locals.browserSession ?? reject('unauthenticated');
  const state = readState(url.searchParams);
  const applied = RangeSchema.safeParse(appliedRange(state));
  if (!applied.success) return { state, shown: readShown(url.searchParams), range: undefined, verify: { problem: inputProblem('Verify', applied.error.issues[0]) } };
  const range = resolveRange(applied.data);
  const context: LoadContext = { executor: nodeExecutor(session.client()), range, signal: request.signal };
  const shown = readShown(url.searchParams);
  const inputs = verifyInputs(state, shown);
  const rail = validatePanel('FacetRail', inputs.facets);
  if ('code' in rail) return { state, shown, range, verify: { problem: rail } };
  const listed = validatePanel('RecordTable', inputs.list);
  if ('code' in listed) return { state, shown, range, verify: { problem: listed } };
  const checked = rail.input as FacetRailInput;
  const support = Promise.all([
    loadExecutionDiscovery(checked, context),
    // ponytail: one page of 200 Verifier versions is named; past that a row shows its exact UID.
    context.executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], request.signal)
  ]).then(([{ facets, charts }, [verifiers]]) => {
    const labels: Record<string, string> = verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name}@${card.version}`])) : {};
    const verifierChart = ranked(charts, labels);
    return { facets, charts: { verdicts: charts.verdicts, verifiers: verifierChart.chart, verifierSeries: verifierChart.order }, labels };
  });
  const list = (await loadPanel('RecordTable', listed.input, context)) as RecordTableProps;
  return { state, shown, range, verify: { list, support: isDataRequest ? support : await support } };
};
