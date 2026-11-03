<!--
  Every retained span of one trace as a tree against the trace's timeline.
  Levels below the second start collapsed, except the path to the selected
  span; a collapsed span shows how many children it holds. "Expand all" opens
  every level and "Errors only" keeps error spans and their ancestors. An axis
  marks round offsets from the trace start up to its end. Names come from
  data and never colour a span: each recorded service takes the series
  palette's colour by first appearance, named in the Services key, and an
  error span takes the failure colour beside its ✕ by recorded status alone.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { spanTree, type TraceWaterfallProps } from '../loaders.js';
  import { fmtDuration } from '../format.js';
  import { SERIES_COLORS } from '../plot.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';

  let { input, result, actions = {}, onaction }: TraceWaterfallProps & ActionProps = $props();

  const spans = $derived(result.state === 'populated' ? spanTree(result.data) : []);
  const total = $derived(Math.max(0, ...spans.map((span) => span.startMs + span.durationMs)));
  /** Each recorded service, in order of first appearance; an unrecorded service is its own entry. */
  const services = $derived([...new Set(spans.map((span) => span.service ?? null))]);
  const serviceColor = (service: string | null | undefined) => SERIES_COLORS[services.indexOf(service ?? null) % SERIES_COLORS.length];
  /** Spans whose children show; `undefined` until the reader toggles one, meaning the first level only. */
  let opened = $state.raw<ReadonlySet<string> | undefined>();
  let errorsOnly = $state(false);
  /** The selected span's ancestors: open by default so a deep selection is never hidden. */
  const path = $derived.by(() => {
    const parents = new Map(spans.map((span) => [span.spanId, span.depth === 0 ? null : span.parentId]));
    const ancestors = new Set<string>();
    for (let id = parents.get(input.spanId ?? ''); id && !ancestors.has(id); id = parents.get(id)) ancestors.add(id);
    return ancestors;
  });
  const open = (span: (typeof spans)[number]) => (opened ? opened.has(span.spanId) : span.depth === 0 || path.has(span.spanId));
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
    const next = new Set(opened ?? spans.filter(open).map((span) => span.spanId));
    if (!next.delete(spanId)) next.add(spanId);
    opened = next;
  }

  /** An axis offset: milliseconds below a second, else seconds without trailing zeros. */
  const axisText = (ms: number) => (ms > 0 && ms < 1000 ? `${Math.round(ms)}ms` : `${Number((ms / 1000).toFixed(2))}s`);
  /** Round offsets from the start, about four of them, then the trace's end. */
  const ticks = $derived.by(() => {
    if (total <= 0) return [];
    const rough = total / 4;
    const power = 10 ** Math.floor(Math.log10(rough));
    const step = ([1, 2, 5, 10].find((each) => each * power >= rough) ?? 10) * power;
    const marks = [];
    for (let at = 0; at < total * 0.9; at += step) marks.push(at);
    return [...marks, total];
  });

  const selectState = $derived(actions['trace.select']);
  const blocked = $derived(!onaction || selectState?.state === 'pending' || selectState?.state === 'unavailable');
</script>

{#if result.state === 'populated'}
  <div class="waterfall">
    <div class="tools">
      <span class="caption">Waterfall · collapsed below 2 levels · selected path stays open</span>
      <button
        type="button"
        aria-pressed={allOpen}
        onclick={() => (opened = allOpen ? new Set() : new Set(spans.map((span) => span.spanId)))}>{allOpen ? 'Collapse all' : 'Expand all'}</button
      >
      <button type="button" aria-pressed={errorsOnly} disabled={erring.size === 0} onclick={() => (errorsOnly = !errorsOnly)}>Errors only</button>
      <ActionNote state={selectState} />
    </div>
    <ul class="key" aria-label="Services">
      {#each services as service (service)}<li><i style:background={serviceColor(service)}></i>{service ?? '∅'}</li>{/each}
      {#if spans.some((span) => span.error)}<li><i style:background="var(--danger)"></i>✕ error</li>{/if}
    </ul>
    <div class="row axis" aria-hidden="true">
      <span></span><span class="service"></span>
      <span class="ticks">
        {#each ticks as at, index (index)}
          <span class:first={index === 0} class:last={index === ticks.length - 1} style:left="{(at / total) * 100}%">{axisText(at)}</span>
        {/each}
      </span>
      <span></span>
    </div>
    <ol role="tree" aria-label="Spans of trace {input.traceId}">
      {#each visible as span (span.spanId)}
        <li
          role="treeitem"
          class="row"
          aria-level={span.depth + 1}
          aria-expanded={span.children > 0 ? open(span) : undefined}
          aria-selected={span.spanId === input.spanId}
          class:error={span.error}
        >
          <span class="name">
            {#each { length: span.depth } as _, level (level)}<span class="guide" aria-hidden="true"></span>{/each}
            {#if span.children > 0}
              <button type="button" class="twisty" aria-label="{open(span) ? 'Collapse' : 'Expand'} {span.name}" onclick={() => toggle(span.spanId)}
                >{open(span) ? '▾' : '▸'}</button
              >
            {:else}<span class="twisty" aria-hidden="true"></span>{/if}
            <button
              type="button"
              class="select"
              title={span.name}
              disabled={blocked}
              onclick={() => onaction?.({ name: 'trace.select', input: { trace_id: input.traceId, span_id: span.spanId } })}
            >
              {#if span.error}<span aria-label="error">✕</span>{/if}
              {span.name}
            </button>
            {#if span.children > 0 && !open(span) && !errorsOnly}<span class="collapsed" title="{span.children} child spans collapsed">+{span.children}</span>{/if}
          </span>
          <span class="service">{span.service ?? '∅'}</span>
          <span class="track" aria-hidden="true">
            <span
              class="bar"
              style:background={span.error ? 'var(--danger)' : serviceColor(span.service)}
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
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 0;
  }
  .caption {
    margin-right: auto;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .tools button {
    height: 26px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 10px;
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .tools button:enabled:hover {
    background: var(--surface-hover);
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
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 300px) 110px minmax(80px, 1fr) 64px;
    align-items: center;
    gap: 10px;
    height: 28px;
    padding: 0 16px 0 14px;
    border-left: 2px solid transparent;
    font: 12px var(--font-mono);
  }
  li:hover {
    background: var(--surface-hover);
  }
  li[aria-selected='true'] {
    border-left-color: var(--declare);
    background: var(--declare-soft);
  }
  .axis {
    height: 20px;
    color: var(--muted);
    font-size: 11px;
  }
  .ticks {
    position: relative;
    height: 100%;
  }
  .ticks span {
    position: absolute;
    top: 3px;
    transform: translateX(-50%);
    white-space: nowrap;
  }
  .ticks .first {
    transform: none;
  }
  .ticks .last {
    transform: translateX(-100%);
  }
  .name {
    display: flex;
    align-items: center;
    height: 100%;
    min-width: 0;
    white-space: nowrap;
  }
  .guide {
    flex: none;
    align-self: stretch;
    width: 14px;
    border-left: 1px solid var(--border-soft);
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
  .collapsed {
    flex: none;
    margin-left: 6px;
    color: var(--muted);
  }
  .error .select,
  .error .duration {
    color: var(--danger);
  }
  .service {
    color: var(--muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    position: relative;
    height: 6px;
    background: var(--surface-2);
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
  }
  .key {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 0 0 6px;
    padding: 0;
    list-style: none;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .key i {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
  }
  .duration {
    color: var(--muted);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 1px;
  }
  /* A narrow panel keeps the tree and timeline and drops the service; a phone keeps the tree. */
  @container (max-width: 560px) {
    .row {
      grid-template-columns: minmax(110px, 3fr) minmax(50px, 2fr) 56px;
    }
    .service {
      display: none;
    }
    /* A short track keeps only its two end labels, so the ticks never overprint. */
    .ticks span:not(.first, .last) {
      display: none;
    }
  }
  @container (max-width: 280px) {
    .row {
      grid-template-columns: minmax(0, 1fr) 56px;
    }
    .track,
    .ticks {
      display: none;
    }
  }
</style>
