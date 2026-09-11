<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ChartPanel from '$lib/components/charts/ChartPanel.svelte';
  import Line from '$lib/components/charts/Line.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES, scopeQuery } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/metrics$/, ''));
  const serviceOptions = $derived([
    { value: '', label: 'all services' },
    ...(data.view?.services ?? []).map((s: string) => ({ value: s, label: s }))
  ]);
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  function metricHref(name: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('metric', name);
    return `?${query}`;
  }
</script>

<svelte:head><title>Metrics · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Metrics</h1>
  <p class="muted">Find a metric, filter its labels, then read the series and its values.</p>
  <ObserveNav base={`${base}/observe`} current="Metrics" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
      <input type="hidden" name="metric" value={data.filters.metric} />
      <Select label="service" name="service" options={serviceOptions} value={data.filters.service} />
      <Select label="range" name="range" options={rangeOptions} value={data.filters.range} />
      <a class="mono muted reset" href={page.url.pathname}>Reset</a>
    </form>
    {#if data.problem}
      <StateBlock
        state={data.problem.status === 403 ? 'unauthorized' : 'error'}
        title={data.problem.title}
        code={data.problem.code}
        actionLabel="Retry"
        actionHref={page.url.pathname + page.url.search}
      />
    {:else if data.view}
      {#if !data.view.metrics.length}
        <StateBlock state="empty" title="No metrics in this range" detail="No series were reported for this scope." />
      {:else}
        <div class="metrics-columns">
          <div class="stack">
            <Panel title="Metric discovery">
              {#snippet head()}<span class="mono muted">{data.view.metrics.length}</span>{/snippet}
              <ul class="metric-list">
                {#each data.view.metrics as metric (metric.name)}
                  <li>
                    <a
                      class="mono"
                      href={metricHref(metric.name)} data-sveltekit-noscroll
                      aria-current={data.view.selected?.name === metric.name ? 'true' : undefined}
                      >{metric.name}</a
                    >
                  </li>
                {/each}
              </ul>
              {#if data.view.selected}
                <div class="divider">
                  <h3 class="mono labels-title">Labels — {data.view.selected.name}</h3>
                  <div class="kv">
                    {#each data.view.selected.labels as [key, value] (key)}
                      <span class="k">{key}</span><span class="mono">{value}</span>
                    {/each}
                  </div>
                </div>
              {/if}
            </Panel>
            <Panel title="Correlate">
              {#snippet head()}<span class="mono muted">same range</span>{/snippet}
              <div class="stack correlate">
                <a class="mono" href={`${base}/observe/logs${scopeQuery(data.scope, { level: 'error' })}`}>Logs · level=error · {data.scope.range}</a>
                <a class="mono" href={`${base}/observe/traces${scopeQuery(data.scope, { status: 'error' })}`}>Traces · status=error · {data.scope.range}</a>
                {#if data.view.cardHref}
                  <a class="mono" href={`${base}/cards/${data.view.cardHref}`}>{data.filters.service} Card</a>
                {/if}
              </div>
              <p class="muted">Correlation context is injected by the instrumented service, never the browser.</p>
            </Panel>
          </div>
          <div class="stack">
            {#if data.view.selected}
              <ChartPanel
                title={data.view.selected.name}
                measure={data.view.selected.description}
                unit={data.view.selected.unit}
                state={data.view.series.length ? 'ok' : 'empty'}
                detail={data.view.series.length ? undefined : 'A series with no samples draws empty with its label, never omitted.'}
                source="vala.system.metrics"
                from={data.view.labels[0]}
                to={data.view.labels.at(-1)}
              >
                {#if data.view.series.length}
                  <Line series={data.view.series} labels={data.view.labels} unit={data.view.selected.unit} />
                {/if}
              </ChartPanel>
              {#if data.view.values.rows.length}
                <Panel title="Values">
                  {#snippet head()}<span class="mono muted">underlying · last {data.view.values.rows.length} buckets</span>{/snippet}
                  <Table label="Underlying metric values">
                    <table>
                      <thead><tr>{#each data.view.values.columns as column (column)}<th>{column}</th>{/each}</tr></thead>
                      <tbody>
                        {#each data.view.values.rows as row, i (i)}
                          <tr>{#each row as cell, j (j)}<td class="mono">{cell}</td>{/each}</tr>
                        {/each}
                      </tbody>
                    </table>
                  </Table>
                </Panel>
              {/if}
            {/if}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .metrics-columns {
    display: grid;
    grid-template-columns: minmax(230px, 1fr) minmax(0, 2.4fr);
    gap: 20px;
    align-items: start;
  }
  .metric-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
  }
  .metric-list a {
    display: block;
    padding: 7px 9px;
    border-left: 4px solid transparent;
    color: var(--text);
    text-decoration: none;
    font-size: 11px;
  }
  .metric-list a:hover {
    background: var(--surface-2);
  }
  .metric-list a[aria-current='true'] {
    background: var(--brand-soft);
    border-left-color: var(--brand-strong);
    font-weight: 700;
  }
  .labels-title {
    font: 700 10px var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .correlate {
    gap: 6px;
  }
  @media (max-width: 1000px) {
    .metrics-columns {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
