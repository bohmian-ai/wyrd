<script lang="ts">
  let { points, threshold }: { points: number[]; threshold: number } = $props();

  const W = 326;
  const H = 104;
  const x0 = 8;
  const x1 = 320;
  const yTop = 10;
  const yBot = 88;

  const view = $derived.by(() => {
    const max = Math.max(...points, threshold) * 1.1 || 1;
    const n = points.length || 1;
    const step = (x1 - x0) / (n - 1 || 1);
    const Y = (v: number) => yBot - (v / max) * (yBot - yTop);
    const nodes = points.map((v, i) => ({ x: x0 + step * i, y: Y(v), over: v > threshold }));
    return {
      ty: Y(threshold),
      line: nodes.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '),
      nodes
    };
  });
</script>

<svg class="wy-trend" viewBox={`0 0 ${W} ${H}`}>
  <line class="thr" x1={x0} y1={view.ty.toFixed(1)} x2={x1} y2={view.ty.toFixed(1)} stroke-width="2" stroke-dasharray="4 3" />
  <text class="thrlab" x={x1 - 2} y={(view.ty - 4).toFixed(1)} text-anchor="end">thr {threshold}</text>
  <line class="axis" x1={x0} y1={yBot} x2={x1} y2={yBot} stroke-width="2" />
  <polyline points={view.line} fill="none" class="series" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
  {#each view.nodes as p, i (i)}
    <rect class="node" class:over={p.over} x={(p.x - 2.5).toFixed(1)} y={(p.y - 2.5).toFixed(1)} width="5" height="5" stroke-width="2" />
  {/each}
</svg>

<style>
  .wy-trend {
    display: block;
    width: 100%;
    height: auto;
  }
  .axis {
    stroke: var(--border);
  }
  .thr {
    stroke: var(--danger);
  }
  .thrlab {
    font-family: var(--fm);
    font-size: 8px;
    fill: var(--danger);
  }
  .series {
    stroke: var(--rune-strong);
  }
  .node {
    fill: var(--surface);
    stroke: var(--rune-strong);
  }
  .node.over {
    stroke: var(--danger);
  }
</style>
