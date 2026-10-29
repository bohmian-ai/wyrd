<!--
  A row of totals over the range, each with its per-bucket trend. A stat with
  nothing recorded reads "—", never 0; cost shows one amount per recorded
  currency and never adds currencies together.
-->
<script lang="ts">
  import type { Stat, StatStripProps } from '../loaders.js';
  import { fmtDuration, fmtNumber } from '../format.js';
  import { numeric } from '../plot.js';
  import EmptyState from './EmptyState.svelte';
  import Sparkline from './Sparkline.svelte';

  let { input, result }: StatStripProps = $props();

  const LABELS: Record<Stat, string> = {
    traces: 'Traces',
    spans: 'Spans',
    errors: 'Error spans',
    duration_p95: 'p95 duration',
    invocations: 'Agent invocations',
    tokens: 'Tokens',
    calls: 'Gateway calls',
    cost: 'Cost'
  };

  /** One total as text; `undefined` when nothing was recorded. */
  function value(stat: Stat, cell: unknown, currency?: unknown): string | undefined {
    if (cell === null || cell === undefined) return undefined;
    if (stat === 'duration_p95') return fmtDuration(Math.round(numeric(cell) / 1e6));
    // Decimal text, trimmed of trailing zeros but never rounded.
    if (stat === 'cost') return `${String(cell).replace(/(\.\d*?)0+$/, '$1').replace(/\.$/, '')} ${String(currency)}`;
    return fmtNumber(numeric(cell));
  }

  /** Per-bucket values in order, of one currency when given; null buckets are skipped, never zero. */
  function trend(rows: readonly (readonly unknown[])[], currency?: unknown): number[] {
    return rows.filter((row) => row[1] !== null && (currency === undefined || row[2] === currency)).map((row) => numeric(row[1]));
  }
</script>

{#if result.state === 'populated' || result.state === 'empty'}
  <div class="stats">
  <dl class="strip" aria-label="Totals over the range">
    {#each result.data.stats as { stat, total, series } (stat)}
      <!-- Cost has one total and one trend per recorded currency. -->
      {@const values = total.rows
        .map((row) => ({ text: value(stat, row[0], row[1]), currency: stat === 'cost' ? String(row[1]) : '', points: trend(series.rows, stat === 'cost' ? row[1] : undefined) }))
        .filter((each) => each.text !== undefined)}
      <div class="stat">
        <dt>{LABELS[stat]}</dt>
        <dd>
          {#if values.length === 0}
            <span class="value" title="Nothing recorded in this range">—</span>
            <Sparkline points={[]} label="{LABELS[stat]} per bucket" />
          {:else}
            {#each values as { text, currency, points }, index (index)}
              <span class="value">{text}</span>
              <Sparkline {points} label="{LABELS[stat]}{currency ? ` in ${currency}` : ''} per bucket" />
            {/each}
          {/if}
        </dd>
      </div>
    {/each}
  </dl>
  {#if result.state === 'empty'}<p class="reason">{result.reason}</p>{/if}
  </div>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.stats.map((stat) => LABELS[stat]).join(', ')}" />
{:else}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .strip {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    margin: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-sans);
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 12px 14px;
    border-right: 1px solid var(--border-soft);
  }
  dt {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px 10px;
    margin: 0;
  }
  .value {
    font: 600 22px/1.1 var(--font-sans);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .reason {
    margin: 6px 0 0;
    color: var(--muted);
    font: 12px var(--font-sans);
  }
</style>
