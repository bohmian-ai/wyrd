<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import FieldsPanel from '$lib/features/observe/FieldsPanel.svelte';
  import TrendChart from '$lib/features/observe/TrendChart.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/traces$/, ''));
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  // A filter change discards any accumulated load-more depth — the old limit
  // belongs to a different matching set.
  function facetHref(field: string, value: string, active: boolean): string {
    const query = new URLSearchParams(page.url.search);
    if (active) query.delete(field);
    else query.set(field, value);
    query.delete('limit');
    return `?${query}`;
  }
  /** Appends the next load-more step to the current search. */
  const loadMoreHref = $derived.by(() => {
    const query = new URLSearchParams(page.url.search);
    query.set('limit', `${(data.view?.limit ?? 50) + 50}`);
    return `?${query}`;
  });
  /** A row opens its trace directly, carrying the whole search as return context. */
  function rowHref(id: string): string {
    return `${page.url.pathname}/${id}${page.url.search}`;
  }
  const maxDuration = $derived(Math.max(...(data.view?.rows.map((row) => row.durationMs) ?? [1])));
  /** Duration buckets label in seconds once they cross 1000ms. */
  const fmtMs = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(2)}s` : `${Math.round(n)}`);
</script>

{#snippet strip(title: string, unit: string, buckets: number[], tone: 'ok' | 'danger', fmt?: (n: number) => string)}
  <Panel {title}>
    {#snippet head()}<span class="mono muted">{unit}</span>{/snippet}
    <TrendChart {buckets} range={data.filters.range} end={data.view?.trendEnd ?? 0} {unit} {tone} label={title} {fmt} />
  </Panel>
{/snippet}

<svelte:head><title>Traces · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Traces</h1>
  <p class="muted">Search by service, status, duration and attributes — then open one.</p>
  <ObserveNav base={`${base}/observe`} current="Traces" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
      <input aria-label="Search traces" name="q" value={data.filters.q} placeholder="trace id or root operation…" />
      <button class="control">Search</button>
      {#if data.filters.service}<input type="hidden" name="service" value={data.filters.service} />{/if}
      {#if data.filters.status}<input type="hidden" name="status" value={data.filters.status} />{/if}
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
      <div class="signal-layout">
        <FieldsPanel fields={data.view.fields} makeHref={facetHref} />
        <div class="signal-main">
          <p class="where-line">
            WHERE <strong>{data.view.queryWhere}</strong>
            <span class="muted"> · {data.view.matching.exact ? '' : '≥ '}{data.view.matching.count.toLocaleString()} matching · {data.view.errorRate} error · p95 {data.view.p95} · scanned {data.view.cost.scanned} in {data.view.cost.tookMs}ms</span>
          </p>
          {#if data.view.trend.length}
            <div class="red-strip">
              {@render strip('Rate', 'traces per bucket', data.view.rateTrend, 'ok', undefined)}
              {@render strip('Errors', 'error traces per bucket', data.view.trend, 'danger', undefined)}
              {@render strip('Duration', 'p95 ms per bucket', data.view.durationTrend, 'ok', fmtMs)}
            </div>
          {/if}
          {#if !data.view.rows.length}
            <StateBlock
              state="empty"
              title="No matching traces"
              detail="Your search and filters are preserved."
              actionLabel="Clear search and filters"
              actionHref={page.url.pathname}
            />
          {:else}
            <Panel title="Results" variant="raised">
              {#snippet head()}<span class="mono muted">{data.view.hasMore ? `first ${data.view.rows.length} of ${data.view.matching.count.toLocaleString()}` : `all ${data.view.matching.count.toLocaleString()}`} · sorted start ↓ · a row opens the trace</span>{/snippet}
              <Table label="Trace results">
                <table>
                  <thead><tr><th>Trace id</th><th>Root operation</th><th>Service</th><th>Start</th><th>Duration</th><th>Spans</th><th>Status</th></tr></thead>
                  <tbody>
                    {#each data.view.rows as row (row.id)}
                      <tr class={row.status.tone === 'danger' ? 'lvl-error' : ''}>
                        <td class="mono"><a href={rowHref(row.id)}><strong>{row.id}</strong></a></td>
                        <td class="mono">{row.rootOperation}</td>
                        <td class="mono">{row.service}</td>
                        <td class="mono">{row.start}</td>
                        <td class="mono">
                          <span class={`durbar ${row.status.tone === 'danger' ? 'err' : ''}`}><i style={`width:${(row.durationMs / maxDuration) * 100}%`}></i></span>
                          {(row.durationMs / 1000).toFixed(2)}s
                        </td>
                        <td class="mono">{row.spans} · {row.errorSpans} err</td>
                        <td><Badge tone={row.status.tone}>{row.status.label}</Badge></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </Table>
              {#if data.view.hasMore}
                <p class="load-more">
                  <a class="control" href={loadMoreHref} data-sveltekit-noscroll>Load 50 more</a>
                  <span class="mono muted">{(data.view.matching.count - data.view.rows.length).toLocaleString()} more match</span>
                </p>
              {/if}
            </Panel>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .load-more {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
  }
</style>
