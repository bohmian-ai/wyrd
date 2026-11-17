<!--
  Verify: every accepted execution in the range as two charts, counted facets
  and one list, composed from the shared components with the props the server
  load returned. Every control navigates, so filters, range, grouping and page
  survive reload, Back and a shared link; Live re-runs the same load. A row
  opens its exact execution page as an ordinary navigation, so Back returns
  here at the same position. The WHERE box is a read-only projection of the
  applied filters.
-->
<script lang="ts">
  import { goto, invalidate } from '$app/navigation';
  import { Chart, EmptyState, FacetRail, RecordTable, type ActionIntent, type ActionState } from 'wyrd-ai-ui';
  import { NO_VERDICT, columnLabel, validateIntent, type ChartProps, type ComponentName } from 'wyrd-ai-ui/loaders';
  import TimePicker, { type PickedRange, type Zone } from '$lib/TimePicker.svelte';
  import { PRESETS, appliedRange, moved, writeState, zoomed, type ObserveState } from '$lib/observe-host';
  import { VERIFY_GROUPS, verifyStep, whereText } from '$lib/verify-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  /** How often Live re-runs the dashboard's load. */
  const LIVE_MS = 10_000;
  /** A verdict's colour; an execution with no recorded verdict is neutral, never a fourth verdict. */
  const VERDICTS = { passed: 'var(--ok)', failed: 'var(--danger)', inconclusive: 'var(--warn)', [NO_VERDICT]: 'var(--muted)' } as const;
  /** The series colours, in the shared chart's order. */
  const SERIES = ['var(--declare)', 'var(--observe)', 'var(--series-3)', 'var(--series-4)', 'var(--series-5)'] as const;

  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const current = $derived(data.state);
  const verify = $derived(data.verify);
  const where = $derived(whereText(current));

  /** The URL of `next`, on its first page when it reads another list. */
  const href = (next: ObserveState) => {
    const search = writeState(moved(current, next)).toString();
    return `${base}/verify${search ? `?${search}` : ''}`;
  };
  const go = (next: ObserveState) => goto(href(next), { keepFocus: true, noScroll: true });

  /** Host-reported action states, by action name. */
  let actions = $state<Record<string, ActionState>>({});
  /** Each component's intents: validated against its catalog entry, then mapped to the next URL. */
  function onaction(component: ComponentName) {
    return (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      const next = verifyStep(current, valid, href(current));
      if ('unavailable' in next) return void (actions[valid.name] = { state: 'unavailable', reason: next.unavailable });
      delete actions[valid.name];
      if ('open' in next) return void goto(`${base}/verify/executions/${encodeURIComponent(next.open)}`);
      if ('query' in next) return void goto(`${base}/query?${next.query}`);
      void go(next.state);
    };
  }

  $effect(() => {
    if (!current.live) return;
    const timer = setInterval(() => invalidate('wyrd:verify'), LIVE_MS);
    return () => clearInterval(timer);
  });

  /** Zoom the applied range around its centre by `factor`: ½ zooms in, 2 out. */
  function zoom(factor: number) {
    if (!data.range) return;
    const [from, to] = [Date.parse(data.range.from), Date.parse(data.range.to)];
    const [centre, half] = [(from + to) / 2, ((to - from) * factor) / 2];
    void go(zoomed(current, centre - half, centre + half));
  }
  const zoomedIn = $derived(current.from !== undefined);
  /** Executions under the applied filters and range: the status facet counts every one, in fewer values than a facet reads. */
  const total = $derived.by(() => {
    if ('problem' in verify || !('data' in verify.facets.result)) return undefined;
    const statuses = verify.facets.result.data.facets.find((facet) => facet.column === 'status');
    return statuses?.values.rows.reduce((sum, row) => sum + Number(row[1]), 0);
  });
  function apply(range: PickedRange, tz: Zone) {
    void go({ ...current, tz, ...('last' in range ? { last: range.last, from: undefined, to: undefined } : range) });
  }

  /** A short key for the Verifier chart: its first series, coloured as the chart draws them. */
  function key(chart: ChartProps): Record<string, string> {
    if (chart.result.state !== 'populated') return {};
    const at = chart.result.data.columns.findIndex((column) => column.name === 'verifier');
    const names = [...new Set(chart.result.data.rows.map((row) => String(row[at])))].slice(0, SERIES.length);
    return Object.fromEntries(names.map((name, index) => [name, SERIES[index]]));
  }
</script>

<svelte:head><title>Verify · {data.tenant.name} · Wyrd</title></svelte:head>

<header class="top">
  <p class="crumbs"><span>{data.tenant.name}</span> / <strong>Verify</strong></p>
</header>

<div class="board">
  <div class="toolbar">
    <div class="where" title="Read-only: the WHERE clause the selected filters describe">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-4-4"></path></svg>
      <span>WHERE</span>
      <output aria-label="WHERE: read-only projection of the active filters">{#if where}{where}{:else}<span class="hint">select facet values to filter</span>{/if}</output>
      {#if current.where.length}<a href={href({ ...current, where: [] })}>Clear</a>{/if}
    </div>
    <span class="group" role="group" aria-label="Zoom">
      <button type="button" class="icon" aria-label="Zoom out" disabled={!data.range} onclick={() => zoom(2)}>−</button>
      <button type="button" class="icon" aria-label="Zoom in" disabled={!data.range} onclick={() => zoom(0.5)}>+</button>
    </span>
    <button type="button" class="icon" aria-label="Reset zoom" disabled={!zoomedIn} onclick={() => go({ ...current, from: undefined, to: undefined })}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5"></path></svg>
    </button>
    <TimePicker range={appliedRange(current)} zone={current.tz} presets={PRESETS} onapply={apply} />
    <button type="button" class="live" aria-pressed={current.live} onclick={() => go({ ...current, live: !current.live })}>
      <span class="dot" aria-hidden="true"></span>Live
    </button>
  </div>
  {#if current.where.length}
    <ul class="chips" aria-label="Applied filters">
      {#each current.where as clause, index (index)}
        <li>
          <code>{clause.column} {clause.not ? '≠' : '='} {JSON.stringify(clause.value)}</code>
          <a aria-label="Remove {clause.column}" href={href({ ...current, where: current.where.filter((_, at) => at !== index) })}>✕</a>
        </li>
      {/each}
    </ul>
  {/if}

  {#if 'problem' in verify}
    <EmptyState state="error" problem={verify.problem} />
  {:else}
    <section class="panel overview" aria-label="Timelines" data-panel="charts">
      <header>
        <p class="h">Drag across either chart to zoom both</p>
        {#if zoomedIn && data.range}<span class="zoomed">{data.range.from.slice(0, 19)}Z → {data.range.to.slice(0, 19)}Z</span>{/if}
      </header>
      <div class="pair">
        <Chart {...verify.charts.verdicts} compact bars colors={VERDICTS} {actions} onaction={onaction('Chart')} />
        <Chart {...verify.charts.verifiers} compact bars colors={key(verify.charts.verifiers)} {actions} onaction={onaction('Chart')} />
      </div>
    </section>
    <div class="listbar">
      <span class="by" role="group" aria-label="Group by">
        <span>Group by</span>
        {#each [undefined, ...VERIFY_GROUPS] as column (column ?? '')}
          <button type="button" class="icon" aria-pressed={current.group === column} onclick={() => go({ ...current, group: column })}>{column ? columnLabel(column) : 'none'}</button>
        {/each}
      </span>
      {#if total !== undefined}<span class="total">{total.toLocaleString('en-US')} executions</span>{/if}
    </div>
    <div class="split">
      <div data-panel="facets"><FacetRail {...verify.facets} labels={verify.labels} {actions} onaction={onaction('FacetRail')} /></div>
      <div data-panel="list"><RecordTable {...verify.list} labels={verify.labels} {actions} onaction={onaction('RecordTable')} /></div>
    </div>
  {/if}
</div>

<style>
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 48px;
    min-height: 58px;
    box-sizing: border-box;
    padding: 0 24px;
    border-bottom: 1px solid var(--border);
  }
  .crumbs {
    margin: 0;
    color: var(--muted);
    font: 13px var(--font-mono);
  }
  .crumbs strong {
    color: var(--text);
    font-weight: var(--weight-strong);
  }
  .board {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px 24px 24px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .where {
    display: flex;
    flex: 1 1 360px;
    align-items: center;
    gap: 10px;
    height: 34px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .where:focus-within {
    box-shadow: 0 0 0 2px var(--declare-soft);
  }
  .group {
    display: inline-flex;
  }
  .group .icon + .icon {
    margin-left: -1px;
  }
  .icon,
  .live {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 34px;
    height: 34px;
    box-sizing: border-box;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 13px var(--font-sans);
    text-decoration: none;
    cursor: pointer;
  }
  .icon:disabled {
    color: var(--muted);
    cursor: not-allowed;
  }
  .dot {
    width: 8px;
    height: 8px;
    border: 1px solid var(--muted);
    border-radius: 50%;
  }
  .live[aria-pressed='true'] .dot {
    border-color: var(--ok);
    background: var(--ok);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: -8px 0 0;
    padding: 0;
    list-style: none;
  }
  .chips li {
    display: flex;
    gap: 6px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    font: 12px var(--font-mono);
  }
  .chips a {
    color: var(--muted);
    text-decoration: none;
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .overview {
    padding: 12px 14px;
  }
  .overview header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px;
  }
  .h {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .listbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    font: 13px var(--font-mono);
  }
  .by {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .by .icon {
    height: 28px;
    margin-left: -1px;
    font: 12px var(--font-mono);
  }
  .by .icon[aria-pressed='true'] {
    border-color: var(--selected-border);
    background: var(--selected);
  }
  .total {
    margin-left: auto;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .where output {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint,
  .zoomed {
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .where a {
    color: var(--muted);
  }
  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 16px;
  }
  .split {
    display: grid;
    grid-template-columns: minmax(180px, 220px) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  /* Eight columns: tighter cells fit a wide screen; narrower, the table scrolls sideways. */
  [data-panel='list'] :global(:is(th, td)) {
    padding-inline: 8px;
  }
  /* Status and Verdict, the last two columns, stay in view while the rest scrolls sideways. */
  [data-panel='list'] :global(:is(th, td):nth-last-child(-n + 2)) {
    position: sticky;
    right: 0;
    background: var(--surface);
  }
  [data-panel='list'] :global(:is(th, td):last-child) {
    box-sizing: border-box;
    width: 116px;
    min-width: 116px;
  }
  [data-panel='list'] :global(:is(th, td):nth-last-child(2)) {
    right: 116px;
  }
  [data-panel='list'] :global(tr.group th) {
    position: static;
    width: auto;
  }
  @media (max-width: 720px) {
    .top,
    .board {
      padding-inline: 16px;
    }
    .split {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
