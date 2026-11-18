<!--
  Verify: every accepted execution in the range as two charts, counted facets
  and one list, composed from the shared components with the props the server
  load returned. Every control navigates, so filters, range, grouping and page
  survive reload, Back and a shared link; Live re-runs the same load. A row
  opens its exact execution page as an ordinary navigation, so Back returns
  here at the same position. The WHERE box is a read-only projection of the
  applied filters.
-->
<script lang="ts" module>
  /** Each address's last settled sections, outliving a mount so Back keeps the page's height. */
  const remembered = new Map<string, unknown>();
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { goto, invalidate } from '$app/navigation';
  import { page } from '$app/state';
  import { Chart, EmptyState, FacetRail, RecordTable, type ActionIntent, type ActionState } from 'wyrd-ai-ui';
  import { NO_VERDICT, validateIntent, type ChartProps, type ComponentName } from 'wyrd-ai-ui/loaders';
  import TimePicker, { type PickedRange, type Zone } from '$lib/TimePicker.svelte';
  import { Streamed } from '$lib/streamed.svelte';
  import { PRESETS, appliedRange, moved, writeState, zoomed, type ObserveState } from '$lib/observe-host';
  import { VERIFY_GROUPS, VERIFY_MOST, VERIFY_PAGE, verifyStep, whereText } from '$lib/verify-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  /** How often Live re-runs the dashboard's load. */
  const LIVE_MS = 10_000;
  /** A verdict's colour; an execution with no recorded verdict is a dashed outline, never a fourth verdict. */
  const VERDICTS = { passed: 'var(--declare)', failed: 'var(--danger)', inconclusive: 'var(--muted)', [NO_VERDICT]: 'transparent' } as const;
  /** The Verifier chart's colours: its three named series in order, then the one that counts the rest. */
  const SERIES = ['var(--declare)', 'var(--observe)', 'var(--warn)', 'var(--muted)'] as const;

  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const current = $derived(data.state);
  const verify = $derived(data.verify);
  /** Facets, charts and Verifier names: in the first HTML, then streamed after the list on each navigation and refresh. */
  type Support = Awaited<Extract<typeof data.verify, { support: unknown }>['support']>;
  const streamed = new Streamed<Support>(remembered as Map<string, Support>);
  $effect.pre(() => streamed.follow('problem' in verify ? undefined : verify.support, untrack(() => page.url.href)));
  const support = $derived('problem' in verify || verify.support instanceof Promise ? streamed.value : verify.support);
  const labels = $derived(support?.labels ?? {});
  const where = $derived(whereText(current));

  /** The URL of `next`, on its first page when it reads another list. */
  const href = (next: ObserveState, shown = data.shown) => {
    const search = writeState(moved(current, next));
    // Another list starts from its first rows; the same list keeps the rows already asked for.
    if (shown > VERIFY_PAGE && moved(current, next) === next) search.set('shown', String(shown));
    return `${base}/verify${search.size ? `?${search}` : ''}`;
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

  // Live waits for each refresh before timing the next, so slow refreshes never pile up.
  $effect(() => {
    if (!current.live) return;
    let timer: ReturnType<typeof setTimeout>;
    let stopped = false;
    const tick = () => (timer = setTimeout(() => void invalidate('wyrd:verify').finally(() => stopped || tick()), LIVE_MS));
    tick();
    return () => ((stopped = true), clearTimeout(timer));
  });

  /** Zoom the applied range around its centre by `factor`: ½ zooms in, 2 out. */
  function zoom(factor: number) {
    if (!data.range) return;
    const [from, to] = [Date.parse(data.range.from), Date.parse(data.range.to)];
    const [centre, half] = [(from + to) / 2, ((to - from) * factor) / 2];
    void go(zoomed(current, centre - half, centre + half));
  }
  function apply(range: PickedRange, tz: Zone) {
    void go({ ...current, tz, ...('last' in range ? { last: range.last, from: undefined, to: undefined } : range) });
  }
  /** The Group by choices as the approved design words them. */
  const GROUP_NAMES = { run_id: 'run', subject_name: 'Card', service_name: 'Service', group_id: 'session' } as const;
  /** The exact Verifier definition a Verifier facet value names, when the host could name it. */
  function definition(column: string, value: unknown) {
    const [name, version] = (column === 'verifier_card_uid' && labels[String(value)]?.split('@')) || [];
    if (!name || !version) return undefined;
    return { href: `${base}/cards/detail?${new URLSearchParams({ kind: 'Verifier', name, version })}`, label: `Open Verifier ${name}@${version}` };
  }
  const zoomedIn = $derived(current.from !== undefined);
  /** Executions under the applied filters and range: the status facet counts every one, in fewer values than a facet reads. */
  const total = $derived.by(() => {
    if (!support || !('data' in support.facets.result)) return undefined;
    const statuses = support.facets.result.data.facets.find((facet) => facet.column === 'status');
    return statuses?.values.rows.reduce((sum, row) => sum + Number(row[1]), 0);
  });
  /** The list's footer: how many of the counted executions it shows, and the next rows when one read can hold them. */
  const more = $derived(total === undefined ? undefined : { total, next: data.shown < Math.min(total, VERIFY_MOST) ? () => void goto(href(current, data.shown + VERIFY_PAGE), { keepFocus: true, noScroll: true }) : undefined });
  /** The Verifier chart's key: each series in its colour; a fourth series only ever counts the rest. */
  const key = (order: readonly string[]) => Object.fromEntries(order.map((name, index) => [name, SERIES[index]]));
  /** How many executions a chart counts, beside its title. */
  function counted(chart: ChartProps) {
    if (chart.result.state !== 'populated') return undefined;
    const at = chart.result.data.columns.findIndex((column) => column.name === 'executions');
    return { value: chart.result.data.rows.reduce((sum, row) => sum + Number(row[at]), 0).toLocaleString('en-US') };
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
    <TimePicker range={appliedRange(current)} zone={current.tz} presets={PRESETS} onapply={apply} />
    <button type="button" class="live" aria-pressed={current.live} onclick={() => go({ ...current, live: !current.live })}>
      <span class="dot" aria-hidden="true"></span>{current.live ? 'Live' : 'Paused'}
    </button>
  </div>

  {#if 'problem' in verify}
    <EmptyState state="error" problem={verify.problem} />
  {:else}
    <section class="panel overview" aria-label="Timelines" data-panel="charts">
      <header>
        <p class="h">Drag across either chart to zoom both</p>
        {#if zoomedIn && data.range}
          <span class="zoomed">{data.range.from.slice(0, 19)}Z → {data.range.to.slice(0, 19)}Z</span>
          <button type="button" class="reset" onclick={() => go({ ...current, from: undefined, to: undefined })}>Reset zoom</button>
        {/if}
      </header>
      <div class="pair">
        {#if support}
          <Chart {...support.charts.verdicts} compact bars colors={VERDICTS} headline={counted(support.charts.verdicts)} {actions} onaction={onaction('Chart')} />
          <Chart {...support.charts.verifiers} compact bars colors={key(support.charts.verifierSeries)} headline={counted(support.charts.verifiers)} {actions} onaction={onaction('Chart')} />
        {:else}
          <EmptyState state={streamed.problem ? 'error' : 'loading'} problem={streamed.problem} title={streamed.problem ? undefined : 'Reading the timelines'} />
        {/if}
      </div>
    </section>
    <div class="listbar">
      <span class="by" role="group" aria-label="Group by">
        <span>Group by</span>
        {#each [undefined, ...VERIFY_GROUPS] as column (column ?? '')}
          <button type="button" class="icon" aria-pressed={current.group === column} onclick={() => go({ ...current, group: column })}>{column ? GROUP_NAMES[column] : 'none'}</button>
        {/each}
      </span>
      {#if total !== undefined}<span class="total">{total.toLocaleString('en-US')} executions</span>{/if}
    </div>
    <div class="split">
      <div data-panel="facets">
        {#if support}<FacetRail {...support.facets} {labels} link={definition} {actions} onaction={onaction('FacetRail')} />{:else}<EmptyState state={streamed.problem ? 'error' : 'loading'} problem={streamed.problem} title={streamed.problem ? undefined : 'Counting the facets'} />{/if}
      </div>
      <div data-panel="list"><RecordTable {...verify.list} {labels} {more} {actions} onaction={onaction('RecordTable')} /></div>
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
  .reset {
    padding: 0;
    border: 0;
    background: none;
    color: var(--declare);
    font: 12px var(--font-mono);
    cursor: pointer;
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
