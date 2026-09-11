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
  import { RANGES, withScope } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/logs$/, ''));
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  // A facet value link sets its field's URL param; the active value clears it.
  // Changing any filter returns to page 1 — the old page number is meaningless
  // against a different matching set.
  function facetHref(field: string, value: string, active: boolean): string {
    const query = new URLSearchParams(page.url.search);
    if (active) query.delete(field);
    else query.set(field, value);
    query.delete('record');
    query.delete('page');
    return `?${query}`;
  }
  function recordHref(id: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('record', id);
    return `?${query}`;
  }
  /** Paginator link to page `n` of the current search. */
  function pageHref(n: number): string {
    const query = new URLSearchParams(page.url.search);
    if (n <= 1) query.delete('page');
    else query.set('page', `${n}`);
    query.delete('record');
    return `?${query}`;
  }
  /** Rows-per-page link; resets to page 1. */
  function perHref(size: number): string {
    const query = new URLSearchParams(page.url.search);
    if (size === 50) query.delete('per');
    else query.set('per', `${size}`);
    query.delete('page');
    query.delete('record');
    return `?${query}`;
  }
  const PAGE_SIZES = [25, 50, 100, 250];
  const levelTone = (level: string) =>
    level === 'error' ? 'danger' : level === 'warn' ? 'warn' : 'neutral';
  /** Closing the drawer drops only the record param, keeping the search. */
  const closeHref = $derived.by(() => {
    const query = new URLSearchParams(page.url.search);
    query.delete('record');
    const rest = `${query}`;
    return rest ? `?${rest}` : page.url.pathname;
  });
  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && data.view?.selected) goto(closeHref, { noScroll: true });
  }
</script>

<svelte:window onkeydown={onKeydown} />
<svelte:head><title>Logs · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Logs</h1>
  <p class="muted">Guided log investigation — narrow the stream, then read one record.</p>
  <ObserveNav base={`${base}/observe`} current="Logs" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
      <input aria-label="Search log messages" name="q" value={data.filters.q} placeholder="message contains…" />
      <button class="control">Search</button>
      {#if data.filters.service}<input type="hidden" name="service" value={data.filters.service} />{/if}
      {#if data.filters.level}<input type="hidden" name="level" value={data.filters.level} />{/if}
      {#if data.filters.trace}<input type="hidden" name="trace" value={data.filters.trace} />{/if}
      {#if data.filters.per}<input type="hidden" name="per" value={data.filters.per} />{/if}
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
          <Panel title="Volume">
            {#snippet head()}<span class="mono muted">error records per bucket · {data.filters.range}</span>{/snippet}
            {#if data.view.trend.length}
              <TrendChart
                buckets={data.view.trend}
                range={data.filters.range}
                end={data.view.trendEnd}
                unit="error records"
                tone="danger"
                label={`Error log volume over ${data.filters.range}`}
              />
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
              {#snippet head()}<span class="mono muted">page {data.view.page} of {data.view.pageCount.toLocaleString()} · {data.view.records.length} rows</span>{/snippet}
              <Table label="Log records">
                <table>
                  <thead><tr><th>Time</th><th>Level</th><th>Service</th><th>Message</th><th>Trace</th></tr></thead>
                  <tbody>
                    {#each data.view.records as record (record.id)}
                      <tr class={`lvl-${record.level} ${data.view.selected?.id === record.id ? 'sel' : ''}`}>
                        <td class="mono"><a href={recordHref(record.id)} data-sveltekit-noscroll>{record.time}</a></td>
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
              <div class="paginator row mono" data-sveltekit-noscroll>
                <a class="control" href={pageHref(data.view.page - 1)} aria-disabled={data.view.page <= 1} class:off={data.view.page <= 1}>‹ prev</a>
                <span class="muted">page {data.view.page} of {data.view.pageCount.toLocaleString()}</span>
                <a class="control" href={pageHref(data.view.page + 1)} aria-disabled={data.view.page >= data.view.pageCount} class:off={data.view.page >= data.view.pageCount}>next ›</a>
                <span class="per muted">
                  rows:
                  {#each PAGE_SIZES as size (size)}
                    <a href={perHref(size)} aria-current={data.view.perPage === size ? 'true' : undefined} class:on={data.view.perPage === size}>{size}</a>
                  {/each}
                </span>
              </div>
            </Panel>
          {/if}
          <p class="mono muted query-handoff">
            <a class="control primary" href={withScope(`${base}/query`, data.scope, { sql: data.view.querySql })}>Open in Query</a>
            hands these filters to the read-only Bifrost workbench as one SELECT.
          </p>
        </div>
      </div>
      {#if data.view.selected}
        {@const selected = data.view.selected}
        <aside class="drawer" aria-label="Log record detail">
          <div class="drawer-head">
            <Badge tone={levelTone(selected.level)}>{selected.severityText} · {selected.severityNumber}</Badge>
            <span class="mono muted">{selected.time}</span>
            <a class="control close" href={closeHref} data-sveltekit-noscroll aria-label="Close record detail">✕</a>
          </div>
          <p class="drawer-message"><strong>{selected.message}</strong></p>
          {#if selected.eventName}
            <p class="mono muted event-name">{selected.eventName}</p>
          {/if}
          <section>
            <h3 class="mono">Body</h3>
            <pre class="mono body">{typeof selected.body === 'string' ? selected.body : JSON.stringify(selected.body, null, 2)}</pre>
          </section>
          <section>
            <h3 class="mono">Attributes</h3>
            {#if Object.keys(selected.attributes).length}
              <div class="kv">
                {#each Object.entries(selected.attributes) as [key, value] (key)}
                  <span class="k">{key}</span><span class="mono">{value}</span>
                {/each}
              </div>
            {:else}
              <p class="muted">No attributes on this record.</p>
            {/if}
            {#if selected.droppedAttributesCount > 0}
              <p class="muted">{selected.droppedAttributesCount} attributes dropped at the source.</p>
            {/if}
          </section>
          <section>
            <h3 class="mono">Trace context</h3>
            {#if selected.traceId}
              <div class="kv">
                <span class="k">trace_id</span>
                <a class="mono" href={withScope(`${base}/observe/traces/${selected.traceId}`, data.scope)}>{selected.traceId}</a>
                <span class="k">span_id</span><span class="mono">{selected.spanId ?? '—'}</span>
                <span class="k">trace_flags</span><span class="mono">{selected.traceFlags}{selected.traceFlags & 1 ? ' · sampled' : ''}</span>
              </div>
            {:else}
              <p class="muted">Not correlated to a trace.</p>
            {/if}
          </section>
          <section>
            <h3 class="mono">Resource · scope</h3>
            <div class="kv">
              <span class="k">service.name</span><span class="mono">{selected.service}</span>
              <span class="k">scope</span><span class="mono">{selected.scopeName} @ {selected.scopeVersion}</span>
            </div>
          </section>
          <section>
            <h3 class="mono">Timestamps</h3>
            <div class="kv">
              <span class="k">time</span><span class="mono">{selected.at}</span>
              <span class="k">observed_time</span><span class="mono">{selected.observedAt}</span>
            </div>
          </section>
        </aside>
      {/if}
    {/if}
  </div>
</div>

<style>
  .query-handoff {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11px;
  }
  /* Right-side pop-in drawer for one OTel record — raised drilldown altitude. */
  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 20;
    width: min(480px, 92vw);
    overflow-y: auto;
    padding: 16px;
    background: var(--surface);
    border-left: 3px solid var(--border);
    box-shadow: -6px 0 0 0 var(--shadow);
  }
  .drawer-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .drawer-head .close {
    margin-left: auto;
    text-decoration: none;
  }
  .drawer-message {
    margin: 10px 0 2px;
  }
  .event-name {
    font-size: 11px;
    margin: 0 0 4px;
  }
  .drawer section {
    margin-top: 14px;
  }
  .drawer h3 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin: 0 0 6px;
    padding-bottom: 4px;
    border-bottom: 2px dashed var(--border);
  }
  .drawer .body {
    margin: 0;
    padding: 8px 10px;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
  }
  .paginator {
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    font-size: 11px;
  }
  .paginator .off {
    pointer-events: none;
    opacity: 0.4;
  }
  .per {
    margin-left: auto;
    display: inline-flex;
    gap: 8px;
  }
  .per .on {
    font-weight: 700;
    text-decoration: underline;
  }
</style>
