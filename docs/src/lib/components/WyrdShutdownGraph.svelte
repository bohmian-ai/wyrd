<script lang="ts">
  import ZoomableFigure from '$lib/components/ZoomableFigure.svelte';

  // Supervision and shutdown flow, same bespoke flow-graph style as
  // WyrdServerGraph (dotted grid, rounded lime-bordered nodes, orthogonal
  // edges). Left-to-right with a classify branch that fans to graceful vs
  // terminal, then merges back into the drain. Zero deps; all colors from
  // design tokens so light/dark both work.
  type Node = {
    id: string;
    t: string;
    s?: string;
    x: number;
    y: number;
    accent?: boolean;
  };

  const NODE_W = 132;
  const NODE_H = 52;
  const VW = 1052;
  const VH = 300;
  const MAIN_Y = 128;
  const UP_Y = 34;
  const LOW_Y = 214;
  const COL_X = [20, 196, 372, 548, 724, 900];

  const nodes: Node[] = [
    { id: 'wait', t: 'wait', s: 'first exit', x: COL_X[0], y: MAIN_Y },
    { id: 'classify', t: 'classify', x: COL_X[1], y: MAIN_Y, accent: true },
    { id: 'graceful', t: 'graceful', s: 'signal', x: COL_X[2], y: UP_Y },
    { id: 'terminal', t: 'terminal', s: 'task error', x: COL_X[2], y: LOW_Y },
    { id: 'cancel', t: 'cancel token', x: COL_X[3], y: MAIN_Y },
    { id: 'drain', t: 'drain', s: 'within drain_ms', x: COL_X[4], y: MAIN_Y },
    { id: 'abort', t: 'abort', s: 'what remains', x: COL_X[5], y: MAIN_Y, accent: true }
  ];

  const cy = (n: Node) => n.y + NODE_H / 2;
  const right = (n: Node) => n.x + NODE_W;
  const byId = (id: string) => nodes.find((n) => n.id === id) as Node;

  const w = byId('wait');
  const c = byId('classify');
  const g = byId('graceful');
  const t = byId('terminal');
  const x = byId('cancel');
  const d = byId('drain');
  const a = byId('abort');

  // Orthogonal edge paths. Branch splits at x=350, merge gathers at x=526.
  const edges = [
    { d: `M ${right(w)} ${cy(w)} L ${c.x} ${cy(c)}` },
    { d: `M ${right(c)} ${cy(c)} L 350 ${cy(c)} L 350 ${cy(g)} L ${g.x} ${cy(g)}` },
    { d: `M ${right(c)} ${cy(c)} L 350 ${cy(c)} L 350 ${cy(t)} L ${t.x} ${cy(t)}` },
    { d: `M ${right(g)} ${cy(g)} L 526 ${cy(g)} L 526 ${cy(x)} L ${x.x} ${cy(x)}` },
    { d: `M ${right(t)} ${cy(t)} L 526 ${cy(t)} L 526 ${cy(x)} L ${x.x} ${cy(x)}` },
    { d: `M ${right(x)} ${cy(x)} L ${d.x} ${cy(d)}` },
    { d: `M ${right(d)} ${cy(d)} L ${a.x} ${cy(a)}` }
  ];

  const edgeLabels = [
    { text: 'SIGINT / SIGTERM', x: g.x + NODE_W / 2, y: UP_Y - 10 },
    { text: 'transport / worker error', x: t.x + NODE_W / 2, y: LOW_Y + NODE_H + 18 }
  ];
</script>

<ZoomableFigure>
<figure class="sd">
  <svg
    viewBox={`0 0 ${VW} ${VH}`}
    role="img"
    aria-label="Supervision and shutdown: the supervisor waits for the first task to exit, classifies it as a graceful signal or a terminal error, cancels the shutdown token, drains within drain_ms, then aborts whatever remains"
    xmlns="http://www.w3.org/2000/svg"
  >
    <defs>
      <marker
        id="sd-arrow"
        viewBox="0 0 10 10"
        refX="9"
        refY="5"
        markerWidth="7"
        markerHeight="7"
        orient="auto"
      >
        <path d="M0,0 L10,5 L0,10 Z" fill="var(--border)" />
      </marker>
      <pattern id="sd-dots" width="18" height="18" patternUnits="userSpaceOnUse">
        <circle cx="1.5" cy="1.5" r="1.1" class="sd-dot" />
      </pattern>
    </defs>

    <rect x="0" y="0" width={VW} height={VH} fill="url(#sd-dots)" />

    {#each edges as e (e.d)}
      <path class="sd-edge" d={e.d} marker-end="url(#sd-arrow)" />
    {/each}

    {#each edgeLabels as l (l.text)}
      <text class="sd-edge-label" x={l.x} y={l.y}>{l.text}</text>
    {/each}

    {#each nodes as n (n.id)}
      <rect class="sd-node-shadow" x={n.x + 4} y={n.y + 4} width={NODE_W} height={NODE_H} rx="5" />
      <rect class={n.accent ? 'sd-node-accent' : 'sd-node'} x={n.x} y={n.y} width={NODE_W} height={NODE_H} rx="5" />
      <text
        class={n.accent ? 'sd-node-text sd-node-text-accent' : 'sd-node-text'}
        x={n.x + NODE_W / 2}
        y={n.s ? n.y + 23 : n.y + 31}>{n.t}</text>
      {#if n.s}
        <text class={n.accent ? 'sd-node-sub sd-node-sub-accent' : 'sd-node-sub'} x={n.x + NODE_W / 2} y={n.y + 39}
          >{n.s}</text>
      {/if}
    {/each}
  </svg>
</figure>
</ZoomableFigure>

<style>
  .sd {
    margin: 1.5rem 0;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
  }
  .sd svg {
    display: block;
    width: 100%;
    height: auto;
  }
  .sd-dot {
    fill: var(--border);
    opacity: 0.28;
  }
  .sd-edge {
    fill: none;
    stroke: var(--border);
    stroke-width: 2.5;
  }
  .sd-edge-label {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.02em;
    text-anchor: middle;
  }
  .sd-node-shadow {
    fill: var(--shadow);
  }
  .sd-node {
    fill: var(--surface);
    stroke: var(--border);
    stroke-width: 2;
  }
  .sd-node-accent {
    fill: var(--rune);
    stroke: var(--border);
    stroke-width: 2;
  }
  .sd-node-text {
    fill: var(--text);
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    text-anchor: middle;
  }
  .sd-node-text-accent {
    fill: var(--hero-ink);
  }
  .sd-node-sub {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    text-anchor: middle;
  }
  .sd-node-sub-accent {
    fill: color-mix(in srgb, var(--hero-ink) 78%, transparent);
  }
</style>
