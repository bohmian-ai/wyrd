<script lang="ts">
  import { SvelteFlow, Background, Controls, type Node, type Edge } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { theme } from '$lib/theme.svelte';
  import type { Span } from './core/types';

  let {
    spans,
    colors,
    selectedId,
    onselect
  }: {
    spans: Span[];
    colors: Map<string, string>;
    selectedId: string;
    onselect: (spanId: string) => void;
  } = $props();

  // The span DAG: parent/child edges recovered from the waterfall's
  // depth-plus-order contract (a span's parent is the nearest preceding span
  // one level shallower). Nodes column by depth, lane by arrival, so the graph
  // reads left-to-right like the request actually flowed.
  function build(): { nodes: Node[]; edges: Edge[] } {
    const lanes = new Map<number, number>();
    const stack: Span[] = [];
    const nodes: Node[] = [];
    const edges: Edge[] = [];
    for (const span of spans) {
      while (stack.length && stack[stack.length - 1].depth >= span.depth) stack.pop();
      const parent = stack[stack.length - 1];
      stack.push(span);
      const lane = lanes.get(span.depth) ?? 0;
      lanes.set(span.depth, lane + 1);
      const color = span.error ? 'var(--danger)' : (colors.get(span.service) ?? 'var(--border)');
      nodes.push({
        id: span.id,
        position: { x: span.depth * 250, y: lane * 52 },
        data: { label: `${span.error ? '✕ ' : ''}${span.name} · ${span.durationLabel}` },
        sourcePosition: 'right',
        targetPosition: 'left',
        style: [
          `border: 2px solid ${color}`,
          'border-radius: 5px',
          'background: var(--surface)',
          'color: var(--text)',
          'font: 700 10px var(--font-mono)',
          'padding: 6px 10px',
          'width: 210px',
          span.id === selectedId ? 'box-shadow: 3px 3px 0 var(--shadow)' : ''
        ].join(';')
      } as Node);
      if (parent)
        edges.push({
          id: `${parent.id}->${span.id}`,
          source: parent.id,
          target: span.id,
          animated: span.error,
          style: span.error ? 'stroke: var(--danger); stroke-width: 1.5px' : 'stroke: var(--muted)',
          label: span.error ? 'error' : undefined
        } as Edge);
    }
    return { nodes, edges };
  }

  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  // Rebuilds when the trace or selection changes; SvelteFlow keeps pan/zoom.
  $effect(() => {
    const graph = build();
    nodes = graph.nodes;
    edges = graph.edges;
  });
</script>

<div class="span-flow">
  <SvelteFlow
    bind:nodes
    bind:edges
    fitView
    minZoom={0.2}
    colorMode={theme.mode === 'dark' ? 'dark' : 'light'}
    onnodeclick={({ node }) => onselect(node.id)}
    proOptions={{ hideAttribution: true }}
  >
    <Background gap={18} />
    <Controls showLock={false} />
  </SvelteFlow>
</div>

<style>
  .span-flow {
    height: 480px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    overflow: hidden;
  }
  .span-flow :global(.svelte-flow) {
    background: var(--surface-2);
  }
  .span-flow :global(.svelte-flow__node) {
    cursor: pointer;
  }
</style>
