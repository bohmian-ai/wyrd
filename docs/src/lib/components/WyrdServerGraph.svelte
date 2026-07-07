<script lang="ts">
  import ZoomableFigure from '$lib/components/ZoomableFigure.svelte';

  // Server component map. Bespoke SVG rather than a flow-graph library: the
  // docs site builds on Vite 8 (rolldown), which cannot parse @xyflow/svelte's
  // shipped .svelte source (xyflow/xyflow#5734 — xyflow is not rolldown-ready).
  // This reproduces the flow-graph look (dotted grid, rounded bordered nodes,
  // orthogonal edges) with zero deps and pulls every color from the design
  // tokens so light/dark both work. It matches WyrdConceptsDiagram's approach.
  type Node = { t: string; s?: string; accent?: boolean };
  type Col = { label: string; seq?: boolean; nodes: Node[] };

  const COLS: Col[] = [
    {
      label: 'BOOT',
      seq: true,
      nodes: [
        { t: 'load config' },
        { t: 'PostgresBoot', s: '+ migrations' },
        { t: 'assemble', s: 'pools · storage · OLAP' },
        { t: 'install auth', s: 'signing key · verifier' },
        { t: 'seed federation', s: 'issuers · bindings' },
        { t: 'production_validate', accent: true }
      ]
    },
    {
      label: 'APPSTATE',
      nodes: [
        { t: 'postgres' },
        { t: 'storage' },
        { t: 'bifrost OLAP' },
        { t: 'auth + authz' },
        { t: 'telemetry + limits' }
      ]
    },
    {
      label: 'RUNTIME',
      nodes: [
        { t: 'HTTP transport' },
        { t: 'gRPC transport' },
        { t: 'metrics endpoint' },
        { t: 'readiness · health', s: 'sweeper' },
        { t: 'signal watcher' }
      ]
    }
  ];

  // ── Geometry ──────────────────────────────────────────────────────────
  const VW = 770;
  const COL_W = 210;
  const COL_X = [20, 280, 540];
  const NODE_H = 48;
  const NODE_GAP = 18;
  const TOP = 16;
  const LABEL_SPACE = 34;
  const INSET = 14;
  const CLUSTER_PAD = 14;

  const firstY = TOP + LABEL_SPACE;
  const nodeTop = (i: number) => firstY + i * (NODE_H + NODE_GAP);
  const clusterHeight = (count: number) =>
    nodeTop(count - 1) + NODE_H - TOP + CLUSTER_PAD;

  const model = $derived.by(() => {
    type Box = { x: number; y: number; w: number; cx: number } & Node;
    type Rect = { x: number; y: number; w: number; h: number; label: string };
    type Edge = { d: string; label?: string; lx?: number; ly?: number };

    const clusters: Rect[] = [];
    const boxes: Box[] = [];
    const seqEdges: Edge[] = [];

    COLS.forEach((col, c) => {
      const x = COL_X[c];
      const w = col.nodes.length;
      clusters.push({ x, y: TOP, w: COL_W, h: clusterHeight(w), label: col.label });
      const cx = x + COL_W / 2;
      col.nodes.forEach((n, i) => {
        boxes.push({ ...n, x: x + INSET, y: nodeTop(i), w: COL_W - INSET * 2, cx });
        if (col.seq && i < col.nodes.length - 1) {
          seqEdges.push({ d: `M ${cx} ${nodeTop(i) + NODE_H} L ${cx} ${nodeTop(i + 1)}` });
        }
      });
    });

    const bootMid = TOP + clusterHeight(COLS[0].nodes.length) / 2;
    const bootRight = COL_X[0] + COL_W;
    const stateRight = COL_X[1] + COL_W;
    const runCx = COL_X[2] + COL_W / 2;
    const runBottom = nodeTop(COLS[2].nodes.length - 1) + NODE_H;

    const supY = 470;
    const supH = 52;
    const supX = COL_X[1];
    const supW = stateRight + (COL_X[2] + COL_W - stateRight) - COL_X[1];

    const crossEdges: Edge[] = [
      { d: `M ${bootRight} ${bootMid} L ${COL_X[1]} ${bootMid}`, label: 'handles', lx: bootRight + 8, ly: bootMid - 8 },
      { d: `M ${stateRight} ${bootMid} L ${COL_X[2]} ${bootMid}`, label: 'shared', lx: stateRight + 10, ly: bootMid - 8 },
      { d: `M ${runCx} ${runBottom} L ${runCx} ${supY}`, label: 'supervised', lx: runCx + 10, ly: (runBottom + supY) / 2 }
    ];

    return {
      clusters,
      boxes,
      seqEdges,
      crossEdges,
      supervisor: { x: supX, y: supY, w: supW, h: supH, cx: supX + supW / 2 },
      height: supY + supH + TOP
    };
  });
</script>

<ZoomableFigure>
<figure class="srv">
  <svg
    viewBox={`0 0 ${VW} ${model.height}`}
    role="img"
    aria-label="wyrd-server components: an ordered fail-closed boot sequence loads the AppState handle registry, which is shared into the supervised runtime tasks, all drained by one supervisor"
    xmlns="http://www.w3.org/2000/svg"
  >
    <defs>
      <marker
        id="srv-arrow"
        viewBox="0 0 10 10"
        refX="9"
        refY="5"
        markerWidth="7"
        markerHeight="7"
        orient="auto"
      >
        <path d="M0,0 L10,5 L0,10 Z" fill="var(--border)" />
      </marker>
      <pattern id="srv-dots" width="18" height="18" patternUnits="userSpaceOnUse">
        <circle cx="1.5" cy="1.5" r="1.1" class="srv-dot" />
      </pattern>
    </defs>

    <rect x="0" y="0" width={VW} height={model.height} fill="url(#srv-dots)" />

    {#each model.clusters as cl (cl.label)}
      <rect class="srv-cluster" x={cl.x} y={cl.y} width={cl.w} height={cl.h} rx="6" />
      <text class="srv-cluster-label" x={cl.x + 14} y={cl.y + 21}>{cl.label}</text>
    {/each}

    {#each model.seqEdges as e (e.d)}
      <path class="srv-edge" d={e.d} marker-end="url(#srv-arrow)" />
    {/each}

    {#each model.crossEdges as e (e.d)}
      <path class="srv-edge" d={e.d} marker-end="url(#srv-arrow)" />
      {#if e.label}
        <text class="srv-edge-label" x={e.lx} y={e.ly}>{e.label}</text>
      {/if}
    {/each}

    {#each model.boxes as b (b.x + '-' + b.y)}
      <rect class="srv-node-shadow" x={b.x + 4} y={b.y + 4} width={b.w} height={NODE_H} rx="5" />
      <rect class={b.accent ? 'srv-node-accent' : 'srv-node'} x={b.x} y={b.y} width={b.w} height={NODE_H} rx="5" />
      <text
        class={b.accent ? 'srv-node-text srv-node-text-accent' : 'srv-node-text'}
        x={b.cx}
        y={b.s ? b.y + 21 : b.y + 29}>{b.t}</text>
      {#if b.s}
        <text class={b.accent ? 'srv-node-sub srv-node-sub-accent' : 'srv-node-sub'} x={b.cx} y={b.y + 37}
          >{b.s}</text>
      {/if}
    {/each}

    <rect
      class="srv-node-shadow"
      x={model.supervisor.x + 4}
      y={model.supervisor.y + 4}
      width={model.supervisor.w}
      height={model.supervisor.h}
      rx="5"
    />
    <rect
      class="srv-node"
      x={model.supervisor.x}
      y={model.supervisor.y}
      width={model.supervisor.w}
      height={model.supervisor.h}
      rx="5"
    />
    <text class="srv-node-text" x={model.supervisor.cx} y={model.supervisor.y + 23}>supervisor</text>
    <text class="srv-node-sub" x={model.supervisor.cx} y={model.supervisor.y + 39}>drain → abort</text>
  </svg>
</figure>
</ZoomableFigure>

<style>
  .srv {
    margin: 1.5rem 0;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
  }
  .srv svg {
    display: block;
    width: 100%;
    height: auto;
  }
  .srv-dot {
    fill: var(--border);
    opacity: 0.28;
  }
  .srv-cluster {
    fill: var(--surface-2);
    fill-opacity: 0.4;
    stroke: var(--border);
    stroke-width: 2;
    stroke-dasharray: 5 4;
  }
  .srv-cluster-label {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }
  .srv-edge {
    fill: none;
    stroke: var(--border);
    stroke-width: 2.5;
  }
  .srv-edge-label {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.02em;
  }
  .srv-node-shadow {
    fill: var(--shadow);
  }
  .srv-node {
    fill: var(--surface);
    stroke: var(--border);
    stroke-width: 2;
  }
  .srv-node-accent {
    fill: var(--rune);
    stroke: var(--border);
    stroke-width: 2;
  }
  .srv-node-text {
    fill: var(--text);
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    text-anchor: middle;
  }
  .srv-node-text-accent {
    fill: var(--hero-ink);
  }
  .srv-node-sub {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    text-anchor: middle;
  }
  .srv-node-sub-accent {
    fill: color-mix(in srgb, var(--hero-ink) 78%, transparent);
  }
</style>
