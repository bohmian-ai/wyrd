<!--
  A Verifier's declared graph: inputs, steps with their attached requirements
  and whole-Verifier requirements, under stage headings, with labelled
  dependency arrows. Read-only: nothing drags, connects or deletes, and no
  execution state is drawn. Laid out once from topology; selection, focus and
  zoom never move a node.
-->
<script lang="ts">
  import { Position, SvelteFlow, type Edge, type Node, type Viewport } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/base.css';
  import { onMount, setContext } from 'svelte';
  import type { DefinitionGraph } from '../verifier.js';
  import { HEAD, layoutDefinition, needsText } from '../verifier-layout.js';
  import Panel from './Panel.svelte';
  import VerifierGraphEdge from './VerifierGraphEdge.svelte';
  import VerifierGraphNode, { SELECTION, type GraphNodeData, type GraphSelection } from './VerifierGraphNode.svelte';

  interface Props {
    /** The declared graph. Re-key this component to draw another one. */
    graph: DefinitionGraph;
    /** The selected node key (`input:`, `step:` or `req:` plus the id); empty for none. */
    selected?: string;
  }

  let { graph, selected = $bindable('') }: Props = $props();

  // Laid out once: the arrays below are never replaced, so nothing re-flows after mount.
  // svelte-ignore state_referenced_locally
  const laid = layoutDefinition(graph);
  const fixed = { draggable: false, connectable: false, selectable: false, deletable: false, focusable: false };
  // svelte-ignore state_referenced_locally
  const declared = new Map<string, GraphNodeData>([
    ...graph.inputs.map((node) => [node.key, { type: 'input', node }] as const),
    ...graph.steps.map((node) => [node.key, { type: 'step', node, needs: needsText(node) }] as const),
    ...graph.requirements.map((node) => [node.key, { type: 'req', node }] as const)
  ]);
  // Sizes and handle positions are known from layout, so edges draw before (and without) DOM measurement.
  const handles = (width: number, height: number) => [
    { type: 'target' as const, position: Position.Left, x: 0, y: height / 2, width: 1, height: 1 },
    { type: 'source' as const, position: Position.Right, x: width, y: height / 2, width: 1, height: 1 }
  ];
  let nodes = $state.raw<Node[]>([
    ...laid.stages.map((stage) => ({ id: `stage:${stage.text}`, type: 'definition', class: 'stage', data: { type: 'stage', text: stage.text }, position: { x: stage.x, y: 0 }, width: Math.max(stage.width, 1), height: HEAD - 16, ...fixed })),
    ...laid.nodes.map((node) => ({ id: node.key, type: 'definition', data: declared.get(node.key) as GraphNodeData, position: { x: node.x, y: node.y }, width: node.width, height: node.height, measured: { width: node.width, height: node.height }, handles: handles(node.width, node.height), ...fixed }))
  ]);
  let edges = $state.raw<Edge[]>(laid.edges.map((edge) => ({ id: edge.id, source: edge.source, target: edge.target, type: 'dependency', data: { ...edge }, selectable: false, deletable: false, focusable: false })));

  const nodeTypes = { definition: VerifierGraphNode };
  const edgeTypes = { dependency: VerifierGraphEdge };
  const PAD = 16;
  const MIN = 0.6;
  const MAX = 1.6;

  let viewport = $state<Viewport>({ x: PAD, y: 12, zoom: 1 });
  let host = $state<HTMLDivElement>();
  let height = $state(0);

  setContext<GraphSelection>(SELECTION, {
    get selected() {
      return selected;
    },
    select: (key) => (selected = key)
  });

  /** The canvas width, or the graph's own when the host has not been laid out. */
  const width = () => host?.clientWidth || laid.width + 2 * PAD;
  /** Fit: the whole graph at up to 100%, never below the minimum; a narrower panel pans instead. */
  const fit = (): Viewport => ({ x: PAD, y: 12, zoom: Math.max(MIN, Math.min(1, (width() - 2 * PAD) / laid.width)) });
  /** Zoom around the centre of the canvas. */
  function zoomTo(zoom: number) {
    const next = Math.min(MAX, Math.max(MIN, zoom));
    const cx = width() / 2;
    const cy = height / 2;
    viewport = { x: cx - ((cx - viewport.x) / viewport.zoom) * next, y: cy - ((cy - viewport.y) / viewport.zoom) * next, zoom: next };
  }

  // The panel is sized to the laid-out graph once, so a simple graph gets a compact panel.
  onMount(() => {
    viewport = fit();
    height = Math.round(Math.min(640, Math.max(132, laid.height * viewport.zoom + 30)));
  });

  /** Pan just far enough to bring a graph point inside the canvas. */
  function reveal(point: { x: number; y: number }) {
    const margin = 48;
    const pan = (at: number, size: number) => (at < margin ? margin - at : at > size - margin ? size - margin - at : 0);
    const dx = pan(point.x * viewport.zoom + viewport.x, width());
    const dy = pan(point.y * viewport.zoom + viewport.y, height);
    if (dx || dy) viewport = { ...viewport, x: viewport.x + dx, y: viewport.y + dy };
  }

  // Arrow keys move between node buttons by layout position; moving selects, like a listbox.
  function onkeydown(event: KeyboardEvent) {
    const step = { ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowDown: [0, 1], ArrowUp: [0, -1] }[event.key];
    const from = laid.targets.find((target) => target.key === (event.target as Element).closest?.('[data-node]')?.getAttribute('data-node'));
    if (!step || !from) return;
    event.preventDefault();
    let best: (typeof laid.targets)[number] | undefined;
    let bestScore = Infinity;
    for (const target of laid.targets) {
      const along = (target.x - from.x) * step[0] + (target.y - from.y) * step[1];
      const across = Math.abs((target.x - from.x) * step[1] + (target.y - from.y) * step[0]);
      if (along > 4 && along + across * 2 < bestScore) [best, bestScore] = [target, along + across * 2];
    }
    if (!best) return;
    selected = best.key;
    reveal(best);
    host?.querySelector<HTMLElement>(`[data-node="${CSS.escape(best.key)}"]`)?.focus({ preventScroll: true });
  }
</script>

<Panel title="Graph" note="Declared dependencies. No execution state is shown.">
  {#snippet tools()}
    <span class="zoom" role="group" aria-label="Zoom">
      <span class="value" data-zoom>{Math.round(viewport.zoom * 100)}%</span>
      <button type="button" aria-label="Zoom out" disabled={viewport.zoom <= MIN} onclick={() => zoomTo(viewport.zoom / 1.2)}>−</button>
      <button type="button" aria-label="Zoom in" disabled={viewport.zoom >= MAX} onclick={() => zoomTo(viewport.zoom * 1.2)}>+</button>
      <button type="button" onclick={() => (viewport = fit())}>Fit</button>
      <button type="button" onclick={() => zoomTo(1)}>100%</button>
    </span>
  {/snippet}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="graph flush" style:height="{height}px" bind:this={host} data-graph-width={Math.round(laid.width)} {onkeydown}>
    {#if height}
      <SvelteFlow
        bind:nodes
        bind:edges
        bind:viewport
        {nodeTypes}
        {edgeTypes}
        minZoom={MIN}
        maxZoom={MAX}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        nodesFocusable={false}
        edgesFocusable={false}
        deleteKey={null}
        selectionKey={null}
        multiSelectionKey={null}
        selectionOnDrag={false}
        zoomOnDoubleClick={false}
        zoomOnScroll={false}
        panOnScroll={false}
        preventScrolling={false}
        onpaneclick={() => (selected = '')}
        proOptions={{ hideAttribution: true }}
      />
    {/if}
  </div>
</Panel>

<style>
  .zoom {
    display: inline-flex;
    align-items: center;
  }
  .value {
    min-width: 40px;
    padding-right: 8px;
    color: var(--muted);
    font: 11px var(--font-mono);
    text-align: right;
  }
  .zoom button {
    min-width: 30px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 8px;
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  .zoom button + button {
    border-left: 0;
  }
  .zoom button:hover:enabled {
    background: var(--surface-hover);
  }
  .zoom button:disabled {
    color: var(--muted);
    cursor: default;
  }
  .zoom button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .graph {
    min-width: 0;
    min-height: 132px;
  }
  /* Svelte Flow, restyled with Wyrd tokens. Nothing animates and nothing casts a shadow. */
  .graph :global(.svelte-flow) {
    --xy-background-color: var(--surface);
    --xy-edge-stroke: var(--muted);
    overflow: clip;
    background: var(--surface);
  }
  .graph :global(.svelte-flow__node) {
    border: 0;
    border-radius: var(--r);
    padding: 0;
    background: transparent;
    box-shadow: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: default;
  }
  .graph :global(.svelte-flow__node.stage) {
    pointer-events: none;
  }
  .graph :global(.svelte-flow__handle) {
    width: 1px;
    min-width: 0;
    height: 1px;
    min-height: 0;
    border: 0;
    opacity: 0;
    pointer-events: none;
  }
</style>
