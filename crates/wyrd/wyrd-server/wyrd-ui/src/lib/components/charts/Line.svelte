<script lang="ts">
  // Multi-series time chart. Series identity never rests on hue: --control-bar and
  // --brand-strong collapse to 1.22:1 under deuteranopia, so each series also carries a
  // distinct stroke-dasharray and marker shape, and the legend mirrors both as a line
  // sample rather than a block swatch. Series take no colour prop — the style is chosen
  // by position from a fixed table, so a serialized view can never name a token.
  type Series = { label: string; points: number[] };
  type Stamp = { label: string; at: string };
  type Threshold = { value: number; label: string };
  type Props = { series: Series[]; labels?: Stamp[]; unit?: string; threshold?: Threshold; min?: number };
  let { series, labels = [], unit, threshold, min = 0 }: Props = $props();

  // The plot renders at the container's real pixel width: the viewBox tracks the measured
  // width so a 1.5px stroke is 1.5px on screen. A fixed small viewBox stretched to the
  // panel would scale strokes, markers and axis text into toy proportions.
  let width = $state(0);
  const W = $derived(Math.max(width, 280));
  const H = 168;
  // Y-axis labels live in a reserved left gutter so they never collide with
  // the series, the threshold dash, or the plot's left edge.
  const x0 = 36;
  const x1 = $derived(W - 8);
  const yTop = 12;
  const yBot = 138;

  /** Stroke, dash and marker per series position. Four distinct non-colour identities. */
  const styles = [
    { color: 'var(--brand-strong)', dash: '', marker: 'square' },
    { color: 'var(--control-bar)', dash: '6 3', marker: 'circle' },
    { color: 'var(--server-bar)', dash: '2 3', marker: 'triangle' },
    { color: 'var(--lime-text)', dash: '8 2 2 2', marker: 'diamond' }
  ] as const;

  const grid = [0, 1, 2, 3].map((g) => yTop + ((yBot - yTop) * g) / 3);

  const count = $derived(series[0]?.points.length ?? 0);

  // The point the tooltip is reporting. Hovering or arrowing moves it; letting go returns
  // it to the most recent point. The tooltip shows while the plot is hovered or focused,
  // so it is reachable by keyboard and by tap, not by pointer alone.
  let hovered = $state<number | null>(null);
  let focused = $state(false);
  const active = $derived(hovered ?? count - 1);
  const showing = $derived(hovered !== null || focused);

  /** Snap the pointer's x to the nearest plotted index. */
  function track(event: PointerEvent): void {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    if (!box.width || count < 2) return;
    const ratio = ((event.clientX - box.left) / box.width) * W;
    const step = (x1 - x0) / (count - 1);
    hovered = Math.min(count - 1, Math.max(0, Math.round((ratio - x0) / step)));
  }

  /** Left/right step the reported point; home/end jump to the ends of the range. */
  function step(event: KeyboardEvent): void {
    const moves: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 };
    if (event.key in moves) {
      hovered = Math.min(count - 1, Math.max(0, active + moves[event.key]));
    } else if (event.key === 'Home') {
      hovered = 0;
    } else if (event.key === 'End') {
      hovered = count - 1;
    } else {
      return;
    }
    event.preventDefault();
  }

  const maxValue = $derived(
    Math.max(...series.flatMap((s) => s.points), threshold?.value ?? 0) || 1
  );

  // The drawn y-domain: `min` lets a banded operational value (availability
  // around an SLO, a rate inside its band) use the resolution the reading
  // needs instead of a flat line from zero. Values below min clamp to the axis.
  const span = $derived(Math.max(maxValue - min, Number.MIN_VALUE));

  const scale = $derived((v: number) => yBot - (Math.max(v - min, 0) / span) * (yBot - yTop));

  /** Compact y tick value at the axis's own resolution ("2.1k", "99.96"). */
  function fmt(v: number): string {
    if (span < 0.5) return v.toFixed(2);
    if (span < 10) return v.toFixed(1);
    if (v >= 10_000) return `${Math.round(v / 1000)}k`;
    if (v >= 1000) return `${(v / 1000).toFixed(1)}k`;
    if (v >= 10) return `${Math.round(v)}`;
    return `${Math.round(v * 100) / 100}`;
  }

  /** Value labels at the gridlines — a chart must say what its scale is. */
  const ylabels = $derived(
    grid.map((gy, i) => ({ y: gy, label: fmt(min + span * (1 - i / 3)) }))
  );

  const plotted = $derived.by(() => {
    if (series.length > styles.length) throw new RangeError('Line supports at most four series');
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    return series.map((s, i) => {
      const style = styles[i];
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

  const activeX = $derived(x0 + ((x1 - x0) / (count - 1 || 1)) * active);

  /** What the tooltip is titled: the point's own label, or its position in the range. */
  const activeLabel = $derived(labels[active]?.label || `point ${active + 1} of ${count}`);

  // The tooltip is positioned in percentages of the viewBox, so it tracks the plot at any
  // rendered width. It sits above the highest series at the active point and anchors
  // inward at the ends of the range so it is never clipped by the panel.
  const tipLeft = $derived((activeX / W) * 100);
  const tipTop = $derived((Math.min(...plotted.map((s) => s.pts[active]?.y ?? yBot)) / H) * 100);
  const tipShift = $derived(
    tipLeft < 22 ? '-6px' : tipLeft > 78 ? 'calc(-100% + 6px)' : '-50%'
  );

  const xlabels = $derived.by(() => {
    const n = series[0]?.points.length ?? 0;
    const step = (x1 - x0) / (n - 1 || 1);
    // The first and last labels anchor inward so an edge label is never clipped.
    return labels
      .map((label, i) => ({
        ...label,
        x: x0 + step * i,
        anchor: i === 0 ? 'start' : i === labels.length - 1 ? 'end' : 'middle'
      }))
      .filter((l) => l.label);
  });

  /** Marker path for one plotted point, so shape reads where colour cannot. */
  function marker(shape: string, x: number, y: number): string {
    const r = 2.5;
    if (shape === 'circle') return `M ${x - r} ${y} a ${r} ${r} 0 1 0 ${r * 2} 0 a ${r} ${r} 0 1 0 ${-r * 2} 0`;
    if (shape === 'triangle') return `M ${x} ${y - r} L ${x + r} ${y + r} L ${x - r} ${y + r} Z`;
    if (shape === 'diamond') return `M ${x} ${y - r} L ${x + r} ${y} L ${x} ${y + r} L ${x - r} ${y} Z`;
    return `M ${x - r} ${y - r} h ${r * 2} v ${r * 2} h ${-r * 2} Z`;
  }
</script>

<div class="wy-line">
  <div class="plot" bind:clientWidth={width}>
  <!-- A focusable data region, the same case as Table's scroller: the plot is a picture,
       but the point the tooltip reports is a position a reader must be able to move. The
       lint models only widgets and static images, so it sees no legitimate third case.
       Focusing the plot opens the tooltip on the most recent point and the arrow keys walk
       it, so nothing here is reachable by pointer alone. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <svg
    viewBox={`0 0 ${W} ${H}`}
    role="group"
    aria-label={`${series.map((s) => s.label).join(', ')} over the selected range. Use the arrow keys to read each point.`}
    tabindex="0"
    onpointermove={track}
    onpointerleave={() => (hovered = null)}
    onkeydown={step}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      hovered = null;
    }}
  >
    {#each grid as gy (gy)}
      <line class="grid" x1={x0} y1={gy.toFixed(1)} x2={x1} y2={gy.toFixed(1)} stroke-width="1" stroke-dasharray="2 4" />
    {/each}
    {#each ylabels as l (l.y)}
      <text aria-hidden="true" class="yl" x="2" y={(l.y - 3).toFixed(1)}>{l.label}{unit && l.y === grid[0] ? ` ${unit}` : ''}</text>
    {/each}
    <line class="axis" x1={x0} y1={yBot} x2={x1} y2={yBot} stroke-width="1" />
    {#if threshold}
      <line class="thr" x1={x0} y1={thresholdY.toFixed(1)} x2={x1} y2={thresholdY.toFixed(1)} stroke-width="1.5" stroke-dasharray="5 4" />
      <line class="tick" x1={x0 - 4} y1={thresholdY.toFixed(1)} x2={x0 + 4} y2={thresholdY.toFixed(1)} stroke-width="2.5" />
      <text class="thrl" x={x1} y={(thresholdY - 5).toFixed(1)} text-anchor="end">{threshold.label}</text>
    {/if}
    {#if showing}
      <line class="cursor" x1={activeX.toFixed(1)} y1={yTop} x2={activeX.toFixed(1)} y2={yBot} stroke-width="1" stroke-dasharray="2 3" />
    {/if}
    {#each plotted as s (s.label)}
      <polyline
        points={s.line}
        fill="none"
        style={`stroke:${s.color}`}
        stroke-dasharray={s.dash || undefined}
        stroke-width="1.5"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
      {#each s.pts as p, i (i)}
        <path
          class="node"
          class:read={showing && i === active}
          d={marker(s.marker, p.x, p.y)}
          style={`stroke:${s.color}`}
          stroke-width="1.25"
        />
      {/each}
    {/each}
    {#each xlabels as l (l.at)}
      <text aria-hidden="true" class="xl" x={l.x.toFixed(1)} y={yBot + 16} text-anchor={l.anchor}>{l.label}</text>
    {/each}
  </svg>
  <!-- Keep the SVG typography; HTML time elements expose the same ticks semantically. -->
  <span class="sr-only">
    {#each xlabels as l (l.at)}
      <time datetime={l.at}>{l.label}</time>{' '}
    {/each}
  </span>
  {#if showing}
    <div
      class="tip"
      style={`left:${tipLeft.toFixed(2)}%;top:${tipTop.toFixed(2)}%;transform:translate(${tipShift},-100%)`}
      aria-live="polite"
    >
      {#if labels[active]}
        <time class="at" datetime={labels[active].at}>{activeLabel}</time>
      {:else}
        <span class="at">{activeLabel}</span>
      {/if}
      <ul>
        {#each plotted as s, i (s.label)}
          <li>
            <svg class="sample" viewBox="0 0 26 10" aria-hidden="true">
              <line x1="1" y1="5" x2="25" y2="5" style={`stroke:${s.color}`} stroke-dasharray={s.dash || undefined} stroke-width="1.5" />
              <path class="node" d={marker(s.marker, 13, 5)} style={`stroke:${s.color}`} stroke-width="1.25" />
            </svg>
            <span class="sl">{s.label}</span>
            <span class="sv">{series[i].points[active] ?? '–'}{#if unit}<span class="su">{unit}</span>{/if}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
  </div>
  <ul class="legend">
    {#each plotted as s (s.label)}
      <li>
        <svg class="sample" viewBox="0 0 26 10" aria-hidden="true">
          <line x1="1" y1="5" x2="25" y2="5" style={`stroke:${s.color}`} stroke-dasharray={s.dash || undefined} stroke-width="1.5" />
          <path class="node" d={marker(s.marker, 13, 5)} style={`stroke:${s.color}`} stroke-width="1.25" />
        </svg>
        {s.label}
      </li>
    {/each}
  </ul>
</div>

<style>
  .plot {
    position: relative;
  }
  /* the plot only — the legend and tooltip samples keep their own fixed 26x10 box */
  .plot > svg {
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
    font-size: 10px;
    font-weight: 700;
    fill: var(--danger-text);
  }
  .node {
    fill: var(--surface);
  }
  /* the point the tooltip is reporting */
  .node.read {
    fill: var(--text);
  }
  .cursor {
    stroke: var(--muted);
    stroke-opacity: 0.55;
  }
  .plot > svg:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .xl,
  .yl {
    font-family: var(--fm);
    font-size: 10px;
    fill: var(--muted);
  }
  /* Workbench geometry, quiet altitude: 2px border, 5px radius, 3px hard shadow. It is
     never a hit target — pointer-events stay off so it cannot interrupt its own tracking. */
  .tip {
    position: absolute;
    z-index: 1;
    pointer-events: none;
    margin-top: -10px;
    padding: 6px 8px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
    font-family: var(--fm);
    font-size: 9.5px;
    white-space: nowrap;
  }
  .tip ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 3px 0 0;
    padding: 0;
    list-style: none;
  }
  .tip li {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .at {
    display: block;
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sl {
    color: var(--muted);
    margin-right: auto;
  }
  .sv {
    font-weight: 700;
    color: var(--text);
  }
  .su {
    font-weight: 400;
    color: var(--muted);
    margin-left: 2px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 3px 0 0;
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
