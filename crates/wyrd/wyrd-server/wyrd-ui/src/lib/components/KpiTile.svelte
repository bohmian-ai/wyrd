<script lang="ts">
  import type { Snippet } from 'svelte';

  type Trend = 'up' | 'down' | 'flat';
  let {
    label,
    value,
    delta,
    trend = 'flat',
    spark
  }: {
    label: string;
    value: string | number;
    delta?: string;
    trend?: Trend;
    spark?: Snippet;
  } = $props();
</script>

<div class="wy-kpi">
  <div class="k">{label}</div>
  <div class="n">{value}</div>
  {#if delta}<div class="d" data-trend={trend}>{delta}</div>{/if}
  {#if spark}<div class="sp">{@render spark()}</div>{/if}
</div>

<style>
  .wy-kpi {
    /* min-width:0 + overflow prevents wide values ($42.10) from blowing out a grid track */
    min-width: 0;
    overflow: hidden;
    padding: 11px 13px;
  }
  .k {
    font-family: var(--fm);
    font-size: 8.5px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .n {
    font-family: var(--fm);
    font-size: 22px;
    font-weight: 700;
    color: var(--text);
    margin-top: 2px;
  }
  .d {
    font-family: var(--fm);
    font-size: 10px;
    margin-top: 4px;
    color: var(--muted);
  }
  .d[data-trend='up'] {
    color: var(--ok);
  }
  .d[data-trend='down'] {
    color: var(--danger);
  }
  .sp {
    margin-top: 8px;
  }
</style>
