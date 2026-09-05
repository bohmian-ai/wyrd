<script lang="ts">
  let {
    points,
    width = 90,
    height = 24
  }: { points: number[]; width?: number; height?: number } = $props();

  const d = $derived.by(() => {
    const max = Math.max(...points);
    const min = Math.min(...points);
    const rng = max - min || 1;
    const step = width / (points.length - 1 || 1);
    return points
      .map((v, i) => `${(i * step).toFixed(1)},${(height - 2 - ((v - min) / rng) * (height - 4)).toFixed(1)}`)
      .join(' ');
  });
</script>

<svg
  class="wy-spark"
  viewBox={`0 0 ${width} ${height}`}
  preserveAspectRatio="none"
  fill="none"
>
  <polyline
    points={d}
    stroke="var(--muted)"
    stroke-width="2"
    stroke-linejoin="round"
    stroke-linecap="round"
  />
</svg>

<style>
  .wy-spark {
    display: block;
    width: 100%;
    height: 24px;
  }
</style>
