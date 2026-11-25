<script>
  import { SvelteFlow, SvelteFlowProvider } from '@xyflow/svelte';
  import ZoomBar from './ZoomBar.svelte';
  import StepNode from './StepNode.svelte';
  import InputNode from './InputNode.svelte';
  import ReqNode from './ReqNode.svelte';
  import MemberNode from './MemberNode.svelte';
  import StageNode from './StageNode.svelte';
  import DepEdge from './DepEdge.svelte';
  import { layoutDefinition } from './layout.js';
  import { select, current, execs } from './store.svelte.js';

  let { mode } = $props();

  // Laid out once from the definition (and a session's pinned members). Execution updates never replace these arrays;
  // the parent re-keys this component only when the definition or execution changes.
  const { def, exec } = current();
  const laid = layoutDefinition(def, (exec?.members ?? []).map((id) => execs[id]));
  let nodes = $state.raw(laid.nodes);
  let edges = $state.raw(laid.edges);
  let viewport = $state({ x: 16, y: 12, zoom: 1 });
  let host = $state();
  let cw = $state(0);
  let height = $state(0);

  const nodeTypes = { step: StepNode, input: InputNode, req: ReqNode, member: MemberNode, stage: StageNode };
  const edgeTypes = { dep: DepEdge };
  const PAD = 16, MIN = 0.6, MAX = 1.6;

  // Fit: the whole graph at up to 100%, never below 60%; narrower panels pan instead.
  const fitFor = (w) => ({ x: PAD, y: 12, zoom: Math.max(MIN, Math.min(1, (w - 2 * PAD) / laid.width)) });
  // The panel is sized to the laid-out graph once, so a simple graph gets a compact panel.
  $effect(() => {
    if (!cw || height) return;
    viewport = fitFor(cw);
    height = Math.round(Math.min(640, Math.max(132, laid.height * viewport.zoom + 30)));
  });

  // Arrow keys move between node buttons by layout position; focusing selects, like a listbox.
  function onkeydown(e) {
    const dir = { ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowDown: [0, 1], ArrowUp: [0, -1] }[e.key];
    const from = e.target.closest?.('[data-node]');
    if (!dir || !from) return;
    e.preventDefault();
    const c = (el) => { const r = el.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; };
    const [fx, fy] = c(from);
    let best, bestScore = Infinity;
    for (const el of host.querySelectorAll('[data-node]')) {
      if (el === from) continue;
      const [x, y] = c(el);
      const along = (x - fx) * dir[0] + (y - fy) * dir[1];
      if (along <= 4) continue;
      const across = Math.abs((x - fx) * dir[1] + (y - fy) * dir[0]);
      const score = along + across * 2;
      if (score < bestScore) { best = el; bestScore = score; }
    }
    if (best) { best.focus({ preventScroll: true }); best.click(); }
  }
</script>

<section class="graph-panel" aria-label="{exec ? 'Execution' : 'Definition'} graph" data-testid="graph" data-graph-width={Math.round(laid.width)}>
  <SvelteFlowProvider>
    <ZoomBar zoom={viewport.zoom} fit={() => fitFor(host.clientWidth)} min={MIN} max={MAX} {host} definition={!exec} />
    <div class="graph-measure" bind:clientWidth={cw}>
      {#if height}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="graph-host" style:height="{height}px" bind:this={host} {onkeydown}>
          <SvelteFlow
            bind:nodes bind:edges bind:viewport
            {nodeTypes} {edgeTypes}
            colorMode={mode}
            minZoom={MIN} maxZoom={MAX}
            nodesDraggable={false} nodesConnectable={false} elementsSelectable={false}
            nodesFocusable={false} edgesFocusable={false}
            deleteKey={null} selectionKey={null} multiSelectionKey={null} selectionOnDrag={false}
            zoomOnDoubleClick={false} zoomOnScroll={false} panOnScroll={false} preventScrolling={false}
            onpaneclick={() => select(exec ? 'execution:' : '')}
            proOptions={{ hideAttribution: true }}
          />
        </div>
      {/if}
    </div>
  </SvelteFlowProvider>
</section>
