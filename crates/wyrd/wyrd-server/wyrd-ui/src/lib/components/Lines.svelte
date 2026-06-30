<script lang="ts">
  type Series = { points: number[]; color?: string };
  let { series, labels = [] }: { series: Series[]; labels?: string[] } = $props();

  const W = 300;
  const H = 148;
  const x0 = 10;
  const x1 = 294;
  const yTop = 12;
  const yBot = 120;

  const grid = [0, 1, 2, 3].map((g) => yTop + ((yBot - yTop) * g) / 3);

  const plotted = $derived.by(() => {
    const max = Math.max(...series.flatMap((s) => s.points)) || 1;
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    const Y = (v: number) => yBot - (v / max) * (yBot - yTop);
    return series.map((s) => {
      const pts = s.points.map((v, i) => ({ x: x0 + step * i, y: Y(v) }));
      return {
        color: s.color ?? 'var(--rune-strong)',
        line: pts.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '),
        pts
      };
    });
  });

  const xlabels = $derived.by(() => {
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    return labels.map((label, i) => ({ label, x: x0 + step * i })).filter((l) => l.label);
  });
</script>

<svg class="wy-lines" viewBox={`0 0 ${W} ${H}`}>
  {#each grid as gy}
    <line class="grid" x1={x0} y1={gy.toFixed(1)} x2={x1} y2={gy.toFixed(1)} stroke-width="1" stroke-dasharray="3 3" />
  {/each}
  <line class="axis" x1={x0} y1={yBot} x2={x1} y2={yBot} stroke-width="2" />
  {#each plotted as s}
    <polyline points={s.line} fill="none" style={`stroke:${s.color}`} stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" />
    {#each s.pts as p}
      <rect x={(p.x - 2.5).toFixed(1)} y={(p.y - 2.5).toFixed(1)} width="5" height="5" class="node" style={`stroke:${s.color}`} stroke-width="2" />
    {/each}
  {/each}
  {#each xlabels as l}
    <text class="xl" x={l.x.toFixed(1)} y={yBot + 13} text-anchor="middle">{l.label}</text>
  {/each}
</svg>

<style>
  .wy-lines {
    display: block;
    width: 100%;
    height: auto;
  }
  .grid {
    stroke: var(--border);
    stroke-opacity: 0.22;
  }
  .axis {
    stroke: var(--border);
  }
  .node {
    fill: var(--surface);
  }
  .xl {
    font-family: var(--fm);
    font-size: 8.5px;
    fill: var(--muted);
  }
</style>
