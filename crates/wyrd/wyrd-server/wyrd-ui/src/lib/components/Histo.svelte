<script lang="ts">
  let { reference, current }: { reference: number[]; current: number[] } = $props();

  const W = 326;
  const H = 116;
  const x0 = 8;
  const x1 = 320;
  const yTop = 8;
  const yBase = 92;

  const groups = $derived.by(() => {
    const max = Math.max(...reference, ...current) || 1;
    const n = reference.length || 1;
    const slot = (x1 - x0) / n;
    const bw = slot * 0.32;
    return reference.map((rv, i) => {
      const cx = x0 + slot * i + slot / 2;
      const rh = (rv / max) * (yBase - yTop);
      const ch = ((current[i] ?? 0) / max) * (yBase - yTop);
      return {
        bw,
        refX: cx - bw - 1,
        refY: yBase - rh,
        refH: rh,
        curX: cx + 1,
        curY: yBase - ch,
        curH: ch
      };
    });
  });
</script>

<svg class="wy-histo" viewBox={`0 0 ${W} ${H}`}>
  <line class="axis" x1={x0} y1={yBase} x2={x1} y2={yBase} stroke-width="2" />
  {#each groups as g, i (i)}
    <rect class="ref" x={g.refX.toFixed(1)} y={g.refY.toFixed(1)} width={g.bw.toFixed(1)} height={g.refH.toFixed(1)} stroke-width="2" />
    <rect class="cur" x={g.curX.toFixed(1)} y={g.curY.toFixed(1)} width={g.bw.toFixed(1)} height={g.curH.toFixed(1)} stroke-width="2" />
  {/each}
</svg>

<style>
  .wy-histo {
    display: block;
    width: 100%;
    height: auto;
  }
  .axis {
    stroke: var(--border);
  }
  .ref {
    fill: color-mix(in srgb, var(--control-bar) 38%, var(--surface));
    stroke: var(--control-bar);
  }
  .cur {
    fill: color-mix(in srgb, var(--rune-strong) 40%, var(--surface));
    stroke: var(--rune-strong);
  }
</style>
