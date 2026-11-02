<!--
  One stored span or log record, whole. A span reads as GenAI messages beside
  its other attributes, with Attributes, Events, Exception and Raw tabs counted
  from what was recorded; a log record reads as its body and attributes. A
  recorded exception heads either. Message roles and parts come from the
  recorded message attributes; a value that is not the GenAI message shape
  shows as recorded. A section with nothing recorded names the attribute or
  column that would fill it; nothing is inferred.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { cellText, type RecordDetailProps } from '../loaders.js';
  import { fmtDuration } from '../format.js';
  import { columnIndex, numeric } from '../plot.js';
  import ActionButton from './ActionButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill from './StatusPill.svelte';

  let { input, result, actions = {}, onaction }: RecordDetailProps & ActionProps = $props();

  const MESSAGES = ['gen_ai.system_instructions', 'gen_ai.input.messages', 'gen_ai.output.messages'];
  const isLog = $derived('log' in input);
  const data = $derived(result.state === 'populated' ? result.data : undefined);
  const row = $derived(data?.rows[0]);
  const at = (name: string) => (data && row && columnIndex(data, name) >= 0 ? row[columnIndex(data, name)] : undefined);
  const attributes = $derived.by(() => {
    const value = at('attributes');
    return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
  });
  const others = $derived(Object.entries(attributes).filter(([key]) => !MESSAGES.includes(key)));
  const events = $derived(Array.isArray(at('events')) ? (at('events') as unknown[]) : []);
  const exception = $derived(['exception_type', 'exception_message', 'exception_stacktrace'].filter((name) => at(name) !== null && at(name) !== undefined));
  const headline = $derived([at('exception_type'), at('exception_message')].filter((value) => value !== null && value !== undefined).join(': '));

  /** One part of a message: text reads as prose, anything else as its recorded JSON. */
  interface Part {
    readonly text: string;
    readonly mono: boolean;
  }
  /** One heading and its parts, in recorded order. */
  interface Block {
    readonly label: string;
    readonly parts: readonly Part[];
  }
  const json = (value: unknown) => (typeof value === 'string' ? value : JSON.stringify(value, null, 2));
  const part = (value: unknown): Part => {
    const each = value as { type?: unknown; content?: unknown; arguments?: unknown; response?: unknown; result?: unknown };
    if (each && typeof each === 'object' && each.type === 'text' && typeof each.content === 'string') return { text: each.content, mono: false };
    const payload = each?.arguments ?? each?.response ?? each?.result;
    return { text: json(payload === undefined ? value : payload), mono: true };
  };
  /** The blocks one message attribute recorded; JSON text is parsed, anything unparseable shows whole under its key. */
  function blocks(key: string, recorded: unknown): Block[] {
    let value = recorded;
    if (typeof value === 'string') {
      try {
        value = JSON.parse(value);
      } catch {
        return [{ label: key, parts: [{ text: recorded as string, mono: false }] }];
      }
    }
    if (!Array.isArray(value)) return [{ label: key, parts: [{ text: json(value), mono: true }] }];
    if (key === 'gen_ai.system_instructions') return [{ label: 'system instructions', parts: value.map(part) }];
    return value.map((message) => {
      const { role, parts, content } = (message ?? {}) as { role?: unknown; parts?: unknown; content?: unknown };
      const call = Array.isArray(parts) ? parts.find((each) => each?.type === 'tool_call' && typeof each.name === 'string') : undefined;
      const label = `${typeof role === 'string' ? role : key}${call ? ` · ${call.name}` : ''}`;
      if (Array.isArray(parts)) return { label, parts: parts.map(part) };
      if (typeof content === 'string') return { label, parts: [{ text: content, mono: false }] };
      return { label, parts: [{ text: json(message), mono: true }] };
    });
  }
  const messages = $derived(MESSAGES.filter((key) => attributes[key] !== undefined).flatMap((key) => blocks(key, attributes[key])));

  const TABS = ['Messages', 'Attributes', 'Events', 'Exception', 'Raw'] as const;
  const counts = $derived<Record<string, number>>({ Messages: messages.length, Attributes: Object.keys(attributes).length, Events: events.length });
  let chosen = $state<string>('Messages');

  const traceId = $derived(at('trace_id') as string | null | undefined);
  const spanId = $derived(at('span_id') as string | null | undefined);
  const erring = $derived(!isLog && at('status_code') !== undefined && numeric(at('status_code')) === 2);
  /** OTLP severity: 13–16 warn, 17 and above error or fatal. */
  const severity = $derived.by(() => {
    const number = at('severity_number');
    if (number === null || number === undefined) return undefined;
    return numeric(number) >= 17 ? 'danger' : numeric(number) >= 13 ? 'warn' : undefined;
  });
  const filterColumn = $derived(isLog ? 'service_name' : 'name');
  const filterValue = $derived(at(filterColumn) as string | null | undefined);
  const instant = $derived(at(isLog ? 'time_unix_nano' : 'start_time_unix_nano'));
</script>

{#snippet attributeList(entries: [string, unknown][])}
  {#if entries.length === 0}
    <EmptyState state="empty" title="No attributes recorded" detail="Recorded in the attributes column." />
  {:else}
    <dl>
      {#each entries as [key, value] (key)}<dt>{key}</dt><dd>{json(value)}</dd>{/each}
    </dl>
  {/if}
{/snippet}

{#snippet raw()}
  <dl>
    {#each data?.columns ?? [] as column, index (column.name)}<dt>{column.name}</dt><dd>{row?.[index] === null ? '∅' : json(row?.[index])}</dd>{/each}
  </dl>
{/snippet}

{#if result.state === 'populated' && data && row}
  <article class="detail">
    <header>
      {#if isLog}
        <h3 class="severity" data-tone={severity}>{cellText('severity_text', at('severity_text'))}</h3>
      {:else}
        <h3 class:error={erring}>{String(at('name'))}</h3>
        {#if erring}<StatusPill tone="danger" label="error" />{/if}
      {/if}
      <p class="meta">
        {#if traceId}<span>trace {traceId}</span>{/if}
        {#if spanId}<span>span {spanId}</span>{/if}
        {#if !isLog && at('duration_nano') !== null && at('duration_nano') !== undefined}<span>{fmtDuration(Math.round(numeric(at('duration_nano')) / 1e6))}</span>{/if}
        {#if at('service_name') !== null && at('service_name') !== undefined}<span>{String(at('service_name'))}</span>{/if}
        <time datetime={String(instant)} title={String(instant)}>{cellText(isLog ? 'time_unix_nano' : 'start_time_unix_nano', instant)} UTC</time>
      </p>
      {#if data.rows.length > 1}<p class="warn" role="note">{data.rows.length} records share this key; showing the first</p>{/if}
    </header>

    {#if headline}<p class="exception" role="note">{headline}</p>{/if}

    {#if isLog}
      {#if at('body') === null || at('body') === undefined}
        <EmptyState state="empty" title="No body recorded" detail="Recorded in the body column." />
      {:else}<pre class="body">{json(at('body'))}</pre>{/if}
      <h4>Attributes</h4>
      {@render attributeList(Object.entries(attributes))}
      <details>
        <summary>Raw</summary>
        {@render raw()}
      </details>
    {:else}
      <div class="tabs" role="tablist" aria-label="Record sections">
        {#each TABS as name (name)}
          <button type="button" role="tab" aria-selected={chosen === name} onclick={() => (chosen = name)}>{name}{counts[name] ? ` ${counts[name]}` : ''}</button>
        {/each}
      </div>

      <div class="panel" role="tabpanel" aria-label={chosen}>
        {#if chosen === 'Messages'}
          <div class="messages">
            <div class="blocks">
              {#if messages.length === 0}
                <EmptyState state="empty" title="No GenAI messages recorded" detail="Recorded under the {MESSAGES.join(', ')} span attributes." />
              {:else}
                {#each messages as block, index (index)}
                  <h4>{block.label}</h4>
                  {#each block.parts as each, position (position)}<div class="msg" class:mono={each.mono}>{each.text}</div>{/each}
                {/each}
              {/if}
            </div>
            {#if others.length > 0}
              <dl class="side" aria-label="Other attributes">
                {#each others as [key, value] (key)}<dt>{key}</dt><dd title={json(value)}>{json(value)}</dd>{/each}
              </dl>
            {/if}
          </div>
        {:else if chosen === 'Attributes'}
          {@render attributeList(Object.entries(attributes))}
        {:else if chosen === 'Events'}
          {#if events.length === 0}
            <EmptyState state="empty" title="No events recorded" detail="Recorded in the events column." />
          {:else}<pre>{json(events)}</pre>{/if}
        {:else if chosen === 'Exception'}
          {#if exception.length === 0}
            <EmptyState state="empty" title="No exception recorded" detail="Recorded in the exception_type, exception_message and exception_stacktrace columns." />
          {:else}
            <dl>
              {#each exception as name (name)}<dt>{name}</dt><dd><pre>{String(at(name))}</pre></dd>{/each}
            </dl>
          {/if}
        {:else}
          {@render raw()}
        {/if}
      </div>
    {/if}

    {#if onaction}
      <div class="actions">
        {#if filterValue !== undefined}
          <ActionButton
            label="Filter = {filterValue ?? 'not set'}"
            state={actions['record.filter']}
            onclick={() => onaction?.({ name: 'record.filter', input: { signal: isLog ? 'logs' : 'spans', column: filterColumn, value: filterValue } })}
          />
          <ActionButton
            label="Exclude"
            state={actions['record.filter']}
            onclick={() => onaction?.({ name: 'record.filter', input: { signal: isLog ? 'logs' : 'spans', column: filterColumn, value: filterValue, not: true } })}
          />
        {/if}
        {#if !isLog && traceId && spanId}
          <ActionButton
            label="Logs for this span"
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
    gap: 10px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
  }
  h3 {
    margin: 0;
    font: 13px/1.3 var(--font-mono);
    overflow-wrap: anywhere;
  }
  h3.error,
  [data-tone='danger'] {
    color: var(--danger);
  }
  [data-tone='warn'] {
    color: var(--warn);
  }
  h4 {
    margin: 4px 0 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .warn {
    flex-basis: 100%;
    margin: 0;
    color: var(--warn);
    font: 12px var(--font-sans);
  }
  .exception {
    margin: 0;
    border: 1px solid color-mix(in srgb, var(--danger) 40%, var(--border));
    padding: 8px 10px;
    background: var(--danger-soft);
    color: var(--danger);
    font: 12px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    border: 0;
    padding: 8px 14px;
    background: none;
    color: var(--muted);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  .tabs button[aria-selected='true'] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--declare);
  }
  .tabs button:focus-visible,
  summary:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .messages {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }
  .blocks {
    display: flex;
    flex: 999 1 420px;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .msg {
    border: 1px solid var(--border);
    padding: 10px 12px;
    background: var(--bg);
    font: 13px/1.5 var(--font-sans);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .msg.mono {
    font: 12px/1.5 var(--font-mono);
  }
  dl {
    display: grid;
    grid-template-columns: minmax(120px, max-content) 1fr;
    gap: 6px 14px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  .side {
    flex: 1 1 240px;
    align-content: start;
    grid-template-columns: minmax(0, max-content) minmax(0, 1fr);
  }
  .side dd {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .body {
    border-radius: 0;
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
  }
  dd pre {
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
  }
  summary {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    cursor: pointer;
  }
  details[open] summary {
    margin-bottom: 8px;
  }
  @media (max-width: 560px) {
    dl {
      grid-template-columns: 1fr;
    }
  }
</style>
