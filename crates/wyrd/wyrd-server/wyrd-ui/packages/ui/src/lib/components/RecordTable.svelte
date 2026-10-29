<!--
  Signal rows as typed cells: one row per root span for traces, one per span,
  log record or metric point otherwise, or the rows of authored SQL. Trace
  rows carry span and error counts read for the visible traces only.

  A live list holds what is on screen while the reader hovers, focuses or
  scrolls it, or the host pauses it, and offers "↑ N new" instead of moving
  rows under the pointer.
-->
<script lang="ts">
  import type { ActionProps, QueryResult } from '../contract.js';
  import { KEY_COLUMNS, cellText, shownColumns, type RecordTableProps } from '../loaders.js';
  import { fmtCount } from '../format.js';
  import { columnIndex, numeric } from '../plot.js';
  import ActionNote from './ActionNote.svelte';
  import DurationBar from './DurationBar.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill, { type Tone } from './StatusPill.svelte';

  let { input, result, actions = {}, onaction, paused = false }: RecordTableProps & ActionProps & { paused?: boolean } = $props();

  const STATUS: Record<number, [Tone, string]> = { 0: ['neutral', 'unset'], 1: ['ok', 'ok'], 2: ['danger', 'error'] };
  const signal = $derived('signal' in input ? input.signal : undefined);
  const latest = $derived(result.state === 'populated' || result.state === 'empty' ? result.data.rows : undefined);

  /** Rows on screen while the reader holds a live list. */
  let held = $state.raw<QueryResult | undefined>();
  let reading = $state(false);
  const holding = $derived(input.live === true && (paused || reading));
  $effect.pre(() => {
    if (!holding) held = undefined;
    else if (!held) held = latest;
  });
  const rows = $derived(held ?? latest);

  const columns = $derived(rows ? ('signal' in input ? shownColumns(input) : rows.columns.map((column) => column.name)) : []);
  const keyOf = (data: QueryResult, row: readonly unknown[]) => {
    const keys = signal ? KEY_COLUMNS[signal].map((name) => row[columnIndex(data, name)]) : [];
    return JSON.stringify(keys.length ? keys : row);
  };
  const fresh = $derived.by(() => {
    if (!held || !latest || held === latest) return 0;
    const seen = new Set(held.rows.map((row) => keyOf(held!, row)));
    return latest.rows.filter((row) => !seen.has(keyOf(latest, row))).length;
  });

  const counts = $derived(result.state === 'populated' ? result.data.counts : undefined);
  const byTrace = $derived(
    new Map(counts?.state === 'populated' ? counts.data.rows.map((row) => [String(row[0]), [numeric(row[1]), numeric(row[2])]]) : [])
  );
  const longest = $derived(
    rows && columns.includes('duration_nano') ? Math.max(0, ...rows.rows.map((row) => numeric(row[columnIndex(rows, 'duration_nano')]) / 1e6)) : 0
  );

  /** Rows split by the groupBy column, groups in first-seen order. */
  const groups = $derived.by(() => {
    if (!rows) return [];
    const at = input.groupBy ? columnIndex(rows, input.groupBy) : -1;
    const grouped = new Map<string, (readonly unknown[])[]>();
    for (const row of rows.rows) {
      const key = at < 0 ? '' : cellText(input.groupBy ?? '', row[at]);
      grouped.set(key, [...(grouped.get(key) ?? []), row]);
    }
    return [...grouped];
  });

  const openState = $derived(actions['record.open']);
  const openable = $derived(signal !== undefined && signal !== 'metrics' && onaction !== undefined);
  function open(data: QueryResult, row: readonly unknown[]) {
    if (!openable || openState?.state === 'pending' || openState?.state === 'unavailable') return;
    const at = (name: string) => row[columnIndex(data, name)];
    const target =
      signal === 'logs'
        ? { log: { time_unix_nano: at('time_unix_nano'), observed_time_unix_nano: at('observed_time_unix_nano'), wyrd_request_id: at('wyrd_request_id') } }
        : { trace_id: at('trace_id'), span_id: at('span_id') };
    onaction?.({ name: 'record.open', input: target });
  }

  function onscroll(event: Event) {
    reading = (event.currentTarget as HTMLElement).scrollTop > 0;
  }
</script>

{#if rows && rows.rows.length > 0}
  <div class="table">
    {#if input.live}
      <p class="live" role="status">
        {#if holding}
          Live paused{fresh > 0 ? '' : ' while you read'}
          {#if fresh > 0}
            · <button type="button" onclick={() => ((held = undefined), (reading = false))}>↑ {fresh} new {signal ?? 'rows'}</button>
          {/if}
        {:else}
          <span aria-hidden="true">●</span> Live
        {/if}
      </p>
    {/if}
    {#if counts?.state === 'denied' || counts?.state === 'error'}
      <p class="note" role="note">Span and error counts unavailable: {counts.problem.title} ({counts.problem.code})</p>
    {/if}
    <div
      class="scroll"
      role="region"
      aria-label="{signal ?? 'Query'} rows"
      {onscroll}
      onmouseenter={() => (reading = true)}
      onmouseleave={(event) => (reading = event.currentTarget.scrollTop > 0)}
    >
      <table>
        <thead>
          <tr>
            {#each columns as column (column)}<th scope="col">{column}</th>{/each}
            {#if signal === 'traces'}<th scope="col">spans</th><th scope="col">errors</th>{/if}
          </tr>
        </thead>
        <tbody>
          {#each groups as [group, members] (group)}
            {#if input.groupBy}
              <tr class="group"><th scope="rowgroup" colspan={columns.length + (signal === 'traces' ? 2 : 0)}>{input.groupBy} = {group} · {members.length}</th></tr>
            {/if}
            {#each members as row, index (index)}
              {@const traceCounts = byTrace.get(String(row[columnIndex(rows, 'trace_id')]))}
              <tr
                class:openable
                tabindex={openable ? 0 : undefined}
                onclick={() => open(rows, row)}
                onkeydown={(event) => event.key === 'Enter' && open(rows, row)}
              >
                {#each columns as column (column)}
                  {@const value = row[columnIndex(rows, column)]}
                  <td>
                    {#if column === 'duration_nano' && value !== null}
                      <DurationBar ms={Math.round(numeric(value) / 1e6)} max={longest} />
                    {:else if column === 'status_code' && value !== null && STATUS[numeric(value)]}
                      <StatusPill tone={STATUS[numeric(value)][0]} label={STATUS[numeric(value)][1]} />
                    {:else}
                      <span class:null={value === null}>{cellText(column, value)}</span>
                    {/if}
                  </td>
                {/each}
                {#if signal === 'traces'}
                  <td>{traceCounts ? fmtCount(traceCounts[0]) : '…'}</td>
                  <td class:errors={(traceCounts?.[1] ?? 0) > 0}>{traceCounts ? fmtCount(traceCounts[1]) : '…'}</td>
                {/if}
              </tr>
            {/each}
          {/each}
        </tbody>
      </table>
    </div>
    <ActionNote state={openState} />
  </div>
{:else if result.state === 'empty'}
  <EmptyState state="empty" title={result.reason} />
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {signal ?? 'query'} rows" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .table {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .scroll {
    max-height: 560px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  th {
    position: sticky;
    top: 0;
    padding: 8px 10px;
    background: var(--surface-2);
    color: var(--muted);
    font: 10.5px var(--font-mono);
    letter-spacing: 0.04em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .group th {
    position: static;
    background: var(--surface);
    color: var(--text);
    text-transform: none;
  }
  td {
    max-width: 360px;
    padding: 7px 10px;
    border-top: 1px solid var(--border-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td :global(.duration) {
    min-width: 140px;
  }
  .openable {
    cursor: pointer;
  }
  .openable:hover {
    background: var(--surface-hover);
  }
  .openable:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: -2px;
  }
  .null {
    color: var(--muted);
  }
  .errors {
    color: var(--danger);
  }
  .live,
  .note {
    margin: 0;
    color: var(--muted);
    font: 12px var(--font-sans);
  }
  .note {
    color: var(--warn);
  }
  .live button {
    border: 0;
    padding: 0;
    background: none;
    color: var(--declare);
    font: inherit;
    cursor: pointer;
  }
</style>
