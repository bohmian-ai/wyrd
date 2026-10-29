<!--
  Every retained span of one trace as a tree against the trace's timeline.
  Levels below the second start collapsed; "Expand all" opens every level and
  "Errors only" keeps error spans and their ancestors. Names come from data.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { spanTree, type TraceWaterfallProps } from '../loaders.js';
  import { fmtDuration } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';

  let { input, result, actions = {}, onaction }: TraceWaterfallProps & ActionProps = $props();

  const spans = $derived(result.state === 'populated' ? spanTree(result.data) : []);
  const total = $derived(Math.max(0, ...spans.map((span) => span.startMs + span.durationMs)));
  /** Spans whose children show; `undefined` until the reader toggles one, meaning the first level only. */
  let opened = $state.raw<ReadonlySet<string> | undefined>();
  let errorsOnly = $state(false);
  const open = (span: (typeof spans)[number]) => (opened ? opened.has(span.spanId) : span.depth === 0);
  const allOpen = $derived(spans.every((span) => span.children === 0 || open(span)));

  /** Error spans and every ancestor of one. */
  const erring = $derived.by(() => {
    const parents = new Map(spans.map((span) => [span.spanId, span.depth === 0 ? null : span.parentId]));
    const keep = new Set<string>();
    for (const span of spans.filter((each) => each.error))
      for (let id: string | null | undefined = span.spanId; id && !keep.has(id); id = parents.get(id)) keep.add(id);
    return keep;
  });

  /** Spans on screen: preorder, skipping what lies under a closed span; "Errors only" opens every error path. */
  const visible = $derived.by(() => {
    const shown = [];
    let closedAt = Infinity;
    for (const span of spans) {
      if (!errorsOnly && span.depth > closedAt) continue;
      closedAt = open(span) ? Infinity : span.depth;
      if (!errorsOnly || erring.has(span.spanId)) shown.push(span);
    }
    return shown;
  });

  function toggle(spanId: string) {
    const next = new Set(opened ?? spans.filter((span) => span.depth === 0).map((span) => span.spanId));
    if (!next.delete(spanId)) next.add(spanId);
    opened = next;
  }

  const selectState = $derived(actions['trace.select']);
  const blocked = $derived(!onaction || selectState?.state === 'pending' || selectState?.state === 'unavailable');
</script>

{#if result.state === 'populated'}
  <div class="waterfall">
    <div class="tools">
      <span class="meta">{spans.length} spans · {fmtDuration(Math.round(total))}</span>
      <button
        type="button"
        aria-pressed={allOpen}
        onclick={() => (opened = allOpen ? new Set() : new Set(spans.map((span) => span.spanId)))}>{allOpen ? 'Collapse all' : 'Expand all'}</button
      >
      <button type="button" aria-pressed={errorsOnly} disabled={erring.size === 0} onclick={() => (errorsOnly = !errorsOnly)}>Errors only</button>
      <ActionNote state={selectState} />
    </div>
    <ol role="tree" aria-label="Spans of trace {input.traceId}">
      {#each visible as span (span.spanId)}
        <li
          role="treeitem"
          aria-level={span.depth + 1}
          aria-expanded={span.children > 0 ? open(span) : undefined}
          aria-selected={span.spanId === input.spanId}
          class:error={span.error}
        >
          <span class="name" style:padding-left="{span.depth * 14}px">
            {#if span.children > 0}
              <button type="button" class="twisty" aria-label="{open(span) ? 'Collapse' : 'Expand'} {span.name}" onclick={() => toggle(span.spanId)}
                >{open(span) ? '▾' : '▸'}</button
              >
            {:else}<span class="twisty" aria-hidden="true"></span>{/if}
            <button
              type="button"
              class="select"
              disabled={blocked}
              onclick={() => onaction?.({ name: 'trace.select', input: { trace_id: input.traceId, span_id: span.spanId } })}
            >
              {#if span.error}<span aria-label="error">✕</span>{/if}
              {span.name}
            </button>
          </span>
          <span class="service">{span.service ?? '∅'}</span>
          <span class="track" aria-hidden="true">
            <span
              class="bar"
              style:left="{total > 0 ? (span.startMs / total) * 100 : 0}%"
              style:width="{total > 0 ? Math.max(0.5, (span.durationMs / total) * 100) : 100}%"
            ></span>
          </span>
          <span class="duration">{fmtDuration(Math.round(span.durationMs))}</span>
        </li>
      {/each}
    </ol>
  </div>
{:else if result.state === 'empty'}
  <EmptyState state="empty" title={result.reason} />
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading trace {input.traceId}" />
{:else}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .waterfall {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .meta {
    margin-right: auto;
    color: var(--muted);
    font: 11.5px var(--font-mono);
  }
  .tools button {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 3px 9px;
    background: var(--surface-2);
    color: var(--text);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .tools button[aria-pressed='true'] {
    border-color: var(--declare);
    color: var(--declare);
  }
  .tools button:disabled {
    color: var(--muted);
    cursor: default;
  }
  ol {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: minmax(140px, 2fr) minmax(70px, 1fr) minmax(80px, 3fr) 64px;
    align-items: center;
    gap: 10px;
    padding: 4px 10px;
    border-top: 1px solid var(--border-soft);
    font: 12px var(--font-mono);
  }
  li:first-child {
    border-top: 0;
  }
  li[aria-selected='true'] {
    background: var(--declare-soft);
  }
  .name {
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .twisty {
    flex: none;
    width: 16px;
    border: 0;
    padding: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .select {
    min-width: 0;
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .select:disabled {
    cursor: default;
  }
  .error .select {
    color: var(--danger);
  }
  .service {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    position: relative;
    height: 8px;
    background: var(--surface-2);
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--declare);
  }
  .error .bar {
    background: var(--danger);
  }
  .duration {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 1px;
  }
  @media (max-width: 560px) {
    li {
      grid-template-columns: minmax(0, 1fr) 64px;
    }
    .service,
    .track {
      display: none;
    }
  }
</style>
