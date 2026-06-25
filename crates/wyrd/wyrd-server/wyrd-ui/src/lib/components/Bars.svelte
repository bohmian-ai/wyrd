<script lang="ts">
  type Bar = { label: string; value: number; color?: string };
  let { data }: { data: Bar[] } = $props();

  const W = 300;
  const H = 148;
  const x0 = 8;
  const x1 = 294;
  const yTop = 14;
  const yBase = 120;

  const bars = $derived.by(() => {
    const max = Math.max(...data.map((d) => d.value)) || 1;
    const n = data.length || 1;
    const slot = (x1 - x0) / n;
    const bw = Math.min(34, slot * 0.62);
    return data.map((d, i) => {
      const cx = x0 + slot * i + slot / 2;
      const h = (d.value / max) * (yBase - yTop);
      return {
        x: cx - bw / 2,
        y: yBase - h,
        w: bw,
        h,
        cx,
        value: d.value,
        label: d.label,
        color: d.color ?? 'var(--rune-strong)'
      };
    });
  });
</script>

<svg class="wy-bars" viewBox={`0 0 ${W} ${H}`}>
  <line class="axis" x1={x0} y1={yBase} x2={x1} y2={yBase} stroke-width="2" />
  {#each bars as b (b.label)}
    <rect
      x={b.x.toFixed(1)}
      y={b.y.toFixed(1)}
      width={b.w.toFixed(1)}
      height={b.h.toFixed(1)}
      rx="2"
      class="bar"
      style={`fill:${b.color}`}
      stroke-width="2"
    />
    <text class="val" x={b.cx.toFixed(1)} y={(b.y - 4).toFixed(1)} text-anchor="middle">{b.value}</text>
    <text class="cat" x={b.cx.toFixed(1)} y={yBase + 13} text-anchor="middle">{b.label}</text>
  {/each}
</svg>

<style>
  .wy-bars {
    display: block;
    width: 100%;
    height: auto;
  }
  .axis,
  .bar {
    stroke: var(--border);
  }
  .val {
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 700;
    fill: var(--text);
  }
  .cat {
    font-family: var(--fm);
    font-size: 8.5px;
    fill: var(--muted);
  }
</style>
