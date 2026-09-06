<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/traces$/, ''));
  const serviceOptions = $derived([
    { value: '', label: 'all services' },
    ...(data.view?.services ?? []).map((s: string) => ({ value: s, label: s }))
  ]);
  const statusOptions = [
    { value: '', label: 'any status' },
    { value: 'error', label: 'error' },
    { value: 'ok', label: 'ok' }
  ];
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  /** A row opens its trace directly, carrying the whole search as return context. */
  function rowHref(id: string): string {
    return `${page.url.pathname}/${id}${page.url.search}`;
  }
  const maxDuration = $derived(Math.max(...(data.view?.rows.map((row) => row.durationMs) ?? [1])));
  const trendPeak = $derived(Math.max(...(data.view?.trend ?? [0])));
</script>

<svelte:head><title>Traces · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Traces</h1>
  <p class="muted">Search by service, status, duration and attributes — then open one.</p>
  <ObserveNav base={`${base}/observe`} current="Traces" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET">
      <input aria-label="Search traces" name="q" value={data.filters.q} placeholder="trace id or root operation…" />
      <button class="control">Search</button>
      <Select label="service" name="service" options={serviceOptions} value={data.filters.service} />
      <Select label="status" name="status" options={statusOptions} value={data.filters.status} />
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
      <Panel title="Trend · error traces per minute">
        {#snippet head()}<span class="mono muted">last {data.filters.range} · partial — {data.view.loaded} of {data.view.matching} loaded</span>{/snippet}
        <div class="row trend-row">
          <div class="kpis row">
            <div><p class="kpi-n">{data.view.matching}</p><p class="kpi-k">matching</p></div>
            <div><p class="kpi-n danger">{data.view.errorRate}</p><p class="kpi-k">error rate</p></div>
            <div><p class="kpi-n">{data.view.p95}</p><p class="kpi-k">p95 duration</p></div>
          </div>
          {#if data.view.trend.length}
            <div class="trend-grow">
              <div class="trendbars" role="img" aria-label={`Error traces per minute: ${data.view.trend.join(', ')}`}>
                {#each data.view.trend as bucket, i (i)}
                  <span style={`height:${Math.max(6, (bucket / trendPeak) * 100)}%`}></span>
                {/each}
              </div>
              <div class="trend-axis mono muted">
                <span>−{data.filters.range}</span>
                <span>peak {trendPeak}/min</span>
                <span>now</span>
              </div>
            </div>
          {/if}
        </div>
      </Panel>
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
          {#snippet head()}<span class="mono muted">sorted start ↓ · a row opens the trace, search preserved</span>{/snippet}
          <Table label="Trace results">
            <table>
              <thead><tr><th>Trace id</th><th>Root operation</th><th>Service</th><th>Start</th><th>Duration</th><th>Spans</th><th>Status</th></tr></thead>
              <tbody>
                {#each data.view.rows as row (row.id)}
                  <tr>
                    <td class="mono"><a href={rowHref(row.id)}><strong>{row.id}</strong></a></td>
                    <td class="mono">{row.rootOperation}</td>
                    <td class="mono">{row.service}</td>
                    <td class="mono">{row.start}</td>
                    <td class="mono">
                      {(row.durationMs / 1000).toFixed(2)}s
                      <span class="dur-bar"><span style={`width:${(row.durationMs / maxDuration) * 100}%`}></span></span>
                    </td>
                    <td class="mono">{row.spans} · {row.errorSpans} err</td>
                    <td><Badge tone={row.status.tone}>{row.status.label}</Badge></td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </Table>
        </Panel>
      {/if}
    {/if}
  </div>
</div>

<style>
  .kpi-n {
    font: 700 22px var(--font-display);
    margin: 0;
  }
  .kpi-n.danger {
    color: var(--danger-text);
  }
  .kpi-k {
    font: 700 9px var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 2px 0 0;
  }
  .kpis {
    gap: 22px;
    align-items: flex-start;
  }
  .trend-row {
    align-items: stretch;
  }
  .trend-grow {
    flex: 1;
    min-width: 200px;
  }
  .dur-bar {
    display: inline-block;
    vertical-align: middle;
    margin-left: 6px;
    width: 64px;
    height: 8px;
    border: 1px solid var(--border);
    border-radius: 2px;
    background: var(--surface-2);
  }
  .dur-bar > span {
    display: block;
    height: 100%;
    background: var(--brand-strong);
  }
  .reset {
    align-self: center;
    font-size: 11px;
  }
</style>
