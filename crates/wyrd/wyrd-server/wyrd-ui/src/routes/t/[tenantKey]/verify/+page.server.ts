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
import { readShown, verifyInputs } from '$lib/verify-host';
import type { PageServerLoad } from './$types';

/** How many Verifiers the Verifier chart names; the rest share one series, so its key stays on one line. */
const NAMED_SERIES = 3;

/**
 * `chart` with its three largest Verifiers as series, largest first, and every
 * other Verifier counted in one `N more` series. A series reads the Verifier's
 * name, or its name and version when two of the three share a name; a Verifier
 * the host could not name reads as its Card UID. `order` is the series order.
 */
function ranked(chart: ChartProps, names: Readonly<Record<string, string>>): { readonly chart: ChartProps; readonly order: readonly string[] } {
  const read = chart.result;
  if (read.state !== 'populated') return { chart, order: [] };
  const [at, verifier, n] = ['at', 'verifier', 'executions'].map((name) => read.data.columns.findIndex((column) => column.name === name));
  const totals = new Map<string, number>();
  for (const row of read.data.rows) totals.set(String(row[verifier]), (totals.get(String(row[verifier])) ?? 0) + Number(row[n]));
  const uids = [...totals.keys()].sort((a, b) => totals.get(b)! - totals.get(a)! || a.localeCompare(b));
  const top = uids.slice(0, NAMED_SERIES).map((uid) => names[uid] ?? uid);
  const short = top.map((name) => name.split('@')[0]);
  const series = new Map(uids.slice(0, NAMED_SERIES).map((uid, index) => [uid, short.indexOf(short[index]) === short.lastIndexOf(short[index]) ? short[index] : top[index]]));
  const rest = uids.length > NAMED_SERIES ? `${uids.length - NAMED_SERIES} more` : undefined;
  const merged = new Map<string, unknown[]>();
  for (const row of read.data.rows) {
    const label = series.get(String(row[verifier])) ?? rest!;
    const key = `${String(row[at])}\u0000${label}`;
    const kept = merged.get(key);
    if (kept) kept[n] = Number(kept[n]) + Number(row[n]);
    else merged.set(key, row.map((value, index) => (index === verifier ? label : index === n ? Number(value) : value)));
  }
  return { chart: { ...chart, result: { ...read, data: { ...read.data, rows: [...merged.values()] } } }, order: [...series.values(), ...(rest ? [rest] : [])] };
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
  const [facets, list, charts, [verifiers]] = await Promise.all([
    loadFacetRail(checked, context),
    loadPanel('RecordTable', listed.input, context) as Promise<RecordTableProps>,
    loadExecutionCharts({ where: checked.where }, context),
    // ponytail: one page of 200 Verifier versions is named; past that a row shows its exact UID.
    context.executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], request.signal)
  ]);
  const labels = verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name}@${card.version}`])) : {};
  const verifierChart = ranked(charts.verifiers, labels);
  return { state, shown, range, verify: { facets, list, charts: { verdicts: charts.verdicts, verifiers: verifierChart.chart, verifierSeries: verifierChart.order }, labels } };
};
