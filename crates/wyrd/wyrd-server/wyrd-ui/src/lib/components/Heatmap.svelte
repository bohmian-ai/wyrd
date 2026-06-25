<script lang="ts">
  let {
    values,
    columns = 12,
    max
  }: { values: number[]; columns?: number; max?: number } = $props();

  const peak = $derived(max ?? Math.max(...values, 1));

  const cells = $derived(
    values.map((v) => `color-mix(in srgb, var(--rune-strong) ${((v / peak) * 88).toFixed(0)}%, var(--surface-2))`)
  );
</script>

<div class="wy-heatwrap">
  <div class="wy-heat" style={`grid-template-columns:repeat(${columns},1fr)`}>
    {#each cells as bg, i (i)}
      <span style={`background:${bg}`}></span>
    {/each}
  </div>
</div>

<style>
  .wy-heatwrap {
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 6px;
    background: var(--surface-2);
  }
  .wy-heat {
    display: grid;
    gap: 3px;
  }
  .wy-heat span {
    aspect-ratio: 1;
    border-radius: 2px;
  }
</style>
