<!--
  One chart over read-only SQL: timeseries, bar, stat or table. A non-populated
  state renders EmptyState in place of the plot and never a value; a degraded
  terminal draws the plot with its partial notice. The footer names the exact
  SQL and offers Open in Query with the executed binds.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import type { ChartProps } from '../loaders.js';
  import { fmtInstant, fmtNumber } from '../format.js';
  import { barRows, columnIndex, numeric, timeSeries } from '../plot.js';
  import ActionButton from './ActionButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill from './StatusPill.svelte';

  let { input, executed, result, actions = {}, onaction }: ChartProps & ActionProps = $props();

  /** Rendered body width; the plot tracks its content box so SVG text draws at its CSS size. */
  let bodyWidth = $state(0);
  const W = $derived(bodyWidth > 64 ? bodyWidth - 32 : 560);
  const H = 160;
  /** Tick labels sit above their gridline, so the plot needs no left gutter. */
  const PAD = { left: 32, right: 6, top: 16, bottom: 22 };
  /** Position-indexed series styles: colour, dash and marker always differ together. */
  const STYLES = [
    { stroke: 'var(--declare)', dash: '', marker: 'square' },
    { stroke: 'var(--observe)', dash: '6 3', marker: 'circle' },
    { stroke: 'var(--text)', dash: '2 3', marker: 'triangle' },
    { stroke: 'var(--muted)', dash: '8 2 2 2', marker: 'diamond' }
  ] as const;

  const data = $derived(result.state === 'populated' || result.state === 'empty' ? result.data : undefined);
  const degraded = $derived(data?.terminal.outcome === 'degraded');

  const lines = $derived(
    data && input.type === 'timeseries' && result.state === 'populated' ? timeSeries(data, input.x, input.y, input.series) : []
  );
  const xs = $derived([...new Set(lines.flatMap((line) => line.points.map((point) => point.x)))].sort((a, b) => a - b));
  const bounds = $derived.by(() => {
    const ys = lines.flatMap((line) => line.points.map((point) => point.y));
    const low = Math.min(0, ...ys);
    const high = Math.max(...ys);
    return { x0: xs[0], x1: xs[xs.length - 1], y0: low, y1: niceCeil(high > low ? high : low + 1) };
  });
  const sx = (x: number) =>
    PAD.left + (bounds.x1 > bounds.x0 ? ((x - bounds.x0) / (bounds.x1 - bounds.x0)) * (W - PAD.left - PAD.right) : 0);
  const sy = (y: number) => H - PAD.bottom - ((y - bounds.y0) / (bounds.y1 - bounds.y0)) * (H - PAD.top - PAD.bottom);
  const ticks = $derived([0, 0.5, 1].map((share) => bounds.y0 + share * (bounds.y1 - bounds.y0)));
  /** Up to four x labels on observed instants: clock time within a day and a half, else the date. */
  const xLabels = $derived.by(() => {
    if (xs.length === 0) return [];
    const short = bounds.x1 - bounds.x0 <= 36 * 3_600_000;
    const label = (ms: number) => new Date(ms).toISOString().slice(short ? 11 : 5, short ? 16 : 10);
    const picks = [...new Set([0, 1, 2, 3].map((share) => Math.round((share / 3) * (xs.length - 1))))];
    return picks.map((index, position) => ({
      at: xs[index],
      text: label(xs[index]),
      anchor: position === 0 ? 'start' : position === picks.length - 1 ? 'end' : 'middle'
    }));
  });

  /** The smallest round step × 10ⁿ at or above `value`, so the halfway gridline lands on a round number too. */
  function niceCeil(value: number): number {
    const power = 10 ** Math.floor(Math.log10(value));
    return ([1, 2, 3, 4, 5, 6, 8, 10].find((step) => step * power >= value) ?? 10) * power;
  }

  /** Keyboard- and pointer-driven tooltip position over `xs`. */
  let focused = $state<number | undefined>();
  /** Brush selection in SVG x units while a pointer is down. */
  let brush = $state<{ from: number; to: number } | undefined>();

  function svgX(event: PointerEvent): number {
    const box = (event.currentTarget as SVGElement).getBoundingClientRect();
    return box.width > 0 ? ((event.clientX - box.left) / box.width) * W : 0;
  }
  function nearest(x: number): number {
    let best = 0;
    xs.forEach((value, index) => {
      if (Math.abs(sx(value) - x) < Math.abs(sx(xs[best]) - x)) best = index;
    });
    return best;
  }
  function onkeydown(event: KeyboardEvent) {
    const last = xs.length - 1;
    const step: Record<string, number> = { ArrowLeft: (focused ?? last) - 1, ArrowRight: (focused ?? last) + 1, Home: 0, End: last };
    if (!(event.key in step)) return;
    event.preventDefault();
    focused = Math.max(0, Math.min(last, step[event.key]));
  }
  function release() {
    const selected = brush;
    brush = undefined;
    if (!selected || !onaction || Math.abs(selected.to - selected.from) < 6) return;
    const at = (x: number) => bounds.x0 + ((x - PAD.left) / (W - PAD.left - PAD.right)) * (bounds.x1 - bounds.x0);
    const [from, to] = [at(Math.min(selected.from, selected.to)), at(Math.max(selected.from, selected.to))];
    onaction({ name: 'chart.range', input: { from: new Date(from).toISOString(), to: new Date(to).toISOString() } });
  }

  function marker(kind: (typeof STYLES)[number]['marker'], x: number, y: number): string {
    switch (kind) {
      case 'square':
        return `M${x - 3},${y - 3}h6v6h-6z`;
      case 'circle':
        return `M${x - 3},${y}a3,3 0 1,0 6,0a3,3 0 1,0 -6,0`;
      case 'triangle':
        return `M${x},${y - 4}l4,7h-8z`;
      case 'diamond':
        return `M${x},${y - 4}l4,4l-4,4l-4,-4z`;
    }
  }

  const bars = $derived(data && input.type === 'bar' && result.state === 'populated' ? barRows(data, input.x, input.y) : []);
  const barMax = $derived(Math.max(0, ...bars.flatMap((row) => row.values.map((value) => value ?? 0))) || 1);
  /** One drawn bar per category × `y`, positioned across the plot width. */
  const barCells = $derived.by(() => {
    if (input.type !== 'bar') return [];
    const ys = input.y;
    const cells = bars.flatMap((row) =>
      row.values.map((value, position) => ({ label: ys.length > 1 ? `${row.label} · ${ys[position]}` : row.label, value }))
    );
    const slot = (W - PAD.left - PAD.right) / Math.max(1, cells.length);
    const width = Math.min(48, slot * 0.7);
    return cells.map((cell, index) => {
      const height = ((cell.value ?? 0) / barMax) * (H - PAD.top - PAD.bottom);
      return { ...cell, x: PAD.left + index * slot + (slot - width) / 2, width, y: H - PAD.bottom - height, height };
    });
  });

  const stat = $derived.by(() => {
    if (!data || input.type !== 'stat' || result.state !== 'populated') return undefined;
    const index = input.y ? columnIndex(data, input.y[0]) : 0;
    const value = data.rows[0]?.[index];
    return value === null || value === undefined ? undefined : numeric(value);
  });

  const unitLabel = $derived(input.unit ? ` ${input.unit}` : '');
</script>

<section class="chart" aria-label={input.title ?? 'Chart'}>
  <header>
    <h3>{input.title ?? 'Chart'}{#if input.unit}{' '}<span class="unit">· {input.unit}</span>{/if}</h3>
    {#if degraded && data}
      <span title={data.terminal.warnings.join(', ')}><StatusPill tone="warn" label="Partial result" /></span>
    {/if}
    <span class="open">
      <ActionButton
        variant="link"
        label="Open in Query"
        icon="↗"
        state={onaction ? actions['chart.open_in_query'] : { state: 'unavailable', reason: 'This host has no Query' }}
        disabled={!executed}
        onclick={() => executed && onaction?.({ name: 'chart.open_in_query', input: executed })}
      />
    </span>
  </header>

  <div class="body" class:centered={input.type === 'stat' && result.state === 'populated'} bind:clientWidth={bodyWidth}>
    {#if result.state === 'populated'}
      {#if degraded && data}
        <p class="partial">Some sources did not complete: {data.terminal.warnings.join(', ') || 'see source completion'}.</p>
      {/if}
      {#if input.type === 'timeseries'}
        <!-- The plot is keyboard-operable: focus opens the tooltip and arrows walk it. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <svg
          class="plot"
          viewBox="0 0 {W} {H}"
          role="application"
          aria-roledescription="chart"
          tabindex="0"
          aria-label="{input.title ?? 'Time series'}: {lines.map((line) => line.label).join(', ')}"
          onfocus={() => (focused ??= xs.length - 1)}
          onblur={() => (focused = undefined)}
          {onkeydown}
          onpointerdown={(event) => {
            const x = svgX(event);
            brush = onaction ? { from: x, to: x } : undefined;
            (event.currentTarget as Element).setPointerCapture?.(event.pointerId);
          }}
          onpointermove={(event) => {
            const x = svgX(event);
            if (brush) brush = { ...brush, to: x };
            else focused = nearest(x);
          }}
          onpointerup={release}
          onpointerleave={() => (brush ? undefined : (focused = undefined))}
        >
          {#each ticks as tick (tick)}
            <line class="grid" x1="0" x2={W} y1={sy(tick)} y2={sy(tick)} />
            <text class="axis" x="0" y={sy(tick) - 4}>{fmtNumber(tick)}</text>
          {/each}
          {#each xLabels as label (label.at)}
            <text class="axis" x={sx(label.at)} y={H - 4} text-anchor={label.anchor}>{label.text}</text>
          {/each}
          {#each lines as line, index (line.label)}
            {@const style = STYLES[index]}
            <polyline
              points={line.points.map((point) => `${sx(point.x)},${sy(point.y)}`).join(' ')}
              stroke={style.stroke}
              stroke-dasharray={style.dash}
            />
            {#each line.points.length <= 60 ? line.points : [] as point (point.x)}
              <path class="marker" d={marker(style.marker, sx(point.x), sy(point.y))} fill={style.stroke} />
            {/each}
          {/each}
          {#if focused !== undefined && xs[focused] !== undefined}
            <line class="cursor" x1={sx(xs[focused])} x2={sx(xs[focused])} y1={PAD.top} y2={H - PAD.bottom} />
          {/if}
          {#if brush}
            <rect class="brush" x={Math.min(brush.from, brush.to)} y={PAD.top} width={Math.abs(brush.to - brush.from)} height={H - PAD.top - PAD.bottom} />
          {/if}
        </svg>
        {#if focused !== undefined && xs[focused] !== undefined}
          {@const at = xs[focused]}
          <p class="tooltip" role="status">
            <time datetime={new Date(at).toISOString()}>{fmtInstant(at)}</time>
            {#each lines as line (line.label)}
              {@const point = line.points.find((each) => each.x === at)}
              <span>{line.label}: {point ? `${fmtNumber(point.y)}${unitLabel}` : '—'}</span>
            {/each}
          </p>
        {/if}
        <ul class="legend">
          {#each lines as line, index (line.label)}
            {@const style = STYLES[index]}
            <li>
              <svg viewBox="0 0 24 8" width="24" height="8" aria-hidden="true">
                <line x1="0" x2="24" y1="4" y2="4" stroke={style.stroke} stroke-dasharray={style.dash} />
                <path d={marker(style.marker, 12, 4)} fill={style.stroke} />
              </svg>
              {line.label}
            </li>
          {/each}
        </ul>
      {:else if input.type === 'bar'}
        <svg
          class="plot bars"
          viewBox="0 0 {W} {H}"
          role="img"
          aria-label="{input.title ?? 'Bar chart'}: {barCells.map((cell) => `${cell.label} ${cell.value === null ? 'no value' : fmtNumber(cell.value)}${unitLabel}`).join(', ')}"
        >
          <line class="grid" x1="0" x2={W} y1={H - PAD.bottom} y2={H - PAD.bottom} />
          {#each barCells as cell, index (index)}
            <rect class="bar" x={cell.x} y={cell.y} width={cell.width} height={cell.height} />
            <text class="value" x={cell.x + cell.width / 2} y={cell.y - 4} text-anchor="middle">{cell.value === null ? '—' : fmtNumber(cell.value)}</text>
            <text class="axis" x={cell.x + cell.width / 2} y={H - 4} text-anchor="middle">{cell.label}</text>
          {/each}
        </svg>
      {:else if input.type === 'stat'}
        <p class="stat">{stat === undefined ? '—' : fmtNumber(stat)}{#if input.unit}<span class="unit">{input.unit}</span>{/if}</p>
      {:else if data}
        <!-- A wide result scrolls; the focusable region lets a keyboard reader pan it. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="scroller" role="region" aria-label="{input.title ?? 'Result'} table" tabindex="0">
          <table>
            <thead>
              <tr>{#each data.columns as column, index (index)}<th scope="col" title={column.data_type}>{column.name}</th>{/each}</tr>
            </thead>
            <tbody>
              {#each data.rows as row, index (index)}
                <tr>{#each row as cell, position (position)}<td>{cell === null ? '∅' : typeof cell === 'object' ? JSON.stringify(cell) : String(cell)}</td>{/each}</tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    {:else if result.state === 'empty'}
      <EmptyState state="empty" title="No data" detail={result.reason} />
    {:else if result.state === 'loading'}
      <EmptyState state="loading" title="Running query" />
    {:else}
      <EmptyState state={result.state} problem={result.problem} />
    {/if}
  </div>

  <footer>
    <span>source</span>
    <code title={input.sql}>{input.sql}</code>
  </footer>
</section>

<style>
  .chart {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-sans);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    border-bottom: 1px solid var(--border);
    padding: 10px 16px;
  }
  h3 {
    margin: 0;
    min-width: 0;
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .open {
    margin-left: auto;
  }
  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 12px 16px;
  }
  .centered {
    justify-content: center;
  }
  .partial {
    margin: 0;
    color: var(--warn);
    font: 11px var(--font-mono);
  }
  .plot {
    display: block;
    width: 100%;
    height: auto;
    touch-action: none;
  }
  .plot:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  polyline {
    fill: none;
    stroke-width: 1.6;
  }
  .grid {
    stroke: var(--border);
    stroke-dasharray: 3 3;
  }
  .axis,
  .value {
    fill: var(--muted);
    font: 10px var(--font-mono);
  }
  .value {
    fill: var(--text);
  }
  .bar {
    fill: var(--declare);
  }
  .cursor {
    stroke: var(--muted);
  }
  .brush {
    fill: var(--declare-soft);
    stroke: var(--declare);
    opacity: 0.6;
  }
  .tooltip {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 0;
    font: 11px var(--font-mono);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .stat {
    margin: 0;
    padding: 8px 0;
    font: 600 40px/1.1 var(--font-sans);
    font-variant-numeric: tabular-nums;
  }
  .stat .unit {
    margin-left: 6px;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .unit {
    text-transform: none;
  }
  .scroller {
    max-height: 320px;
    margin: 0 -16px -12px;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  th {
    position: sticky;
    top: 0;
    border-bottom: 1px solid var(--border);
    padding: 8px 16px;
    background: var(--surface);
    color: var(--muted);
    font-size: 11px;
    font-weight: 400;
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
  }
  td {
    padding: 9px 16px;
    white-space: nowrap;
  }
  tbody tr:not(:last-child) td {
    border-bottom: 1px solid var(--border-soft);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    border-top: 1px solid var(--border-soft);
    padding: 8px 16px;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  footer code {
    min-width: 0;
    overflow: hidden;
    font: inherit;
    color: var(--text);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
