<!--
  Signal rows as typed cells: one row per root span for traces, one per span,
  log record or metric point otherwise, or the rows of authored SQL. Trace
  rows carry span and error counts read for the visible traces only.

  Only a signal list reads canonical columns: its headings are words and its
  durations, statuses, instants and services are typed cells. Authored SQL keeps
  its returned headings and values as text, whatever its aliases are called.

  A live list holds what is on screen, rows and counts together, while the
  reader hovers, focuses or scrolls it, after it opens a record until the
  reader resumes, or while the host pauses it; it offers "↑ N new" instead of
  moving rows under the pointer. The footer states the returned extent and
  never a total it did not read. A log list pages through its returned rows.

  Columns hides read columns; a host that passes `oncolumns` instead offers
  every column the signal may show and reads the reader's choice again. CSV
  saves the rows and columns on screen. A host that passes `controls` places
  Wrap, Columns and CSV in that element, such as its list bar, instead of
  above the table.
-->
<script lang="ts">
  import type { ActionProps, QueryResult } from '../contract.js';
  import { KEY_COLUMNS, LIST_LIMIT, STATUS_NAMES, cellText, columnLabel, shownColumns, signalColumns, type RecordTableProps } from '../loaders.js';
  import { fmtCount } from '../format.js';
  import { columnIndex, numeric } from '../plot.js';
  import ActionButton from './ActionButton.svelte';
  import ActionNote from './ActionNote.svelte';
  import DurationBar from './DurationBar.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill, { type Tone } from './StatusPill.svelte';

  let {
    input,
    executed,
    result,
    actions = {},
    onaction,
    paused = false,
    oncolumns,
    controls
  }: RecordTableProps & ActionProps & { paused?: boolean; oncolumns?: (columns: readonly string[]) => void; controls?: HTMLElement } = $props();

  /** Keep `node` inside `target` while it is mounted. */
  const placeIn = (target: HTMLElement) => (node: HTMLElement) => {
    target.append(node);
    return () => node.remove();
  };

  const TONES: Record<number, Tone> = { 0: 'neutral', 1: 'ok', 2: 'danger' };
  const NOUNS = { traces: 'traces', spans: 'spans', logs: 'records', metrics: 'points' } as const;
  const signal = $derived('signal' in input ? input.signal : undefined);
  const noun = $derived(signal ? NOUNS[signal] : 'rows');
  const loaded = $derived(result.state === 'populated' ? result.data : result.state === 'empty' ? { rows: result.data.rows } : undefined);
  const latest = $derived(loaded?.rows);

  /** The signal list's column projection; authored SQL has none. */
  const projection = $derived('signal' in input ? shownColumns(input) : undefined);
  /**
   * Rows, counts, columns and executed query on screen while the reader holds
   * a live list. A changed projection replaces the snapshot once a read that
   * carries every new column lands, so headings, cells, CSV and Open in Query
   * always describe one read.
   */
  let held = $state.raw<{ data: NonNullable<typeof loaded>; columns: typeof projection; executed: typeof executed }>();
  let reading = $state(false);
  /** A record opened from this list, until the reader resumes. */
  let opened = $state(false);
  const holding = $derived(input.live === true && (paused || reading || opened));
  const same = (a: readonly string[] | undefined, b: readonly string[] | undefined) => JSON.stringify(a) === JSON.stringify(b);
  $effect.pre(() => {
    if (!holding) held = undefined;
    else if (loaded && (!held || (!same(held.columns, projection) && (projection ?? []).every((column) => columnIndex(loaded.rows, column) >= 0))))
      held = { data: loaded, columns: projection, executed };
  });
  const shown = $derived(held?.data ?? loaded);
  const query = $derived(held ? held.executed : executed);
  const rows = $derived(shown?.rows);
  function resume() {
    held = undefined;
    reading = false;
    opened = false;
  }

  const columns = $derived(rows ? ((held ? held.columns : projection) ?? rows.columns.map((column) => column.name)) : []);
  /** Columns the reader hid with Columns; reads are unchanged. */
  let hidden = $state.raw<ReadonlySet<string>>(new Set());
  const visible = $derived(columns.filter((column) => !hidden.has(column)));
  let wrap = $state(false);
  const heading = (column: string) => (signal ? columnLabel(column) : column);
  const text = (column: string, value: unknown) => cellText(signal ? column : '', value);

  const keyOf = (data: QueryResult, row: readonly unknown[]) => {
    const keys = signal ? KEY_COLUMNS[signal].map((name) => row[columnIndex(data, name)]) : [];
    return JSON.stringify(keys.length ? keys : row);
  };
  const fresh = $derived.by(() => {
    const before = held?.data.rows;
    if (!before || !latest || before === latest) return 0;
    const seen = new Set(before.rows.map((row) => keyOf(before, row)));
    return latest.rows.filter((row) => !seen.has(keyOf(latest, row))).length;
  });

  const counts = $derived(shown?.counts);
  /** A count cell before its read lands, or after it was refused. */
  const pending = $derived(counts?.state === 'denied' || counts?.state === 'error' ? '—' : '…');
  const byTrace = $derived(
    new Map(counts?.state === 'populated' ? counts.data.rows.map((row) => [String(row[0]), [numeric(row[1]), numeric(row[2])]]) : [])
  );
  const longest = $derived(
    rows && signal && columns.includes('duration_nano') ? Math.max(0, ...rows.rows.map((row) => numeric(row[columnIndex(rows, 'duration_nano')]) / 1e6)) : 0
  );
  /** A canonical span row whose recorded status is OTLP error. */
  const erring = (data: QueryResult, row: readonly unknown[]) =>
    (signal === 'traces' || signal === 'spans') && columnIndex(data, 'status_code') >= 0 && numeric(row[columnIndex(data, 'status_code')]) === 2;

  /** A log list pages through the rows it read; other lists scroll. */
  const PAGE_SIZES = [25, 50, 100] as const;
  let pageSize = $state<number>(50);
  let page = $state(0);
  const pages = $derived(signal === 'logs' && rows ? Math.max(1, Math.ceil(rows.rows.length / pageSize)) : 1);
  const current = $derived(Math.min(page, pages - 1));
  const paged = $derived(rows ? (signal === 'logs' ? rows.rows.slice(current * pageSize, (current + 1) * pageSize) : rows.rows) : []);
  /** Page numbers to offer: every page up to seven, else the ends and the current page's neighbours. */
  const offered = $derived.by(() => {
    const all = Array.from({ length: pages }, (_, index) => index);
    if (pages <= 7) return all;
    return all.filter((index) => index === 0 || index === pages - 1 || Math.abs(index - current) <= 1);
  });
  /** A signal list that returned its whole limit may have older rows it did not read. */
  const limited = $derived(signal !== undefined && rows !== undefined && rows.rows.length >= (input.limit ?? LIST_LIMIT));

  /**
   * Rows split by the groupBy column, groups in first-seen order. Groups are
   * keyed by the typed value; a caption shared by distinct values gains the value's
   * JSON spelling, so formatting never merges or confuses groups.
   */
  const groups = $derived.by(() => {
    if (!rows) return [];
    const at = input.groupBy ? columnIndex(rows, input.groupBy) : -1;
    const grouped = new Map<string, { value: unknown; members: (readonly unknown[])[] }>();
    for (const row of paged) {
      const key = at < 0 ? '' : JSON.stringify(row[at] ?? null);
      const group = grouped.get(key) ?? { value: row[at] ?? null, members: [] };
      group.members.push(row);
      grouped.set(key, group);
    }
    const captions = [...grouped.values()].map(({ value }) => text(input.groupBy ?? '', value));
    return [...grouped].map(([key, { value, members }], index) => {
      const caption = captions[index];
      return { key, members, caption: captions.indexOf(caption) === captions.lastIndexOf(caption) ? caption : `${caption} (${JSON.stringify(value)})` };
    });
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
    opened = input.live === true;
    onaction?.({ name: 'record.open', input: target });
  }

  function onscroll(event: Event) {
    reading = (event.currentTarget as HTMLElement).scrollTop > 0;
  }
  /** The columns Columns offers: every column of the signal when the host reads the choice, else the read ones. */
  const offeredColumns = $derived(oncolumns && signal ? signalColumns(signal) : columns);
  const isShown = (column: string) => visible.includes(column);
  /** Show or hide `column`; false when the change was refused because no column would remain. */
  function toggle(column: string): boolean {
    if (oncolumns && signal) {
      const next = offeredColumns.filter((each) => (each === column ? !isShown(each) : isShown(each)));
      if (next.length) oncolumns(next);
      return next.length > 0;
    }
    const next = new Set(hidden);
    if (!next.delete(column)) next.add(column);
    hidden = next;
    return true;
  }
  /** The rows on screen as RFC 4180 CSV, cells as their exact returned text. */
  const csv = $derived.by(() => {
    if (!rows) return '';
    const quote = (value: unknown) => {
      const text = value === null || value === undefined ? '' : typeof value === 'object' ? JSON.stringify(value) : String(value);
      return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
    };
    return [visible.map(quote).join(','), ...rows.rows.map((row) => visible.map((column) => quote(row[columnIndex(rows, column)])).join(','))].join('\r\n');
  });
</script>

{#if rows && rows.rows.length > 0}
  <div class="table" data-signal={signal ?? 'sql'}>
    {#if input.live}
      <p class="live" role="status">
        {#if holding}
          <button type="button" onclick={resume}
            >Live paused{opened ? ' while a record is open' : reading ? ' while you read' : ''} · {fresh > 0 ? `↑ ${fresh} new ${noun}` : 'Resume'}</button
          >
        {:else}
          <span class="quiet">Live</span>
        {/if}
      </p>
    {/if}
    {#if counts?.state === 'denied' || counts?.state === 'error'}
      <p class="note" role="note">Span and error counts unavailable: {counts.problem.title} ({counts.problem.code})</p>
    {/if}
    <div class="frame">
      {#if signal && controls}
        <span class="tools" {@attach placeIn(controls)}>{@render tools()}</span>
      {:else if signal}
        <div class="bar">
          <span>{fmtCount(rows.rows.length)} {noun} · newest first</span>
          <span class="tools">{@render tools()}</span>
        </div>
      {/if}
      <div
        class="scroll"
        class:wrap
        role="region"
        aria-label="{signal ?? 'Query'} rows"
        {onscroll}
        onmouseenter={() => (reading = true)}
        onmouseleave={(event) => (reading = event.currentTarget.scrollTop > 0)}
        onfocusin={() => (reading = true)}
        onfocusout={(event) => (reading = event.currentTarget.contains(event.relatedTarget as Node) || event.currentTarget.scrollTop > 0)}
      >
        <table>
          <thead>
            <tr>
              {#each visible as column (column)}<th scope="col" title={column}>{heading(column)}</th>{/each}
              {#if signal === 'traces'}<th scope="col" class="number">spans</th><th scope="col" class="number">errors</th>{/if}
            </tr>
          </thead>
          <tbody>
            {#each groups as { key, caption, members } (key)}
              {#if input.groupBy}
                <tr class="group"><th scope="rowgroup" colspan={visible.length + (signal === 'traces' ? 2 : 0)}>{heading(input.groupBy)} = {caption} · {members.length}</th></tr>
              {/if}
              {#each members as row, index (index)}
                {@const traceCounts = byTrace.get(String(row[columnIndex(rows, 'trace_id')]))}
                <tr
                  class:openable
                  class:error={erring(rows, row)}
                  tabindex={openable ? 0 : undefined}
                  onclick={() => open(rows, row)}
                  onkeydown={(event) => event.key === 'Enter' && open(rows, row)}
                >
                  {#each visible as column (column)}
                    {@const value = row[columnIndex(rows, column)]}
                    <td class:name={signal && column === 'name'}>
                      {#if !signal || value === null}
                        <span class:null={value === null} title={typeof value === 'string' && value.length > 40 ? value : undefined}>{text(column, value)}</span>
                      {:else if column === 'duration_nano'}
                        <DurationBar ms={Math.round(numeric(value) / 1e6)} max={longest} />
                      {:else if column === 'status_code' && STATUS_NAMES[numeric(value)]}
                        <StatusPill tone={TONES[numeric(value)]} label={STATUS_NAMES[numeric(value)]} />
                      {:else if column === 'service_name'}
                        <span class="chip" title={String(value)}>{String(value)}</span>
                      {:else if column.endsWith('_unix_nano')}
                        <time datetime={String(value)} title={String(value)}>{text(column, value)}</time>
                      {:else}
                        <span title={typeof value === 'string' && value.length > 40 ? value : undefined}>{text(column, value)}</span>
                      {/if}
                    </td>
                  {/each}
                  {#if signal === 'traces'}
                    <td class="number">{traceCounts ? fmtCount(traceCounts[0]) : pending}</td>
                    <td class="number" class:errors={(traceCounts?.[1] ?? 0) > 0}>{traceCounts ? fmtCount(traceCounts[1]) : pending}</td>
                  {/if}
                </tr>
              {/each}
            {/each}
          </tbody>
        </table>
      </div>
      {#if signal === 'logs'}
        <nav class="foot pager" aria-label="Pages">
          <button type="button" aria-label="Previous page" disabled={current === 0} onclick={() => (page = current - 1)}>‹</button>
          {#each offered as index, position (index)}
            {#if position > 0 && index - offered[position - 1] > 1}<span aria-hidden="true">…</span>{/if}
            <button type="button" aria-current={index === current ? 'page' : undefined} onclick={() => (page = index)}>{index + 1}</button>
          {/each}
          <button type="button" aria-label="Next page" disabled={current >= pages - 1} onclick={() => (page = current + 1)}>›</button>
          {#if limited}<span>newest {fmtCount(rows.rows.length)} read · older records not read</span>{/if}
          <label class="size">
            <select
              value={pageSize}
              onchange={(event) => {
                pageSize = Number(event.currentTarget.value);
                page = 0;
              }}
            >
              {#each PAGE_SIZES as size (size)}<option value={size}>{size}</option>{/each}
            </select>
            per page
          </label>
        </nav>
      {:else}
        <p class="foot">
          Showing {fmtCount(rows.rows.length)}
          {noun}{#if limited}{' '}· newest {fmtCount(rows.rows.length)} read, older {noun} not read{/if}
        </p>
      {/if}
    </div>
    <ActionNote state={openState} />
    {#if query}
      <span class="query">
        <ActionButton
          variant="link"
          label="Open in Query"
          icon="↗"
          state={onaction ? actions['record.open_in_query'] : { state: 'unavailable', reason: 'This host has no Query' }}
          onclick={() => onaction?.({ name: 'record.open_in_query', input: query })}
        />
      </span>
    {/if}
  </div>
{:else if result.state === 'empty'}
  <EmptyState state="empty" title={result.reason} />
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {signal ?? 'query'} rows" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

{#snippet tools()}
  <button type="button" aria-pressed={wrap} onclick={() => (wrap = !wrap)}>Wrap lines</button>
  <details class="columns">
    <summary>Columns</summary>
    <fieldset>
      <legend>Shown columns</legend>
      {#each offeredColumns as column (column)}
        <label title={column}><input
            type="checkbox"
            checked={isShown(column)}
            onchange={(event) => {
              // A refused change, hiding the last column, leaves the box as listed.
              if (!toggle(column)) event.currentTarget.checked = isShown(column);
            }}
          /> {heading(column)}</label>
      {/each}
    </fieldset>
  </details>
  <a class="csv" href="data:text/csv;charset=utf-8,{encodeURIComponent(csv)}" download="{signal}.csv">CSV</a>
{/snippet}

<style>
  .query {
    align-self: flex-end;
  }
  .table {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .frame {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    font: 12px var(--font-mono);
  }
  .tools {
    display: flex;
    gap: 6px;
  }
  .tools button,
  .csv,
  summary {
    display: inline-flex;
    align-items: center;
    height: 28px;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 10px;
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .tools button:hover,
  summary:hover {
    background: var(--surface-hover);
  }
  .tools button[aria-pressed='true'] {
    border-color: var(--declare);
    color: var(--declare);
  }
  summary {
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .columns {
    position: relative;
  }
  fieldset {
    position: absolute;
    right: 0;
    z-index: 2;
    display: grid;
    gap: 4px;
    min-width: 180px;
    margin: 4px 0 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 8px 10px;
    background: var(--surface);
    font: 12px var(--font-mono);
  }
  legend {
    float: left;
    margin-bottom: 4px;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  fieldset label {
    display: flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .scroll {
    max-height: 560px;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 30px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--surface);
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .group th {
    position: static;
    color: var(--text);
    text-transform: none;
  }
  td {
    max-width: 360px;
    height: 40px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  [data-signal='spans'] td {
    height: 34px;
  }
  [data-signal='logs'] td,
  [data-signal='metrics'] td,
  [data-signal='sql'] td {
    height: 30px;
  }
  .wrap td {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .number {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  td :global(.duration) {
    min-width: 140px;
  }
  .error .name,
  .error :global(.duration .value) {
    color: var(--danger);
  }
  .error :global(.duration .bar) {
    background: var(--danger);
  }
  .chip {
    display: inline-block;
    max-width: 100%;
    box-sizing: border-box;
    border: 1px solid color-mix(in srgb, var(--declare) 40%, var(--border));
    border-radius: var(--r);
    padding: 1px 8px;
    background: var(--declare-soft);
    color: var(--declare);
    font: 11px var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
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
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin: 0;
    padding: 10px 14px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .pager {
    border-top: 1px solid var(--border);
  }
  .pager button {
    border: 0;
    padding: 0 2px;
    background: none;
    color: var(--declare);
    font: inherit;
    cursor: pointer;
  }
  .pager button:disabled {
    color: var(--muted);
    cursor: default;
  }
  .pager [aria-current='page'] {
    color: var(--text);
    text-decoration: underline;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  select {
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: inherit;
  }
  .live,
  .note {
    margin: 0;
    color: var(--muted);
    font: 12px var(--font-sans);
  }
  .live {
    display: flex;
    justify-content: center;
  }
  .live button {
    display: inline-flex;
    align-items: center;
    height: 28px;
    box-sizing: border-box;
    border: 1px solid color-mix(in srgb, var(--declare) 40%, var(--border));
    border-radius: var(--r);
    padding: 0 10px;
    background: var(--declare-soft);
    color: var(--declare);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .quiet {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .note {
    color: var(--warn);
  }
  button:focus-visible,
  summary:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 1px;
  }
</style>
