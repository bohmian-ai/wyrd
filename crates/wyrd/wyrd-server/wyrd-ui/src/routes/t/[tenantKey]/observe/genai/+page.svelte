<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import FieldsPanel from '$lib/features/observe/FieldsPanel.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES, withScope } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/genai$/, ''));
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
  /** A row opens its trace's GenAI view with the record's span selected. */
  function rowHref(traceId: string, spanId: string): string {
    return withScope(`${base}/observe/traces/${traceId}`, data.scope, {
      view: 'genai',
      span: spanId
    });
  }
</script>

<svelte:head><title>GenAI · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>GenAI</h1>
  <p class="muted">Extracted GenAI calls — model messages and tool calls across traces.</p>
  <ObserveNav base={`${base}/observe`} current="GenAI" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
      <input aria-label="Search GenAI calls" name="q" value={data.filters.q} placeholder="trace or conversation id…" />
      <button class="control">Search</button>
      {#if data.filters.service}<input type="hidden" name="service" value={data.filters.service} />{/if}
      {#if data.filters.model}<input type="hidden" name="model" value={data.filters.model} />{/if}
      {#if data.filters.operation}<input type="hidden" name="operation" value={data.filters.operation} />{/if}
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
            <span class="muted"> · {data.view.matching.exact ? '' : '≥ '}{data.view.matching.count.toLocaleString()} matching · scanned {data.view.cost.scanned} in {data.view.cost.tookMs}ms</span>
          </p>
          {#if !data.view.rows.length}
            <StateBlock
              state="empty"
              title="No matching GenAI calls"
              detail="Your search and filters are preserved. Absence is normal for non-agent traffic."
              actionLabel="Clear search and filters"
              actionHref={page.url.pathname}
            />
          {:else}
            <Panel title="Calls" variant="raised">
              {#snippet head()}<span class="mono muted">{data.view.hasMore ? `first ${data.view.rows.length} of ${data.view.matching.count.toLocaleString()}` : `all ${data.view.matching.count.toLocaleString()}`} · sorted start ↓ · a row opens its trace's GenAI view</span>{/snippet}
              <Table label="GenAI calls">
                <table>
                  <thead><tr><th>Time</th><th>Operation</th><th>Model / tool</th><th>Service</th><th>Tokens in / out</th><th>Duration</th><th>Status</th><th>Conversation</th></tr></thead>
                  <tbody>
                    {#each data.view.rows as row (row.id)}
                      <tr class={row.status.tone === 'danger' ? 'lvl-error' : ''}>
                        <td class="mono"><a href={rowHref(row.traceId, row.id)}>{row.start}</a></td>
                        <td class="mono">{row.operation}</td>
                        <td class="mono"><strong>{row.model}</strong></td>
                        <td class="mono">{row.service}</td>
                        <td class="mono">{row.tokensIn === null ? '—' : `${row.tokensIn.toLocaleString()} / ${row.tokensOut?.toLocaleString()}`}</td>
                        <td class="mono">{row.durationMs >= 1000 ? `${(row.durationMs / 1000).toFixed(2)}s` : `${row.durationMs}ms`}</td>
                        <td><Badge tone={row.status.tone}>{row.status.label}</Badge></td>
                        <td class="mono">{row.conversationId ?? '—'}</td>
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
