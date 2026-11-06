<!--
  Observe: the breadcrumb and signal tabs, the WHERE/zoom/range/Live toolbar,
  and each signal's board composed from the shared components with the props
  the server load returned. Every control navigates, so the investigation
  survives reload, Back and a shared link; Live re-runs the same load.
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { goto, invalidate } from '$app/navigation';
  import { page } from '$app/state';
  import { Chart, Drawer, EmptyState, FacetRail, RecordTable, fmtDuration, fmtNumber, type ActionIntent, type ActionState, type Problem } from 'wyrd-ai-ui';
  import { MAX_GROUP_KEYS, METRIC_AGGREGATIONS, METRIC_GROUP_KEYS, METRIC_NAMES, SIGNALS, columnLabel, validateIntent, type ComponentName } from 'wyrd-ai-ui/loaders';
  import TimePicker, { type PickedRange, type Zone } from '$lib/TimePicker.svelte';
  import { PRESETS, appliedRange, closed, groupColumns, moved, readState, selectedMetric, step, writeState, zoomed, type HostFilter, type ObserveState } from '$lib/observe-host';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
  const LABELS = { traces: 'Traces', logs: 'Logs', metrics: 'Metrics' } as const;
  /** The signal tabs in the boards' order. */
  const TABS = ['logs', 'metrics', 'traces'] as const;
  /** How often Live re-runs the view's load. */
  const LIVE_MS = 10_000;
  const LEVELS = { debug: 'var(--border)', info: 'var(--level-info)', warn: 'var(--warn)', error: 'var(--danger)', unset: 'var(--muted)' } as const;
  /** The approved duration roles: p95 lime, p50 periwinkle. */
  const DURATIONS = { p95: 'var(--observe)', p50: 'var(--declare)' } as const;
  const PERCENTILES = ['p50', 'p90', 'p99'] as const;

  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const current = $derived(readState(page.url.searchParams));
  const observe = $derived(data.observe);
  const view = $derived('problem' in observe ? undefined : observe);
  const form = $derived(page.form as { draft?: string; problem?: Problem } | null);

  /** The URL of `next`, on its first page when it reads another list. */
  const href = (next: ObserveState) => `${base}/observe?${writeState(moved(current, next))}`;
  const go = (next: ObserveState) => goto(href(next), { keepFocus: true, noScroll: true });
  /** `next` on another signal: its own filters and choices start empty; range, zone and Live carry over. */
  const onSignal = (signal: ObserveState['signal']): ObserveState => ({
    ...closed(current),
    signal,
    where: [],
    where_sql: '',
    group: undefined,
    columns: undefined,
    sort: undefined,
    agg: undefined,
    by: undefined,
    apart: undefined,
    points: false
  });

  /** Host-reported action states, by action name. */
  let actions = $state<Record<string, ActionState>>({});
  /** Each component's intents: validated against its catalog entry, then mapped to the next URL. */
  function onaction(component: ComponentName) {
    return (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      const next = step(current, valid, `${page.url.pathname}${page.url.search}`);
      if ('unavailable' in next) return void (actions[valid.name] = { state: 'unavailable', reason: next.unavailable });
      delete actions[valid.name];
      if ('query' in next) return void goto(`${base}/query?${next.query}`);
      void go(next.state);
    };
  }

  $effect(() => {
    if (!current.live) return;
    const timer = setInterval(() => invalidate('wyrd:observe'), LIVE_MS);
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
  const reset = () => go({ ...closed(current), from: undefined, to: undefined });
  function apply(range: PickedRange, tz: Zone) {
    void go({ ...closed(current), tz, ...('last' in range ? { last: range.last, from: undefined, to: undefined } : range) });
  }

  let whereInput = $state<HTMLInputElement>();
  function slash(event: KeyboardEvent) {
    const target = event.target as HTMLElement;
    if (event.key !== '/' || target.closest('input, textarea, select, [contenteditable]')) return;
    event.preventDefault();
    whereInput?.focus();
  }

  /** The current with `column`'s clauses replaced by one `= value`, or removed when `value` is undefined. */
  const only = (column: string, value?: HostFilter['value']): ObserveState => ({
    ...closed(current),
    where: [...current.where.filter((each) => each.column !== column), ...(value === undefined ? [] : [{ column, value }])]
  });

  // Traces.
  let filtersOpen = $state(true);
  /** The list bar's slot for the trace list's Wrap, Columns and CSV. */
  let listTools = $state<HTMLElement>();
  const totals = $derived.by(() => {
    const read = view?.overview?.totals;
    if (read?.state !== 'populated' || !data.range) return undefined;
    const row = read.data.rows[0];
    const at = (name: string) => Number(row[read.data.columns.findIndex((column) => column.name === name)] ?? 0);
    const traces = at('traces');
    return {
      spans: at('spans'),
      traces,
      errorTraces: at('error_traces'),
      errorShare: traces ? (at('error_traces') / traces) * 100 : 0,
      p95: Math.round(at('p95_ms')),
      spanP95: Math.round(at('span_p95_ms')),
      minutes: (Date.parse(data.range.to) - Date.parse(data.range.from)) / 60_000
    };
  });

  // Metrics.
  const metric = $derived(current.signal === 'metrics' ? selectedMetric(current) : undefined);
  let find = $state('');
  /** The names the server found for `find`; absent while the search is blank, so the view's own read shows. */
  let searched = $state<{ for: string; read: NonNullable<typeof view>['metrics'] }>();
  const names = $derived(find.trim() && searched?.for === find.trim() ? searched.read : view?.metrics);
  const metricRows = $derived.by(() => {
    if (names?.state !== 'populated') return [];
    return names.data.rows.slice(0, METRIC_NAMES).map(([name, type, unit]) => ({ name: String(name), type: String(type ?? ''), unit: unit ? String(unit) : '' }));
  });
  const moreNames = $derived(names?.state === 'populated' && names.data.rows.length > METRIC_NAMES);
  // Search the rail on the server a moment after the reader stops typing; a newer search cancels the older one.
  $effect(() => {
    const q = find.trim();
    if (!q || !data.range || current.signal !== 'metrics') return;
    const params = new URLSearchParams(page.url.searchParams);
    params.set('from', data.range.from);
    params.set('to', data.range.to);
    params.set('q', q);
    const abort = new AbortController();
    const timer = setTimeout(async () => {
      const response = await fetch(`${base}/observe/metrics?${params}`, { signal: abort.signal }).catch(() => undefined);
      if (!response) return;
      const body = await response.json().catch(() => undefined);
      searched = { for: q, read: response.ok ? body : { state: 'error', problem: body } };
    }, 200);
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  });
  const metricRow = $derived(metricRows.find((row) => row.name === metric));
  const facetValues = (column: string): readonly unknown[] => {
    const read = view?.facets.result;
    if (read?.state !== 'populated') return [];
    return read.data.facets.find((facet) => facet.column === column)?.values.rows.map(([value]) => value) ?? [];
  };
  const appliedValue = (column: string) => {
    const clause = current.where.find((each) => each.column === column && !each.not);
    return clause ? JSON.stringify(clause.value) : '';
  };
  /** The reader's visible series, per metric view and histogram plot; absent shows the Chart's default. */
  let picks = $state<Record<string, ReadonlySet<string>>>({});
  const seriesKey = $derived(view?.series ? JSON.stringify(view.series.input) : '');
  /** `n` `noun`s in words: "1 trace", "2 traces". */
  const plural = (n: number, noun: string) => `${fmtNumber(n)} ${noun}${n === 1 ? '' : 's'}`;
</script>

<svelte:window onkeydown={slash} />
<svelte:head><title>Observe · {LABELS[current.signal]} · {data.tenant.name} · Wyrd</title></svelte:head>

<header class="top">
  <p class="crumbs"><span>{data.tenant.name}</span> / <span>Observe</span> / <strong>{LABELS[current.signal]}</strong></p>
  <nav class="tabs" aria-label="Signal">
    <span aria-disabled="true" title="Overview is not available yet">Overview</span>
    {#each TABS as signal (signal)}
      <a href={href(onSignal(signal))} aria-current={current.signal === signal ? 'page' : undefined}>{LABELS[signal]}</a>
    {/each}
    <span aria-disabled="true" title="GenAI is not available yet">GenAI</span>
  </nav>
</header>

<div class="board">
  <div class="toolbar">
    <form
      class="where"
      method="POST"
      action="?/where{page.url.search ? `&${page.url.search.slice(1)}` : ''}"
      use:enhance={() =>
        async ({ update }) =>
          update({ reset: false })}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-4-4"></path></svg>
      <label for="where_sql">WHERE</label>
      <input
        bind:this={whereInput}
        id="where_sql"
        name="where_sql"
        value={form?.draft ?? current.where_sql}
        placeholder={current.signal === 'logs' ? "severity_number >= 13 AND body LIKE '%slow%'" : "service_name = 'support-desk'"}
        aria-invalid={form?.problem ? 'true' : undefined}
        aria-describedby={form?.problem ? 'where-problem' : undefined}
        autocomplete="off"
        spellcheck="false"
      />
      <kbd aria-hidden="true">/</kbd>
    </form>
    <span class="group" role="group" aria-label="Zoom">
      <button type="button" class="icon" aria-label="Zoom out" disabled={!data.range} onclick={() => zoom(2)}>−</button>
      <button type="button" class="icon" aria-label="Zoom in" disabled={!data.range} onclick={() => zoom(0.5)}>+</button>
    </span>
    <button type="button" class="icon" aria-label="Reset zoom" disabled={!zoomedIn} onclick={reset}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5"></path></svg>
    </button>
    <TimePicker range={appliedRange(current)} zone={current.tz} presets={PRESETS} onapply={apply} />
    <button type="button" class="live" aria-pressed={current.live} onclick={() => go({ ...closed(current), live: !current.live })}>
      <span class="dot" aria-hidden="true"></span>Live
    </button>
  </div>
  {#if form?.problem}
    <p id="where-problem" class="draft" role="alert">Not applied: {form.problem.detail ?? form.problem.title} <code>{form.problem.code}</code></p>
  {/if}
  {#if current.where.length}
    <ul class="chips" aria-label="Applied filters">
      {#each current.where as clause, index (index)}
        <li>
          <code>{clause.column} {clause.not ? '≠' : '='} {JSON.stringify(clause.value)}</code>
          <a aria-label="Remove {clause.column}" href={href({ ...closed(current), where: current.where.filter((_, at) => at !== index) })}>✕</a>
        </li>
      {/each}
    </ul>
  {/if}

  {#if 'problem' in observe}
    <EmptyState state="error" problem={observe.problem} />
  {:else if current.signal === 'traces'}
    {#if observe.overview}
      {@const overview = observe.overview}
      <section class="panel overview" aria-label="Overview" data-panel="overview">
        <header>
          <p class="h">Drag across any chart to zoom all three</p>
          {#if zoomedIn && data.range}
            <button type="button" class="badge" onclick={reset} aria-label="Reset zoom">Zoomed {data.range.from.slice(11, 19)} – {data.range.to.slice(11, 19)} ✕</button>
          {/if}
        </header>
        <div class="trio">
          <Chart
            {...overview.spans}
            input={{ ...overview.spans.input, title: 'Spans / min' }}
            compact
            bars
            colors={{ spans: 'var(--series-3)' }}
            headline={totals ? { value: fmtNumber(totals.spans / totals.minutes), detail: plural(totals.traces, 'trace') } : undefined}
            {actions}
            onaction={onaction('Chart')}
          />
          <Chart
            {...overview.errors}
            compact
            bars
            colors={{ errors: 'var(--danger)' }}
            headline={totals ? { value: fmtNumber(totals.errorTraces), detail: `${fmtNumber(totals.errorShare)}% of traces`, tone: 'var(--danger)' } : undefined}
            {actions}
            onaction={onaction('Chart')}
          />
          <Chart {...overview.duration} compact colors={DURATIONS} headline={totals ? { value: `p95 ${fmtDuration(totals.p95)}` } : undefined} {actions} onaction={onaction('Chart')} />
        </div>
      </section>
    {/if}
    <div class="listbar">
      <span class="segmented" role="group" aria-label="Rows">
        <a href={href({ ...closed(current), mode: 'traces', group: undefined, columns: undefined })} aria-current={current.mode === 'traces' ? 'true' : undefined}>Traces</a>
        <a href={href({ ...closed(current), mode: 'spans', group: undefined, columns: undefined })} aria-current={current.mode === 'spans' ? 'true' : undefined}>Spans</a>
      </span>
      <button type="button" class="filters" aria-expanded={filtersOpen} aria-controls="facets" onclick={() => (filtersOpen = !filtersOpen)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 5h18l-7 8v6l-4 2v-8z"></path></svg>
        Filters{current.where.length ? ` · ${current.where.length}` : ''}
      </button>
      {#if totals && current.mode === 'spans'}
        <span class="counts">{plural(totals.spans, 'span')} match · p95 {fmtDuration(totals.spanP95)}</span>
      {:else if totals}
        <span class="counts">{plural(totals.traces, 'trace')} · <span class:bad={totals.errorShare > 0}>{fmtNumber(totals.errorShare)}% error</span> · p95 {fmtDuration(totals.p95)}</span>
      {/if}
      {@render grouping()}
      <span class="listtools" bind:this={listTools}></span>
    </div>
    <div class="split" class:folded={!filtersOpen}>
      {#if filtersOpen}<div id="facets" data-panel="facets"><FacetRail {...observe.facets} {actions} onaction={onaction('FacetRail')} /></div>{/if}
      <div data-panel="list">
        {#if observe.list}<RecordTable {...observe.list} {actions} onaction={onaction('RecordTable')} oncolumns={(columns) => go({ ...closed(current), columns })} controls={listTools} />{/if}
      </div>
    </div>
    {@render children()}
  {:else if current.signal === 'logs'}
    {#if observe.timeline}
      {@const timeline = observe.timeline}
      <section class="panel overview" aria-label="Timeline" data-panel="timeline">
        <Chart {...timeline} input={{ ...timeline.input, title: `${timeline.input.title} · drag to zoom` }} compact bars colors={LEVELS} {actions} onaction={onaction('Chart')} />
      </section>
    {/if}
    <!-- The records work region: fields, records and the selected record side by side, wrapping on a narrow screen. -->
    <div class="work">
      <div data-panel="facets"><FacetRail {...observe.facets} {actions} onaction={onaction('FacetRail')} /></div>
      <div data-panel="list">
        {#if observe.list}<RecordTable {...observe.list} {actions} onaction={onaction('RecordTable')} oncolumns={(columns) => go({ ...closed(current), columns })} tools={grouping} />{/if}
      </div>
      {@render children()}
    </div>
  {:else}
    <div class="metrics">
      <aside class="panel rail" aria-label="Metrics" data-panel="metrics">
        <input class="find" type="search" bind:value={find} placeholder="find a metric…" aria-label="Find a metric" />
        {#each [['service_name', 'Service'], ['subject_card_uid', 'Card']] as const as [column, label] (column)}
          <label class="h">
            {label}
            <select value={appliedValue(column)} onchange={(event) => go(only(column, event.currentTarget.value ? JSON.parse(event.currentTarget.value) : undefined))}>
              <option value="">any</option>
              {#each facetValues(column) as value (JSON.stringify(value))}
                <option value={JSON.stringify(value)}>{value === null ? 'not set' : String(value)}</option>
              {/each}
            </select>
          </label>
        {/each}
        {#if names && names.state !== 'populated'}
          <EmptyState state={names.state} problem={'problem' in names ? names.problem : undefined} detail={'reason' in names ? names.reason : undefined} />
        {:else}
          <ul aria-label="Metric names">
            {#each metricRows as row (`${row.name} ${row.type} ${row.unit}`)}
              <li>
                <a href={href({ ...only('metric_name', row.name), agg: undefined, by: undefined, apart: undefined })} aria-current={row.name === metric ? 'true' : undefined}>
                  <span>{row.name}</span><span class="type">{row.type}{row.unit ? ` · ${row.unit}` : ''}</span>
                </a>
              </li>
            {/each}
          </ul>
          {#if moreNames}<p class="more">More than {METRIC_NAMES} match — type to narrow</p>{/if}
        {/if}
        <a class="points" href={href({ ...closed(current), points: true })} aria-current={current.points ? 'true' : undefined}>Recorded points</a>
      </aside>
      <div class="main">
        {#if view?.series?.histogram && view.series.chart.result.state === 'empty'}
          {@const histogram = view.series.histogram}
          <div data-panel="series">
            <Chart
              {...histogram.latency}
              shown={picks[`latency ${seriesKey}`]}
              onseries={(shown) => (picks[`latency ${seriesKey}`] = shown)}
              note="Percentiles are estimated from the recorded buckets; ≥ marks a percentile in the unbounded top bucket, at least that bucket's lower bound. Drag across the chart to zoom every Observe view to that window."
              {actions}
              onaction={onaction('Chart')}
            >
              {#snippet heading()}
                <div class="metric">
                  <h2>{metric}</h2>
                  <p class="type">{metricRow ? `${metricRow.type}${metricRow.unit ? ` · ${metricRow.unit}` : ''} · ` : ''}{histogram.latency.input.title?.replace(`${metric} · `, '')}</p>
                </div>
                <span class="choices">
                  <span class="segmented" role="group" aria-label="Calculation">
                    <a href={href({ ...closed(current), agg: undefined, apart: undefined })} aria-current={!current.apart ? 'true' : undefined} title="Compatible series merge their buckets">default</a>
                    <a href={href({ ...closed(current), agg: undefined, by: undefined, apart: true })} aria-current={current.apart ? 'true' : undefined} title="The 20 series that counted most, each apart">every series</a>
                  </span>
                  {#if !current.apart}{@render keys()}{/if}
                </span>
              {/snippet}
            </Chart>
          </div>
          <div class="histogram">
            <div data-panel="rate"><Chart {...histogram.rate} shown={picks[`rate ${seriesKey}`]} onseries={(shown) => (picks[`rate ${seriesKey}`] = shown)} {actions} onaction={onaction('Chart')} /></div>
            <div data-panel="distribution"><Chart {...histogram.distribution} {actions} onaction={onaction('Chart')} /></div>
          </div>
        {:else if view?.series}
          {@const series = view.series}
          <div data-panel="series">
            <Chart
              {...series.chart}
              identity={series.identity}
              summary={series.summary}
              note="Drag across the chart to zoom every Observe view to that window."
              shown={picks[seriesKey]}
              onseries={(shown) => (picks[seriesKey] = shown)}
              {actions}
              onaction={onaction('Chart')}
            >
              {#snippet heading()}
                <div class="metric">
                  <h2>{metric}</h2>
                  <!-- The chart title's calculation and precision caveat stay visible under the metric name. -->
                  <p class="type">{metricRow ? `${metricRow.type}${metricRow.unit ? ` · ${metricRow.unit}` : ''} · ` : ''}{series.chart.input.title?.replace(`${metric} · `, '')}</p>
                </div>
                <span class="choices">
                  <span class="segmented" role="group" aria-label="Calculation">
                    <a href={href({ ...closed(current), agg: undefined, apart: undefined })} aria-current={!current.agg && !current.apart ? 'true' : undefined} title="Sums add up and gauges average across series">default</a>
                    {#each METRIC_AGGREGATIONS as aggregation (aggregation)}
                      <a href={href({ ...closed(current), agg: aggregation, apart: undefined })} aria-current={current.agg === aggregation && !current.apart ? 'true' : undefined}>{aggregation}</a>
                    {/each}
                    <a href={href({ ...closed(current), agg: undefined, by: undefined, apart: true })} aria-current={current.apart ? 'true' : undefined} title="The 20 largest series, each apart">every series</a>
                    {#each PERCENTILES as percentile (percentile)}
                      <span aria-disabled="true" title="Percentiles of recorded points are not available">{percentile}</span>
                    {/each}
                  </span>
                  {#if !current.apart}{@render keys()}{/if}
                </span>
              {/snippet}
            </Chart>
          </div>
        {:else}
          <EmptyState state="empty" title="Choose a metric" detail="Pick a metric on the left to plot its recorded series." />
        {/if}
      </div>
    </div>
    <!-- Recorded points open only on request: the same bounded list read, under the same range and filters. -->
    {#if current.points && observe.list}
      <Drawer open title="Recorded points" width="884px" onclose={() => go({ ...closed(current), points: false })}>
        <div data-panel="list"><RecordTable {...observe.list} {actions} onaction={onaction('RecordTable')} oncolumns={(columns) => go({ ...closed(current), columns })} /></div>
      </Drawer>
    {/if}
  {/if}
</div>

{#snippet keys()}
  <!-- Up to three group keys: a promoted column, or one attributes., resource. or scope. key read on its own; the list offers the keys the metric's series record. -->
  <span class="by keys" role="group" aria-label="Group series by">
    by
    {#each current.by ?? [] as key (key)}
      <button type="button" class="key" aria-label="Stop grouping by {key}" onclick={() => go({ ...closed(current), by: current.by?.filter((each) => each !== key) })}>{key} ✕</button>
    {:else}
      <span class="quiet">all series</span>
    {/each}
    {#if (current.by?.length ?? 0) < MAX_GROUP_KEYS}
      <form
        onsubmit={(event) => {
          event.preventDefault();
          const key = new FormData(event.currentTarget).get('key')?.toString().trim();
          if (key && !current.by?.includes(key)) go({ ...closed(current), by: [...(current.by ?? []), key] });
        }}
      >
        <input name="key" list="group-keys" placeholder="attributes.http.route" aria-label="Add a group key" />
      </form>
      <datalist id="group-keys">{#each view?.series?.keys ?? METRIC_GROUP_KEYS as key (key)}<option value={key}></option>{/each}</datalist>
    {/if}
  </span>
{/snippet}

{#snippet grouping()}
  <label class="by">
    Group by
    <select aria-label="Group by" value={current.group ?? ''} onchange={(event) => go({ ...closed(current), group: event.currentTarget.value || undefined })}>
      <option value="">none</option>
      {#each groupColumns(current) as column (column)}<option value={column}>{columnLabel(column)}</option>{/each}
    </select>
  </label>
{/snippet}

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
  .tabs {
    display: flex;
    align-self: stretch;
    max-width: 100%;
    overflow-x: auto;
  }
  .tabs a,
  .tabs span {
    display: flex;
    align-items: center;
    padding: 0 14px;
    color: var(--muted);
    font: 13px var(--font-mono);
    text-decoration: none;
    white-space: nowrap;
  }
  .tabs a[aria-current='page'] {
    box-shadow: inset 0 -2px 0 var(--declare);
    color: var(--text);
  }
  [aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.5;
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
  .where input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: 12px var(--font-mono);
    outline: none;
  }
  .where:focus-within {
    box-shadow: 0 0 0 2px var(--declare-soft);
  }
  kbd {
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    font: 11px var(--font-mono);
  }
  .group {
    display: inline-flex;
  }
  .group .icon + .icon {
    margin-left: -1px;
  }
  .icon,
  .live,
  .filters,
  .badge {
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
  .draft {
    margin: -8px 0 0;
    color: var(--danger);
    font: 12px var(--font-sans);
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
  .badge {
    height: 22px;
    border-color: var(--selected-border);
    background: var(--declare-soft);
    font: 11px var(--font-mono);
  }
  .trio {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 16px;
  }
  /* The Logs board's severity timeline is one 64px band; the Traces trio stays 56px. */
  [data-panel='timeline'] :global(.chart.compact .box) {
    height: 64px;
    min-height: 64px;
  }
  .listbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    font: 13px var(--font-mono);
  }
  .segmented {
    display: inline-flex;
    flex-wrap: wrap;
    max-width: 100%;
  }
  .segmented a,
  .segmented span {
    display: inline-flex;
    align-items: center;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    color: var(--muted);
    font: 12px var(--font-mono);
    text-decoration: none;
    white-space: nowrap;
  }
  .segmented > * + * {
    margin-left: -1px;
  }
  .segmented [aria-current='true'] {
    position: relative;
    border-color: var(--selected-border);
    background: var(--declare-soft);
    color: var(--text);
  }
  .filters[aria-expanded='true'] {
    border-color: var(--selected-border);
    background: var(--declare-soft);
  }
  .counts {
    color: var(--muted);
  }
  .listtools {
    display: contents;
  }
  .bad {
    color: var(--danger);
  }
  .by {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  /* A choice is as wide as a short column name, so the Logs header keeps one row beside the open record; a long attribute name truncates in the closed control. */
  .by select {
    max-width: 7em;
  }
  .keys {
    flex-wrap: wrap;
  }
  .keys .key {
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--declare);
    border-radius: var(--r);
    background: var(--declare-soft);
    color: var(--text);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  .keys input {
    width: 16em;
    height: 26px;
    box-sizing: border-box;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .quiet {
    color: var(--muted);
  }
  select {
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .split {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  .split.folded {
    grid-template-columns: minmax(0, 1fr);
  }
  .work {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 16px;
  }
  .work > [data-panel='facets'] {
    flex: 0 0 220px;
  }
  .work > [data-panel='list'] {
    flex: 999 1 560px;
    min-width: 0;
  }
  /* A log record dragged wider takes the list's room down to 320px, rather than wrapping below it. */
  .work:has(> :global(.record.sized)) > [data-panel='list'] {
    flex-basis: 320px;
  }
  .metrics {
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px;
  }
  .rail label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .find {
    height: 30px;
    box-sizing: border-box;
    padding: 0 8px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .rail ul {
    margin: 0 -10px;
    padding: 0;
    border-top: 1px solid var(--border);
    list-style: none;
  }
  .rail li a {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 2px 8px;
    overflow-wrap: anywhere;
    padding: 8px 12px;
    color: var(--text);
    font: 12px var(--font-mono);
    text-decoration: none;
  }
  .rail li a:hover {
    background: var(--surface-hover);
  }
  .rail li a[aria-current='true'] {
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
  }
  .more {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .rail .type {
    margin-left: auto;
    white-space: nowrap;
  }
  .type {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .histogram {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  /* The metric name gives way first, so the calculation and by choices share its header row. */
  .metric {
    flex: 1 1 0;
    min-width: 160px;
  }
  .metric h2 {
    margin: 0;
    font: 14px var(--font-mono);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    min-width: 0;
  }
  .choices .by {
    margin-left: 0;
  }
  .choices .segmented a,
  .choices .segmented span {
    height: 28px;
  }
  .points {
    display: block;
    margin: 0 -10px -10px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    color: var(--declare);
    font: 12px var(--font-mono);
    text-decoration: none;
  }
  @media (max-width: 899px) {
    .histogram {
      grid-template-columns: minmax(0, 1fr);
    }
    .split,
    .metrics {
      grid-template-columns: minmax(0, 1fr);
    }
    .work > [data-panel='facets'] {
      flex-basis: 100%;
    }
  }
  @media (max-width: 767px) {
    .top,
    .board {
      padding-inline: 16px;
    }
  }
</style>
