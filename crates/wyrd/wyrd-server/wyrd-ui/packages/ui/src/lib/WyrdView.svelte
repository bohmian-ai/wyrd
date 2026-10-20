<!--
  Renders one view document. The document is validated once; each panel is
  validated and loaded on its own, so an invalid or failing panel never blocks
  its siblings. A new document or a brushed range aborts every in-flight load
  and only the current generation may write results.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { fmtInstant, parseInstant } from './format.js';
  import { failed, type ActionIntent, type ActionState, type Executor, type Problem } from './contract.js';
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
  import { ViewDocumentSchema, type AbsoluteRange } from './schemas.js';
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
  }

  let { doc, executor, load, onaction, mode }: Props = $props();

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

  const loader = $derived<PanelLoader>(
    load ??
      ((component, input, context) =>
        executor
          ? loadPanel(component, input, { executor, ...context })
          : Promise.resolve({ code: 'WYRD_UI_NO_EXECUTOR', title: 'This host cannot load panels' }))
  );

  // A changed document drops any brushed range.
  $effect.pre(() => {
    void parsed;
    untrack(() => (brushed = undefined));
  });

  $effect(() => {
    const view = parsed;
    const range = brushed;
    const run = loader;
    untrack(() => start(view, range, run));
    return abortAll;
  });

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
        (error: unknown) =>
          settle(panel.id, controller, failed({ code: 'WYRD_UI_LOAD_FAILED', title: 'Panel failed to load', detail: String(error) }))
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
      panel.actions[valid.name] = {
        state: 'error',
        problem: { code: 'WYRD_UI_ACTION_FAILED', title: 'Action failed', detail: String(error) }
      };
    }
  }
</script>

<div class="wyrd-view" data-mode={mode}>
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
      {#each panels as panel (panel.id)}
        <div class="panel" data-panel={panel.id} data-component={panel.component}>
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
  .panels {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
    gap: 12px;
  }
  .panel {
    min-width: 0;
  }
  .panel[data-component='CardHeader'] {
    grid-column: 1 / -1;
  }
</style>
