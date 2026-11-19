/**
 * The supporting reads of the Observe and Verify boards: everything beside
 * the list that counts the same population. The first HTML awaits them in the
 * page load; a client navigation or refresh reads them from the page's
 * `support` route instead, which the browser cancels by closing its request.
 */
import type { RequestEvent } from '@sveltejs/kit';
import {
  loadExecutionDiscovery,
  loadFacetRail,
  loadLogTimeline,
  loadMetricNames,
  loadTraceOverview,
  type ChartProps,
  type ExecutionChartsProps,
  type FacetRailInput,
  type LoadContext
} from 'wyrd-ai-ui/loaders';
import type { ObserveState } from '$lib/observe-host';

/** Observe's overview, timeline, facets and metric names for `state`'s signal, under the rail's checked filter. */
export async function observeSupport(state: ObserveState, rail: FacetRailInput, context: LoadContext) {
  const filter = { where: rail.where, where_sql: rail.where_sql };
  const [overview, timeline, facets, metrics] = await Promise.all([
    state.signal === 'traces' ? loadTraceOverview(filter, context) : undefined,
    state.signal === 'logs' ? loadLogTimeline(filter, context) : undefined,
    loadFacetRail(rail, context),
    state.signal === 'metrics' ? loadMetricNames(filter, context) : undefined
  ]);
  return { overview, timeline, facets, metrics };
}

/** Verify's facets and charts over the rail's population, with the Verifiers named. */
export async function verifySupport(rail: FacetRailInput, context: LoadContext) {
  const [{ facets, charts }, [verifiers]] = await Promise.all([
    loadExecutionDiscovery(rail, context),
    // ponytail: one page of 200 Verifier versions is named; past that a row shows its exact UID.
    context.executor.run([{ op: 'cards.list', kind: 'Verifier', limit: 200 }], context.signal)
  ]);
  const labels: Record<string, string> = verifiers.ok ? Object.fromEntries(verifiers.value.items.map((card) => [card.card_uid, `${card.name}@${card.version}`])) : {};
  const verifierChart = ranked(charts, labels);
  return { facets, charts: { verdicts: charts.verdicts, verifiers: verifierChart.chart, verifierSeries: verifierChart.order }, labels };
}

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

/** The connection a request arrived on, as the Node adapter exposes it. */
interface Connection {
  once(event: 'close', listener: () => void): unknown;
  off(event: 'close', listener: () => void): unknown;
}

/**
 * Run `read` with a signal that aborts when the browser closes the request's
 * connection, as it does when it cancels the fetch. The Node adapter aborts
 * `request.signal` only while the request body is still arriving, so a GET
 * the browser abandons would otherwise read to the end. The listener is
 * removed once `read` settles, so a kept-alive connection gathers none.
 */
export async function untilDeparted<T>(event: Pick<RequestEvent, 'request' | 'platform'>, read: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const departed = new AbortController();
  const socket = (event.platform as { req?: { socket?: Connection } } | undefined)?.req?.socket;
  const leave = () => departed.abort(new DOMException('The browser closed the request.', 'AbortError'));
  socket?.once('close', leave);
  try {
    return await read(AbortSignal.any([event.request.signal, departed.signal]));
  } finally {
    socket?.off('close', leave);
  }
}
