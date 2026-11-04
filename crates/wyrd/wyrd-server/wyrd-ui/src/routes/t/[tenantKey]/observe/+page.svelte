<!--
  The open record of the Observe view: a trace's waterfall and selected span
  in the drawer, or a log record beside the list inside the Logs work region.
  The log record's left edge drags, or with the arrow keys steps, its width
  between 320px and leaving the list 320px; it keeps that width
  while the page stays open.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { Drawer, EmptyState, RecordDetail, StatusPill, TraceWaterfall, fmtCount, fmtDuration, type ActionIntent, type ActionState } from 'wyrd-ai-ui';
  import { cellText, shortId, spanTree, validateIntent, type ComponentName } from 'wyrd-ai-ui/loaders';
  import { closed, moved, readState, step, writeState, type ObserveState } from '$lib/observe-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const current = $derived(readState(page.url.searchParams));
  const go = (next: ObserveState) => goto(`${base}/observe?${writeState(moved(current, next))}`, { keepFocus: true, noScroll: true });

  /** The log record's dragged width in pixels; absent, its default width. */
  let width = $state<number>();
  /** The widest the log record may be: its right edge less the list's left edge, 320px of list and the 16px gap. */
  function widest(aside: HTMLElement) {
    const list = aside.previousElementSibling?.getBoundingClientRect();
    return list ? aside.getBoundingClientRect().right - list.left - 336 : Infinity;
  }
  function drag(event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement;
    const aside = handle.parentElement as HTMLElement;
    const { right } = aside.getBoundingClientRect();
    const most = widest(aside);
    handle.setPointerCapture(event.pointerId);
    handle.onpointermove = (move) => (width = Math.round(Math.max(320, Math.min(right - move.clientX, most))));
    handle.onpointerup = () => (handle.onpointermove = handle.onpointerup = null);
  }
  function nudge(event: KeyboardEvent) {
    const aside = (event.currentTarget as HTMLElement).parentElement as HTMLElement;
    const by = event.key === 'ArrowLeft' ? 32 : event.key === 'ArrowRight' ? -32 : 0;
    if (!by) return;
    event.preventDefault();
    width = Math.round(Math.max(320, Math.min(aside.getBoundingClientRect().width + by, widest(aside))));
  }

  let actions = $state<Record<string, ActionState>>({});
  function onaction(component: ComponentName) {
    return (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      const next = step(current, valid, `${page.url.pathname}${page.url.search}`);
      if ('unavailable' in next) return void (actions[valid.name] = { state: 'unavailable', reason: next.unavailable });
      delete actions[valid.name];
      if ('query' in next) return void goto(`${base}/query?${next.query}`);
      void go(next.state);
    };
  }

  /** The open trace's root and totals for the drawer header, from the spans it read. */
  const trace = $derived.by(() => {
    const read = data.waterfall && !('code' in data.waterfall) ? data.waterfall.result : undefined;
    if (read?.state !== 'populated') return undefined;
    const spans = spanTree(read.data);
    const root = spans.find((span) => span.depth === 0);
    const start = read.data.columns.findIndex((column) => column.name === 'start_time_unix_nano');
    const first = read.data.rows.map((row) => String(row[start])).sort()[0];
    return {
      root,
      start: first === undefined ? undefined : cellText('start_time_unix_nano', first),
      duration: Math.max(0, ...spans.map((span) => span.startMs + span.durationMs)),
      spans: spans.length,
      errors: spans.filter((span) => span.error).length
    };
  });
  /** Expanded fills the viewport; otherwise the drawer leaves the list beside it. */
  let expanded = $state(false);
  let copied = $state(false);
  async function link() {
    try {
      await navigator.clipboard.writeText(page.url.href);
      copied = true;
    } catch {
      copied = false;
    }
  }
</script>

{#if data.waterfall}
  <Drawer open title="Trace" width={expanded ? '100vw' : '884px'} onclose={() => go(closed(current))}>
    {#snippet heading()}
      <div class="trace">
        <p class="root">
          <strong>{trace?.root?.name ?? 'Trace'}</strong>
          {#if trace?.errors}<StatusPill tone="danger" label="error" />{/if}
          {#if trace?.root?.service}<span class="chip">{trace.root.service}</span>{/if}
        </p>
        <p class="facts">
          trace <span title={current.trace}>{shortId(current.trace ?? '')}</span>{#if trace}{#if trace.start}{' '}· {trace.start}{/if} · {fmtDuration(Math.round(trace.duration))} · {fmtCount(trace.spans)} spans · {fmtCount(trace.errors)} {trace.errors === 1 ? 'error' : 'errors'}{/if}
        </p>
      </div>
    {/snippet}
    {#snippet tools()}
      <button type="button" onclick={link}>{copied ? 'Link copied' : 'Link'}</button>
      <button type="button" aria-pressed={expanded} onclick={() => (expanded = !expanded)}>{expanded ? 'Collapse' : 'Expand'} ↗</button>
    {/snippet}
    {#if 'code' in data.waterfall}<EmptyState state="error" problem={data.waterfall} />{:else}<TraceWaterfall {...data.waterfall} {actions} onaction={onaction('TraceWaterfall')} />{/if}
    <div class="span">{@render record()}</div>
  </Drawer>
{:else if data.record}
  <aside class="record" class:sized={width} aria-label="Log record" style:flex={width ? `0 0 ${width}px` : undefined} style:max-width={width ? 'none' : undefined}>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -- a focusable separator is the ARIA window splitter. -->
    <div class="resize" role="separator" aria-orientation="vertical" aria-label="Resize log record" aria-valuenow={width} tabindex="0" onpointerdown={drag} onkeydown={nudge}></div>
    <header>
      <p>Log record</p>
      <a href={`${base}/observe?${writeState(closed(current))}`} aria-label="Close log record" data-sveltekit-noscroll>✕</a>
    </header>
    <div class="body">{@render record()}</div>
  </aside>
{/if}

{#snippet record()}
  {#if data.record && 'code' in data.record}<EmptyState state="error" problem={data.record} />{:else if data.record}<RecordDetail {...data.record} {actions} onaction={onaction('RecordDetail')} />{/if}
{/snippet}

<style>
  .span {
    margin: 16px -16px 0;
    padding: 16px 16px 0;
    border-top: 1px solid var(--border);
  }
  .trace p {
    margin: 0;
  }
  .root {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font: var(--weight-strong) 16px var(--font-sans);
  }
  .chip {
    padding: 0 6px;
    border: 1px solid var(--declare);
    border-radius: var(--r);
    color: var(--declare);
    font: 11px var(--font-mono);
  }
  .trace .facts {
    margin-top: 4px;
    overflow-wrap: anywhere;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  /* Beside the records in the Logs work region; alone on its own line when the screen is narrow. */
  .record {
    flex: 1 1 360px;
    min-width: 0;
    max-width: 400px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  header {
    display: flex;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  header p {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  header a {
    color: var(--muted);
    text-decoration: none;
  }
  .record {
    position: relative;
  }
  /* The draggable left edge: a rule that turns Declare on hover, focus or drag. */
  .resize {
    position: absolute;
    inset: 0 auto 0 -4px;
    z-index: 1;
    width: 8px;
    cursor: col-resize;
    touch-action: none;
  }
  .resize:focus-visible {
    outline: none;
  }
  .resize:hover::after,
  .resize:focus-visible::after,
  .resize:active::after {
    content: '';
    position: absolute;
    inset: 0 3px;
    background: var(--declare);
  }
  .record .body {
    padding: 12px 14px;
  }
  @media (max-width: 899px) {
    .record {
      max-width: none;
    }
  }
</style>
