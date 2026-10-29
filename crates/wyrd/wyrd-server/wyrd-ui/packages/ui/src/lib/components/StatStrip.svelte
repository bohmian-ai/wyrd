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
</script>

{#if result.state === 'populated' || result.state === 'empty'}
  <div class="stats">
  <dl class="strip" aria-label="Totals over the range">
    {#each result.data.stats as { stat, total, series } (stat)}
      {@const values = total.rows.map((row) => value(stat, row[0], row[1])).filter((text) => text !== undefined)}
      {@const points = series.rows.filter((row) => row[1] !== null).map((row) => numeric(row[1]))}
      <div class="stat">
        <dt>{LABELS[stat]}</dt>
        <dd>
          {#if values.length === 0}
            <span class="value" title="Nothing recorded in this range">—</span>
          {:else}
            {#each values as text, index (index)}<span class="value">{text}</span>{/each}
          {/if}
          {#if stat !== 'cost'}<Sparkline {points} label="{LABELS[stat]} per bucket" />{/if}
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
