<!--
  Renders one view document. The document is validated once; each panel is
  validated and loaded on its own, so an invalid or failing panel never blocks
  its siblings. A new document or a brushed range aborts every in-flight load
  and only the current generation may write results.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { fmtInstant, parseInstant } from './format.js';
  import { failed, wyrdProblem, type ActionIntent, type ActionState, type Executor, type Problem } from './contract.js';
  import {
    inputProblem,
    loadPanel,
    resolveRange,
    validateIntent,
    validatePanel,
    type CardHeaderProps,
    type ChartProps,
    type ComponentName,
    type ComponentProps
  } from './loaders.js';
  import { ViewDocumentSchema, type AbsoluteRange, type Layout, type ViewDocument } from './schemas.js';
  import { COLUMNS, GAP, NARROW, ROW, edit, place, rowsHeight } from './layout.js';
  import ActionButton from './components/ActionButton.svelte';
  import CardHeader from './components/CardHeader.svelte';
  import Chart from './components/Chart.svelte';
  import EmptyState from './components/EmptyState.svelte';
  import type { PanelLoader, ViewActionHandler } from './view.js';

  interface Props {
    /** An untrusted view document. */
    doc: unknown;
    /** Runs reads in this process; ignored when `load` is given. */
    executor?: Executor;
    /** Loads one panel elsewhere, such as through a BFF. */
    load?: PanelLoader;
    /** Performs host-navigation intents; absent, those actions are unavailable. */
    onaction?: ViewActionHandler;
    /** Forces a mode; absent, the host's `[data-mode]` applies. */
    mode?: 'light' | 'dark';
    /** Receives the whole document after each completed layout edit; absent, panels cannot be moved or resized. */
    onlayout?: (doc: ViewDocument) => void;
  }

  let { doc, executor, load, onaction, mode, onlayout }: Props = $props();

  /** One rendered panel. */
  interface PanelView {
    id: string;
    component: ComponentName;
    props?: ComponentProps[ComponentName];
    problem?: Problem;
    actions: Record<string, ActionState>;
  }

  const parsed = $derived(ViewDocumentSchema.safeParse(doc));
  let brushed = $state<AbsoluteRange | undefined>();
  let panels = $state<PanelView[]>([]);
  let controllers: AbortController[] = [];

  /** The host's loader or executor; derived so a host re-passing the same one never reloads panels. */
  const source = $derived(load ?? executor);
  const loader = $derived.by<PanelLoader>(() => {
    const using = source;
    if (typeof using === 'function') return using;
    return (component, input, context) =>
      using
        ? loadPanel(component, input, { executor: using, ...context })
        : Promise.resolve(wyrdProblem('internal', 'This host passed no executor or loader, so it cannot load panels.'));
  });

  /** What panels load from: the range and panels without their layout, so a layout-only change never reloads. */
  const content = $derived(
    parsed.success ? JSON.stringify([parsed.data.range, parsed.data.panels.map(({ layout: _, ...panel }) => panel)]) : undefined
  );

  // A changed document drops any brushed range.
  $effect.pre(() => {
    void content;
    untrack(() => (brushed = undefined));
  });

  $effect(() => {
    void content;
    const range = brushed;
    const run = loader;
    untrack(() => start(parsed, range, run));
    return abortAll;
  });

  /** Rendered width; zero until measured. */
  let width = $state(0);
  const narrow = $derived(width > 0 && width < NARROW);
  const editable = $derived(!!onlayout && !narrow);
  /** Completed edits, shown and reported until the host passes a new document. */
  let kept = $state.raw<Record<string, Layout>>({});
  /** The edit in progress, previewed but not yet reported. */
  let draft = $state.raw<{ id: string; layout: Layout } | undefined>();
  /** Ends the held pointer gesture, if any, without completing it. */
  let release: (() => void) | undefined;

  // A new document drops local edits; leaving ends a held gesture.
  $effect.pre(() => {
    void parsed;
    untrack(() => {
      cancel();
      kept = {};
    });
  });
  $effect(() => () => release?.());

  const layouts = $derived(
    parsed.success ? parsed.data.panels.map((panel) => (draft?.id === panel.id ? draft.layout : (kept[panel.id] ?? panel.layout))) : []
  );
  const placements = $derived(new Map(parsed.success ? place(layouts).map((at, index) => [parsed.data.panels[index].id, at]) : []));
  /** Panels in reading order: document order on the grid, `(y, x)` when stacked. */
  const ordered = $derived(
    narrow ? panels.toSorted((a, b) => { const [p, q] = [placements.get(a.id)!, placements.get(b.id)!]; return p.y - q.y || p.x - q.x; }) : panels
  );

  function boxStyle(id: string) {
    const at = placements.get(id);
    if (!at) return '';
    const column = narrow ? '1 / -1' : `${at.x + 1} / span ${at.w}`;
    const row = narrow ? '' : `grid-row: ${at.y + 1} / span ${at.h};`;
    return `grid-column: ${column}; ${row}${at.auto ? '' : ` height: ${rowsHeight(at.h)};`}`;
  }

  /** The layout an edit of `id` starts from; an unplaced panel starts from its measured height. */
  function current(id: string, element: HTMLElement): Layout {
    if (draft?.id === id) return draft.layout;
    const { x, y, w, h, auto } = placements.get(id)!;
    const height = element.closest<HTMLElement>('.panel')?.offsetHeight ?? 0;
    return { x, y, w, h: auto ? Math.min(48, Math.max(1, Math.round((height + GAP) / (ROW + GAP)))) : h };
  }

  /**
   * Keeps the draft on screen and reports the document with every completed
   * edit, so a host that never passes it back still holds all of them.
   * Collision and measured heights are rendering only and never reported.
   */
  function commit() {
    if (!draft || !parsed.success) return;
    const done = { ...kept, [draft.id]: draft.layout };
    draft = undefined;
    kept = done;
    onlayout?.({ ...parsed.data, panels: parsed.data.panels.map((panel) => (done[panel.id] ? { ...panel, layout: done[panel.id] } : panel)) });
  }

  /** Drops the edit in progress, ending a held pointer gesture so it cannot complete later. */
  function cancel() {
    release?.();
    draft = undefined;
  }

  const KEYS: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };

  /** Arrows move, Shift+arrows resize, Enter completes and Escape cancels the edit. */
  function key(id: string, event: KeyboardEvent) {
    const step = KEYS[event.key];
    if (step) {
      event.preventDefault();
      draft = { id, layout: edit(current(id, event.currentTarget as HTMLElement), step[0], step[1], event.shiftKey) };
    } else if (event.key === 'Enter') {
      event.preventDefault();
      commit();
    } else if (event.key === 'Escape') {
      cancel();
    }
  }

  /**
   * A pointer drag: the handle moves, the corner resizes, both snapped to
   * whole cells. Release completes it; pointercancel or Escape anywhere ends
   * it with nothing reported, whatever has focus.
   */
  function drag(id: string, resize: boolean, event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    cancel();
    const target = event.currentTarget as HTMLElement;
    const from = current(id, target);
    const column = (width + GAP) / COLUMNS;
    const [pointer, startX, startY] = [event.pointerId, event.clientX, event.clientY];
    target.setPointerCapture(pointer);
    const move = (next: PointerEvent) =>
      (draft = { id, layout: edit(from, Math.round((next.clientX - startX) / column), Math.round((next.clientY - startY) / (ROW + GAP)), resize) });
    const escape = (next: KeyboardEvent) => next.key === 'Escape' && cancel();
    const stop = () => {
      release = undefined;
      target.removeEventListener('pointermove', move);
      target.removeEventListener('pointerup', end);
      target.removeEventListener('pointercancel', end);
      window.removeEventListener('keydown', escape);
      if (target.hasPointerCapture?.(pointer)) target.releasePointerCapture(pointer);
    };
    const end = (last: PointerEvent) => {
      stop();
      if (last.type === 'pointerup') commit();
      else draft = undefined;
    };
    release = stop;
    target.addEventListener('pointermove', move);
    target.addEventListener('pointerup', end);
    target.addEventListener('pointercancel', end);
    window.addEventListener('keydown', escape);
  }

  function abortAll() {
    for (const controller of controllers) controller.abort();
    controllers = [];
  }

  function start(view: typeof parsed, override: AbsoluteRange | undefined, run: PanelLoader) {
    abortAll();
    if (!view.success) {
      panels = [];
      return;
    }
    const range = override ?? resolveRange(view.data.range);
    panels = view.data.panels.map((panel) => {
      const valid = validatePanel(panel.component, panel.input);
      if ('code' in valid) return { id: panel.id, component: panel.component as ComponentName, problem: valid, actions: {} };
      const controller = new AbortController();
      controllers.push(controller);
      run(valid.component, valid.input, { range, signal: controller.signal }).then(
        (loaded) => settle(panel.id, controller, 'code' in loaded ? failed(loaded) : loaded),
        (error: unknown) => settle(panel.id, controller, failed(wyrdProblem('internal', `The panel failed to load: ${String(error)}`)))
      );
      const loading = { input: valid.input, result: { state: 'loading' } } as ComponentProps[ComponentName];
      return { id: panel.id, component: valid.component, props: loading, actions: {} };
    });
  }

  /** Accept a load only from a controller of the current generation; a failure keeps the panel's input. */
  function settle(id: string, controller: AbortController, outcome: ComponentProps[ComponentName] | ReturnType<typeof failed>) {
    if (controller.signal.aborted || !controllers.includes(controller)) return;
    const panel = panels.find((each) => each.id === id);
    if (!panel?.props) return;
    panel.props = ('state' in outcome ? { ...panel.props, result: outcome } : outcome) as ComponentProps[ComponentName];
  }

  async function act(panel: PanelView, intent: ActionIntent) {
    if (panel.actions[intent.name]?.state === 'pending') return;
    const valid = validateIntent(panel.component, intent);
    if ('code' in valid) {
      panel.actions[intent.name] = { state: 'error', problem: valid };
      return;
    }
    if (valid.name === 'chart.range') {
      brushed = valid.input as AbsoluteRange;
      return;
    }
    if (!onaction) {
      panel.actions[valid.name] = { state: 'unavailable', reason: 'This host offers no navigation' };
      return;
    }
    panel.actions[valid.name] = { state: 'pending' };
    try {
      panel.actions[valid.name] = (await onaction(valid, panel.id)) ?? { state: 'success' };
    } catch (error) {
      panel.actions[valid.name] = { state: 'error', problem: wyrdProblem('internal', `The host action failed: ${String(error)}`) };
    }
  }
</script>

<div class="wyrd-view" data-mode={mode} bind:clientWidth={width}>
  {#if !parsed.success}
    <EmptyState state="error" problem={inputProblem('View', parsed.error.issues[0])} />
  {:else}
    {#if parsed.data.title}<h2>{parsed.data.title}</h2>{/if}
    {#if brushed}
      <p class="range">
        Range <time datetime={brushed.from}>{fmtInstant(parseInstant(brushed.from))}</time> – <time datetime={brushed.to}>{fmtInstant(parseInstant(brushed.to))}</time>
        <ActionButton label="Reset range" onclick={() => (brushed = undefined)} />
      </p>
    {/if}
    <div class="panels">
      {#each ordered as panel (panel.id)}
        <div class="panel" class:editable class:editing={draft?.id === panel.id} data-panel={panel.id} data-component={panel.component} style={boxStyle(panel.id)}>
          {#if editable}
            <button
              type="button"
              class="move"
              aria-label="Move or resize {panel.id}"
              title="Drag to move. Arrows move, Shift+arrows resize, Enter keeps, Escape cancels."
              onkeydown={(event) => key(panel.id, event)}
              onpointerdown={(event) => drag(panel.id, false, event)}
              onblur={commit}>⠿</button
            >
          {/if}
          <div class="fill">
            {#if panel.problem || !panel.props}
              <EmptyState state="error" problem={panel.problem} />
            {:else}
              {@const onaction = (intent: ActionIntent) => act(panel, intent)}
              {#if panel.component === 'Chart'}
                <Chart {...panel.props as ChartProps} actions={panel.actions} {onaction} />
              {:else}
                <CardHeader {...panel.props as CardHeaderProps} actions={panel.actions} {onaction} />
              {/if}
            {/if}
          </div>
          {#if editable}
            <span class="corner" role="presentation" title="Drag to resize" onpointerdown={(event) => drag(panel.id, true, event)}></span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wyrd-view {
    display: grid;
    gap: 12px;
    min-width: 0;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font-sans);
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 16px var(--font-sans);
  }
  .range {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  /* Keep in step with COLUMNS, ROW and GAP in layout.ts. */
  .panels {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    grid-auto-rows: minmax(40px, auto);
    gap: 12px;
  }
  .panel {
    position: relative;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-width: 0;
  }
  .panel.editable {
    grid-template-rows: auto minmax(0, 1fr);
  }
  .panel.editing {
    outline: 1px dashed var(--declare);
    outline-offset: 2px;
  }
  /* The component fills its box; anything taller scrolls inside the panel. */
  .fill {
    display: grid;
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }
  .move {
    height: 16px;
    border: 0;
    padding: 0;
    background: none;
    color: var(--muted);
    font: 12px/16px var(--font-mono);
    cursor: move;
    touch-action: none;
  }
  .move:hover {
    color: var(--declare);
  }
  .move:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .corner {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 12px;
    height: 12px;
    border-right: 2px solid var(--muted);
    border-bottom: 2px solid var(--muted);
    cursor: nwse-resize;
    touch-action: none;
  }
  .corner:hover {
    border-color: var(--declare);
  }
</style>
