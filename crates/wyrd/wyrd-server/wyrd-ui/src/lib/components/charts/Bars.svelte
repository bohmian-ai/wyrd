<script lang="ts">
  // Categorical bars. Bars carry no colour prop: the fill is the chart line token and the
  // category is named under every bar, so the reading is positional and textual.
  type Bar = { label: string; value: number };
  let { data, unit }: { data: Bar[]; unit?: string } = $props();

  // Rendered at the container's real pixel width so strokes and labels keep production
  // proportions instead of scaling up with the panel (see Line.svelte).
  let width = $state(0);
  const W = $derived(Math.max(width, 280));
  const H = 168;
  const x0 = 8;
  const x1 = $derived(W - 8);
  const yTop = 16;
  const yBase = 138;

  const bars = $derived.by(() => {
    const max = Math.max(...data.map((d) => d.value)) || 1;
    const n = data.length || 1;
    const slot = (x1 - x0) / n;
    const bw = Math.min(48, slot * 0.62);
    return data.map((d, i) => {
      const cx = x0 + slot * i + slot / 2;
      const h = (d.value / max) * (yBase - yTop);
      return { x: cx - bw / 2, y: yBase - h, w: bw, h, cx, value: d.value, label: d.label };
    });
  });
</script>

<div class="wy-bars-wrap" bind:clientWidth={width}>
  <svg
    class="wy-bars"
    viewBox={`0 0 ${W} ${H}`}
    role="img"
    aria-label={`${data.map((d) => `${d.label} ${d.value}${unit ? ` ${unit}` : ''}`).join(', ')}`}
  >
    <line class="axis" x1={x0} y1={yBase} x2={x1} y2={yBase} stroke-width="1" />
    {#each bars as b (b.label)}
      <rect x={b.x.toFixed(1)} y={b.y.toFixed(1)} width={b.w.toFixed(1)} height={b.h.toFixed(1)} class="bar" stroke-width="1" />
      <text class="val" x={b.cx.toFixed(1)} y={(b.y - 5).toFixed(1)} text-anchor="middle">{b.value}</text>
      <text class="cat" x={b.cx.toFixed(1)} y={yBase + 16} text-anchor="middle">{b.label}</text>
    {/each}
  </svg>
</div>

<style>
  .wy-bars {
    display: block;
    width: 100%;
    height: auto;
  }
  .axis {
    stroke: var(--border);
  }
  .bar {
    fill: var(--brand-strong);
    stroke: var(--border);
  }
  .val {
    font-family: var(--fm);
    font-size: 10px;
    font-weight: 700;
    fill: var(--text);
  }
  .cat {
    font-family: var(--fm);
    font-size: 10px;
    fill: var(--muted);
  }
</style>
