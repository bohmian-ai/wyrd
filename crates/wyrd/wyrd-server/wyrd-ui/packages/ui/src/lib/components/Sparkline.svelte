<!-- A neutral trend polyline beside a labeled value; shape carries the trend, never status. -->
<script lang="ts">
  let { points, label, width = 90, height = 24 }: { points: readonly number[]; label: string; width?: number; height?: number } =
    $props();

  const path = $derived.by(() => {
    if (points.length < 2) return '';
    const min = Math.min(...points);
    const span = Math.max(...points) - min || 1;
    return points
      .map((value, index) => `${(index / (points.length - 1)) * width},${height - 1 - ((value - min) / span) * (height - 2)}`)
      .join(' ');
  });
</script>

<svg class="spark" viewBox="0 0 {width} {height}" {width} {height} role="img" aria-label={label}>
  {#if path}<polyline points={path} />{/if}
</svg>

<style>
  polyline {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.5;
  }
</style>
