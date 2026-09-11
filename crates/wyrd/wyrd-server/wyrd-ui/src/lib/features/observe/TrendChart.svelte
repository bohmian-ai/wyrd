<script lang="ts">
  import { RANGE_MS } from './core/filter-state';

  let {
    buckets,
    range,
    end,
    unit,
    tone = 'ok',
    label,
    fmt = (n: number) => n.toLocaleString()
  }: {
    /** One value per time bucket, oldest first. */
    buckets: number[];
    /** Range key ('15m'…'30d') the buckets span, ending at `end`. */
    range: string;
    /** Epoch ms of the window end — bucket clock labels derive from it. */
    end: number;
    /** Unit for tooltips, e.g. 'error traces'. */
    unit: string;
    tone?: 'ok' | 'danger';
    /** Accessible chart description prefix. */
    label: string;
    /** Value formatter for the y-axis and tooltips. */
    fmt?: (n: number) => string;
  } = $props();

  const rangeMs = $derived(RANGE_MS[range] ?? RANGE_MS['1h']);
  const max = $derived(Math.max(...buckets, 1));
  /** UTC clock label — matches every other timestamp the Observe pages show. */
  function clock(ms: number): string {
    return new Date(ms).toISOString().slice(11, 16);
  }
  /** X-axis tick label at fraction `f` of the window (0 = oldest). */
  const xTicks = $derived([0, 0.25, 0.5, 0.75, 1].map((f) => clock(end - rangeMs * (1 - f))));
  /** Tooltip for bucket `i`: its clock window and formatted value. */
  function tip(i: number, value: number): string {
    const start = end - rangeMs + (rangeMs / buckets.length) * i;
    return `${clock(start)}–${clock(start + rangeMs / buckets.length)} · ${fmt(value)} ${unit}`;
  }
</script>

<div class="trend mono">
  <div class="y muted" aria-hidden="true">
    <span>{fmt(max)}</span>
    <span>{fmt(max / 2 >= 10 ? Math.round(max / 2) : max / 2)}</span>
    <span>0</span>
  </div>
  <div class="plot" role="img" aria-label={`${label}: ${buckets.join(', ')} ${unit} per bucket`}>
    {#each buckets as value, i (i)}
      <span
        class={`bar ${tone}`}
        style={`height:${Math.max(2, (value / max) * 100)}%`}
        data-tip={tip(i, value)}
      ></span>
    {/each}
  </div>
  <div class="x muted" aria-hidden="true">
    {#each xTicks as tick, i (i)}<span>{tick}</span>{/each}
  </div>
</div>

<style>
  .trend {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-rows: 72px auto;
    column-gap: 8px;
    font-size: 10px;
  }
  .y {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: flex-end;
    text-align: right;
    /* Center each tick on its gridline. */
    margin: -0.5em 0;
    padding: 0.5em 0;
  }
  .plot {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    border-left: 2px solid var(--border);
    border-bottom: 2px solid var(--border);
    padding: 0 2px;
    /* Gridlines at the max and midpoint y ticks. */
    background:
      linear-gradient(to bottom, var(--border) 1px, transparent 1px) 0 0 / 100% 100% no-repeat,
      linear-gradient(to bottom, var(--border) 1px, transparent 1px) 0 50% / 100% 1px no-repeat;
    background-color: transparent;
  }
  .bar {
    flex: 1;
    min-width: 2px;
    position: relative;
    border-radius: 1px 1px 0 0;
  }
  .bar.danger {
    background: color-mix(in srgb, var(--danger) 72%, var(--surface));
  }
  .bar.ok {
    background: color-mix(in srgb, var(--brand-strong) 72%, var(--surface));
  }
  .bar:hover {
    outline: 1px solid var(--text);
  }
  /* CSS-only hover tooltip: bucket window plus value, anchored above the bar. */
  .bar:hover::after {
    content: attr(data-tip);
    position: absolute;
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 5;
    white-space: nowrap;
    padding: 4px 8px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    box-shadow: 3px 3px 0 0 var(--shadow);
    font: 700 10px var(--font-mono);
    pointer-events: none;
  }
  /* Keep the leftmost/rightmost tooltips inside the panel. */
  .bar:first-child:hover::after {
    left: 0;
    transform: none;
  }
  .bar:last-child:hover::after {
    left: auto;
    right: 0;
    transform: none;
  }
  .x {
    grid-column: 2;
    display: flex;
    justify-content: space-between;
    margin-top: 4px;
  }
</style>
