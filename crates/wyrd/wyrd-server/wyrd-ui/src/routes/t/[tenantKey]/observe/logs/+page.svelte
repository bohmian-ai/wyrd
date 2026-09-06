<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES, withScope, scopeQuery } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/logs$/, ''));
  const serviceOptions = $derived([
    { value: '', label: 'all services' },
    ...(data.view?.services ?? []).map((s: string) => ({ value: s, label: s }))
  ]);
  const levelOptions = $derived([
    { value: '', label: 'all levels' },
    ...(data.view?.levels ?? []).map((l: string) => ({ value: l, label: l }))
  ]);
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  const trendPeak = $derived(Math.max(...(data.view?.trend ?? [0])));
  function recordHref(id: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('record', id);
    return `?${query}`;
  }
  const levelTone = (level: string) =>
    level === 'error' ? 'danger' : level === 'warn' ? 'warn' : 'neutral';
</script>

<svelte:head><title>Logs · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Logs</h1>
  <p class="muted">Guided log investigation — narrow the stream, then read one record.</p>
  <ObserveNav base={`${base}/observe`} current="Logs" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET">
      <input aria-label="Search log messages" name="q" value={data.filters.q} placeholder="message contains…" />
      <button class="control">Search</button>
      <Select label="service" name="service" options={serviceOptions} value={data.filters.service} />
      <Select label="level" name="level" options={levelOptions} value={data.filters.level} />
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
      <div class="columns">
        <div class="stack">
          <Panel title="Volume">
            {#snippet head()}<span class="mono muted">{data.view.matching} matching · error records per minute · {data.filters.range}</span>{/snippet}
            {#if data.view.trend.length}
              <div class="trendbars" role="img" aria-label={`Error log volume over ${data.filters.range}: ${data.view.trend.join(', ')} per bucket`}>
                {#each data.view.trend as bucket, i (i)}
                  <span style={`height:${Math.max(6, (bucket / trendPeak) * 100)}%`}></span>
                {/each}
              </div>
              <div class="trend-axis mono muted">
                <span>−{data.filters.range}</span>
                <span>peak {trendPeak}/min</span>
                <span>now</span>
              </div>
            {:else}
              <StateBlock state="empty" title="No records in this range" detail="Your search and filters are preserved." />
            {/if}
          </Panel>
          {#if !data.view.records.length}
            <StateBlock
              state="empty"
              title="No matching log records"
              detail="Your search and filters are preserved."
              actionLabel="Clear search and filters"
              actionHref={page.url.pathname}
            />
          {:else}
            <Panel title="Records" variant="raised">
              {#snippet head()}<span class="mono muted">partial — {data.view.loaded} of {data.view.matching} loaded</span>{/snippet}
              <Table label="Log records">
                <table>
                  <thead><tr><th>Time</th><th>Level</th><th>Service</th><th>Message</th><th>Trace</th></tr></thead>
                  <tbody>
                    {#each data.view.records as record (record.id)}
                      <tr class={data.view.selected?.id === record.id ? 'sel' : ''}>
                        <td class="mono"><a href={recordHref(record.id)}>{record.time}</a></td>
                        <td><Badge tone={levelTone(record.level)}>{record.level}</Badge></td>
                        <td class="mono">{record.service}</td>
                        <td><strong>{record.message}</strong></td>
                        <td class="mono">
                          {#if record.traceId}
                            <a href={withScope(`${base}/observe/traces/${record.traceId}`, data.scope)}>{record.traceId}</a>
                          {:else}—{/if}
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </Table>
              <p class="muted">Scrolling loads the remainder; the chips and search never reset.</p>
            </Panel>
          {/if}
          <div class="row">
            <a
              class="control primary"
              href={withScope(`${base}/query`, data.scope, { sql: data.view.querySql })}
              >Open in Query</a
            >
            <p class="muted">
              hands these filters to the read-only Bifrost workbench as one SELECT — this page stays
              guided investigation, never raw SQL.
            </p>
          </div>
        </div>
        <div class="stack">
          {#if data.view.selected}
            {@const selected = data.view.selected}
            <Panel title="Selected record" variant="raised">
              {#snippet head()}<span class="mono muted">{selected.time}</span>{/snippet}
              <div class="kv">
                {#each data.view.selected.fields as [key, value] (key + value)}
                  <span class="k">{key}</span><span class="mono">{value}</span>
                {/each}
              </div>
            </Panel>
            <Panel title="Message">
              <p>{data.view.selected.detail}</p>
            </Panel>
            <div class="stack rail-links">
              {#if data.view.selected.traceId}
                <a class="mono" href={withScope(`${base}/observe/traces/${data.view.selected.traceId}`, data.scope)}>→ {data.view.selected.traceId} in Traces</a>
              {/if}
              {#if data.filters.service}
                <a class="mono" href={`${base}/cards/card_service_01`}>→ {data.filters.service} Card</a>
              {/if}
              <a class="mono" href={`${base}/observe/metrics${scopeQuery(data.scope)}`}>→ Metrics, same range</a>
            </div>
          {:else}
            <StateBlock state="empty" title="No record selected" detail="Select a row to read its structured detail." />
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .rail-links {
    gap: 6px;
  }
</style>
