<script lang="ts">
  import { enhance } from '$app/forms';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { Drawer } from 'wyrd-ai-ui';
  import { WyrdView, remoteLoader } from 'wyrd-ai-ui/view';
  import type { ActionIntent } from 'wyrd-ai-ui';
    import { OBSERVE_SIGNALS, PRESETS, closed, detailView, groupColumns, observeView, readState, step, writeState, type ObserveState } from '$lib/observe-host';
  import type { PageProps } from './$types';

  let { data, form }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const load = $derived(remoteLoader(`${base}/ui/load`));
  const state = $derived(readState(page.url.searchParams));
  // The list's view never names the open record, so opening one never reloads the list.
  const view = $derived(observeView(closed(state), data.predicate));
  const detail = $derived(detailView(state));
  const LABELS = { traces: 'Traces', logs: 'Logs', metrics: 'Metrics' } as const;

  /** The URL of `next`; every control navigates, so state survives reload and Back. */
  const href = (next: ObserveState) => `${base}/observe?${writeState(next)}`;
  const go = (next: ObserveState) => goto(href(next), { keepFocus: true, noScroll: true });

  function onaction(intent: ActionIntent) {
    const next = step(state, intent, `${page.url.pathname}${page.url.search}`);
    if ('unavailable' in next) return { state: 'unavailable', reason: next.unavailable } as const;
    if ('query' in next) return void goto(`${base}/query?${next.query}`);
    return void go(next.state);
  }

  /** Apply a custom absolute range from the two local datetime inputs. */
  function custom(event: SubmitEvent) {
    event.preventDefault();
    const fields = new FormData(event.currentTarget as HTMLFormElement);
    const from = new Date(String(fields.get('from')));
    const to = new Date(String(fields.get('to')));
    if (Number.isNaN(from.getTime()) || Number.isNaN(to.getTime()) || from >= to) return;
    void go({ ...closed(state), last: undefined, from: from.toISOString(), to: to.toISOString() });
  }
  /** An instant as a `datetime-local` value in the viewer's zone. */
  const local = (iso?: string) => (iso ? new Date(new Date(iso).getTime() - new Date(iso).getTimezoneOffset() * 60_000).toISOString().slice(0, 16) : '');
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
</script>

<svelte:head><title>Observe · {data.tenant.name} · Wyrd</title></svelte:head>
<h1>Observe</h1>

<nav class="tabs" aria-label="Signal">
  {#each OBSERVE_SIGNALS as signal (signal)}
    <a href={href({ ...closed(state), signal, where: [], where_sql: '', group: undefined })} aria-current={state.signal === signal ? 'page' : undefined}>{LABELS[signal]}</a>
  {/each}
</nav>

<div class="controls">
  {#if state.signal === 'traces'}
    <span class="segmented" role="group" aria-label="Rows">
      <a href={href({ ...closed(state), mode: 'traces', group: undefined })} aria-current={state.mode === 'traces' ? 'true' : undefined}>Trace roots</a>
      <a href={href({ ...closed(state), mode: 'spans', group: undefined })} aria-current={state.mode === 'spans' ? 'true' : undefined}>All spans</a>
    </span>
  {/if}
  <label>
    Range
    <select
      aria-label="Range"
      value={state.last ?? 'custom'}
      onchange={(event) => event.currentTarget.value !== 'custom' && go({ ...closed(state), last: event.currentTarget.value, from: undefined, to: undefined })}
    >
      {#each PRESETS as preset (preset)}<option value={preset}>Last {preset}</option>{/each}
      <option value="custom">Custom</option>
    </select>
  </label>
  <form class="custom" onsubmit={custom} aria-label="Custom range">
    <input type="datetime-local" name="from" aria-label="From" value={local(state.from)} required />
    <input type="datetime-local" name="to" aria-label="To" value={local(state.to)} required />
    <button class="app-control" type="submit">Apply</button>
    <span class="zone">{zone}</span>
  </form>
  <label>
    Group
    <select aria-label="Group by" value={state.group ?? ''} onchange={(event) => go({ ...closed(state), group: event.currentTarget.value || undefined })}>
      <option value="">None</option>
      {#each groupColumns(state) as column (column)}<option value={column}>{column}</option>{/each}
    </select>
  </label>
  <label class="live"><input type="checkbox" checked={state.live} onchange={(event) => go({ ...state, live: event.currentTarget.checked })} /> Live</label>
</div>

<form class="where" method="POST" action="?/where{page.url.search ? `&${page.url.search.slice(1)}` : ''}" use:enhance={() => async ({ update }) => update({ reset: false })}>
  <label for="where_sql">WHERE</label>
  <input id="where_sql" name="where_sql" value={form?.draft ?? state.where_sql} placeholder="http_response_status_code >= 500" autocomplete="off" spellcheck="false" />
  <button class="app-control" type="submit">Apply</button>
</form>
{#if form?.problem}
  <p class="draft" role="alert">Not applied: {form.problem.detail ?? form.problem.title} <code>{form.problem.code}</code></p>
{/if}
{#if state.where.length}
  <ul class="filters" aria-label="Applied filters">
    {#each state.where as clause, index (index)}
      <li>
        <code>{clause.column} {clause.not ? '≠' : '='} {JSON.stringify(clause.value)}</code>
        <a aria-label="Remove {clause.column}" href={href({ ...closed(state), where: state.where.filter((_, at) => at !== index) })}>✕</a>
      </li>
    {/each}
  </ul>
{/if}

<WyrdView doc={view} {load} {onaction} />
<Drawer open={detail !== undefined} title={state.log ? 'Log record' : 'Trace'} onclose={() => go(closed(state))}>
  {#if detail}<WyrdView doc={detail} {load} {onaction} />{/if}
</Drawer>

<style>
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
    margin-bottom: 12px;
  }
  .tabs,
  .controls,
  .where,
  .filters,
  .segmented,
  .custom {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font: 12px var(--font-sans);
  }
  .tabs {
    border-bottom: 1px solid var(--border);
    margin-bottom: 12px;
  }
  .tabs a {
    padding: 6px 10px;
    color: var(--muted);
    text-decoration: none;
  }
  .tabs a[aria-current='page'],
  .segmented a[aria-current='true'] {
    color: var(--text);
    border-bottom: 2px solid var(--declare);
  }
  .segmented a {
    color: var(--declare);
    text-decoration: none;
    padding: 2px 6px;
  }
  .controls,
  .where {
    margin-bottom: 8px;
  }
  .where input {
    flex: 1 1 320px;
    font: 12px var(--font-mono);
  }
  .zone {
    color: var(--muted);
  }
  .draft {
    color: var(--danger, var(--text));
    font: 12px var(--font-sans);
  }
  .filters {
    list-style: none;
    padding: 0;
    margin: 0 0 8px;
  }
  .filters li {
    display: flex;
    gap: 4px;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: var(--r);
  }
</style>
