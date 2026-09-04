<script lang="ts">
  // Multi-series time chart. Series identity never rests on hue: --control-bar and
  // --brand-strong collapse to 1.22:1 under deuteranopia, so each series also carries a
  // distinct stroke-dasharray and marker shape, and the legend mirrors both as a line
  // sample rather than a block swatch. Series take no colour prop — the style is chosen
  // by position from a fixed table, so a serialized view can never name a token.
  type Series = { label: string; points: number[] };
  type Threshold = { value: number; label: string };
  let {
    series,
    labels = [],
    threshold
  }: { series: Series[]; labels?: string[]; threshold?: Threshold } = $props();

  const W = 300;
  const H = 148;
  const x0 = 10;
  const x1 = 294;
  const yTop = 12;
  const yBot = 120;

  /** Stroke, dash and marker per series position. Four styles; a fifth series repeats. */
  const styles = [
    { color: 'var(--brand-strong)', dash: '', marker: 'square' },
    { color: 'var(--control-bar)', dash: '6 3', marker: 'circle' },
    { color: 'var(--server-bar)', dash: '2 3', marker: 'triangle' },
    { color: 'var(--lime-text)', dash: '8 2 2 2', marker: 'diamond' }
  ] as const;

  const grid = [0, 1, 2, 3].map((g) => yTop + ((yBot - yTop) * g) / 3);

  const scale = $derived.by(() => {
    const values = series.flatMap((s) => s.points);
    const max = Math.max(...values, threshold?.value ?? 0) || 1;
    return (v: number) => yBot - (v / max) * (yBot - yTop);
  });

  const plotted = $derived.by(() => {
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    return series.map((s, i) => {
      const style = styles[i % styles.length];
      const pts = s.points.map((v, j) => ({ x: x0 + step * j, y: scale(v) }));
      return {
        ...style,
        label: s.label,
        line: pts.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' '),
        pts
      };
    });
  });

  const thresholdY = $derived(threshold ? scale(threshold.value) : 0);

  const xlabels = $derived.by(() => {
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    return labels.map((label, i) => ({ label, x: x0 + step * i })).filter((l) => l.label);
  });

  /** Marker path for one plotted point, so shape reads where colour cannot. */
  function marker(shape: string, x: number, y: number): string {
    const r = 3;
    if (shape === 'circle') return `M ${x - r} ${y} a ${r} ${r} 0 1 0 ${r * 2} 0 a ${r} ${r} 0 1 0 ${-r * 2} 0`;
    if (shape === 'triangle') return `M ${x} ${y - r} L ${x + r} ${y + r} L ${x - r} ${y + r} Z`;
    if (shape === 'diamond') return `M ${x} ${y - r} L ${x + r} ${y} L ${x} ${y + r} L ${x - r} ${y} Z`;
    return `M ${x - r} ${y - r} h ${r * 2} v ${r * 2} h ${-r * 2} Z`;
  }
</script>

<div class="wy-line">
  <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${series.map((s) => s.label).join(', ')} over the selected range`}>
    {#each grid as gy (gy)}
      <line class="grid" x1={x0} y1={gy.toFixed(1)} x2={x1} y2={gy.toFixed(1)} stroke-width="1" stroke-dasharray="3 3" />
    {/each}
    <line class="axis" x1={x0} y1={yBot} x2={x1} y2={yBot} stroke-width="2" />
    {#if threshold}
      <line class="thr" x1={x0} y1={thresholdY.toFixed(1)} x2={x1} y2={thresholdY.toFixed(1)} stroke-width="2" stroke-dasharray="5 3" />
      <line class="tick" x1={x0 - 4} y1={thresholdY.toFixed(1)} x2={x0 + 4} y2={thresholdY.toFixed(1)} stroke-width="3" />
      <text class="thrl" x={x1} y={(thresholdY - 4).toFixed(1)} text-anchor="end">{threshold.label}</text>
    {/if}
    {#each plotted as s (s.label)}
      <polyline
        points={s.line}
        fill="none"
        style={`stroke:${s.color}`}
        stroke-dasharray={s.dash || undefined}
        stroke-width="2.5"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
      {#each s.pts as p, i (i)}
        <path class="node" d={marker(s.marker, p.x, p.y)} style={`stroke:${s.color}`} stroke-width="2" />
      {/each}
    {/each}
    {#each xlabels as l (l.label)}
      <text class="xl" x={l.x.toFixed(1)} y={yBot + 13} text-anchor="middle">{l.label}</text>
    {/each}
  </svg>
  <ul class="legend">
    {#each plotted as s (s.label)}
      <li>
        <svg class="sample" viewBox="0 0 26 10" aria-hidden="true">
          <line x1="1" y1="5" x2="25" y2="5" style={`stroke:${s.color}`} stroke-dasharray={s.dash || undefined} stroke-width="2.5" />
          <path class="node" d={marker(s.marker, 13, 5)} style={`stroke:${s.color}`} stroke-width="2" />
        </svg>
        <span>{s.label}</span>
      </li>
    {/each}
  </ul>
</div>

<style>
  .wy-line svg {
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
  .thr,
  .tick {
    stroke: var(--danger);
  }
  .thrl {
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    fill: var(--danger-text);
  }
  .node {
    fill: var(--surface);
  }
  .xl {
    font-family: var(--fm);
    font-size: 8.5px;
    fill: var(--muted);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    font-family: var(--fm);
    font-size: 9.5px;
    color: var(--muted);
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .sample {
    width: 26px;
    height: 10px;
    flex: 0 0 auto;
  }
</style>
