<!--
  One stored span or log record, whole: GenAI messages, attributes, events,
  exception and the raw row. A tab with nothing recorded names the attribute
  or column that would fill it; nothing is inferred.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { cellText, type RecordDetailProps } from '../loaders.js';
  import { columnIndex } from '../plot.js';
  import ActionButton from './ActionButton.svelte';
  import EmptyState from './EmptyState.svelte';

  let { input, result, actions = {}, onaction }: RecordDetailProps & ActionProps = $props();

  const MESSAGES = ['gen_ai.system_instructions', 'gen_ai.input.messages', 'gen_ai.output.messages'];
  const isLog = $derived('log' in input);
  const data = $derived(result.state === 'populated' ? result.data : undefined);
  const row = $derived(data?.rows[0]);
  const at = (name: string) => (data && row ? row[columnIndex(data, name)] : undefined);
  const attributes = $derived.by(() => {
    const value = at('attributes');
    return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
  });
  const messages = $derived(MESSAGES.filter((key) => attributes[key] !== undefined));
  const exception = $derived(['exception_type', 'exception_message', 'exception_stacktrace'].filter((name) => at(name) !== null && at(name) !== undefined));

  const TABS = $derived(isLog ? (['Body', 'Attributes', 'Exception', 'Raw'] as const) : (['Messages', 'Attributes', 'Events', 'Exception', 'Raw'] as const));
  let chosen = $state<string>();
  const tab = $derived(chosen && (TABS as readonly string[]).includes(chosen) ? chosen : TABS[0]);

  const traceId = $derived(at('trace_id') as string | null | undefined);
  const spanId = $derived(at('span_id') as string | null | undefined);
  const filterValue = $derived((isLog ? at('severity_text') : at('name')) as string | null | undefined);
  const filterColumn = $derived(isLog ? 'severity_text' : 'name');
  const json = (value: unknown) => (typeof value === 'string' ? value : JSON.stringify(value, null, 2));
</script>

{#if result.state === 'populated' && data && row}
  <article class="detail">
    <header>
      <h3>{isLog ? cellText('severity_text', at('severity_text')) : String(at('name'))}</h3>
      <p class="meta">
        {#if traceId}<span>trace {traceId}</span>{/if}
        {#if spanId}<span>span {spanId}</span>{/if}
        <time datetime={String(at(isLog ? 'time_unix_nano' : 'start_time_unix_nano'))}>{String(at(isLog ? 'time_unix_nano' : 'start_time_unix_nano'))}</time>
        {#if data.rows.length > 1}<span class="warn" role="note">{data.rows.length} records share this key; showing the first</span>{/if}
      </p>
      {#if onaction}
      <div class="actions">
        {#if filterValue !== undefined}
          <ActionButton
            label="Filter = {filterColumn}"
            state={actions['record.filter']}
            onclick={() => onaction?.({ name: 'record.filter', input: { signal: isLog ? 'logs' : 'spans', column: filterColumn, value: filterValue } })}
          />
          <ActionButton
            label="Exclude"
            state={actions['record.filter']}
            onclick={() =>
              onaction?.({ name: 'record.filter', input: { signal: isLog ? 'logs' : 'spans', column: filterColumn, value: filterValue, not: true } })}
          />
        {/if}
        {#if !isLog && traceId && spanId}
          <ActionButton
            label="Logs for this span"
            icon="↗"
            state={actions['record.logs']}
            onclick={() => onaction?.({ name: 'record.logs', input: { trace_id: traceId, span_id: spanId } })}
          />
        {:else if isLog && traceId}
          <ActionButton
            label="Open trace"
            icon="↗"
            state={actions['record.open']}
            onclick={() => onaction?.({ name: 'record.open', input: { trace_id: traceId, ...(spanId ? { span_id: spanId } : {}) } })}
          />
        {/if}
      </div>
      {/if}
    </header>

    <div class="tabs" role="tablist" aria-label="Record sections">
      {#each TABS as name (name)}
        <button type="button" role="tab" aria-selected={tab === name} onclick={() => (chosen = name)}>{name}</button>
      {/each}
    </div>

    <div class="panel" role="tabpanel" aria-label={tab}>
      {#if tab === 'Messages'}
        {#if messages.length === 0}
          <EmptyState state="empty" title="No GenAI messages recorded" detail="Recorded under the {MESSAGES.join(', ')} span attributes." />
        {:else}
          {#each messages as key (key)}<h4>{key}</h4><pre>{json(attributes[key])}</pre>{/each}
        {/if}
      {:else if tab === 'Body'}
        {#if at('body') === null || at('body') === undefined}
          <EmptyState state="empty" title="No body recorded" detail="Recorded in the body column." />
        {:else}<pre>{json(at('body'))}</pre>{/if}
      {:else if tab === 'Attributes'}
        {#if Object.keys(attributes).length === 0}
          <EmptyState state="empty" title="No attributes recorded" detail="Recorded in the attributes column." />
        {:else}
          <dl>
            {#each Object.entries(attributes) as [key, value] (key)}<dt>{key}</dt><dd>{json(value)}</dd>{/each}
          </dl>
        {/if}
      {:else if tab === 'Events'}
        {#if !Array.isArray(at('events')) || (at('events') as unknown[]).length === 0}
          <EmptyState state="empty" title="No events recorded" detail="Recorded in the events column." />
        {:else}<pre>{json(at('events'))}</pre>{/if}
      {:else if tab === 'Exception'}
        {#if exception.length === 0}
          <EmptyState state="empty" title="No exception recorded" detail="Recorded in the exception_type, exception_message and exception_stacktrace columns." />
        {:else}
          <dl>
            {#each exception as name (name)}<dt>{name}</dt><dd><pre>{String(at(name))}</pre></dd>{/each}
          </dl>
        {/if}
      {:else}
        <dl>
          {#each data.columns as column, index (column.name)}<dt>{column.name}</dt><dd>{row[index] === null ? '∅' : json(row[index])}</dd>{/each}
        </dl>
      {/if}
    </div>
  </article>
{:else if result.state === 'empty'}
  <EmptyState state="empty" title={result.reason} />
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {'log' in input ? 'log record' : `span ${input.spanId}`}" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .detail {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  h3 {
    margin: 0;
    font: 600 18px/1.2 var(--font-sans);
    overflow-wrap: anywhere;
  }
  h4 {
    margin: 10px 0 4px;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 4px 0 10px;
    color: var(--muted);
    font: 11.5px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .warn {
    color: var(--warn);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0 18px;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    border: 0;
    padding: 8px 2px;
    background: none;
    color: var(--muted);
    font: 13px var(--font-sans);
    cursor: pointer;
  }
  .tabs button[aria-selected='true'] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--declare);
  }
  .tabs button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  dl {
    display: grid;
    grid-template-columns: minmax(120px, max-content) 1fr;
    gap: 6px 14px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  dd {
    margin: 0;
    min-width: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  pre {
    margin: 0;
    border: 1px solid var(--border);
    padding: 10px;
    border-radius: var(--r);
    background: var(--code-bg);
    color: var(--code-text);
    font: 12px/1.5 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  dd pre {
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
  }
  @media (max-width: 560px) {
    dl {
      grid-template-columns: 1fr;
    }
  }
</style>
