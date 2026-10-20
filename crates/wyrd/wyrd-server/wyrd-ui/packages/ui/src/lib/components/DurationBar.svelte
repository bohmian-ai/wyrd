<!-- One duration against the longest in its set: the bar is positional, the text carries the value. -->
<script lang="ts">
  import { fmtDuration } from '../format.js';

  let { ms, max, label }: { ms: number; max: number; label?: string } = $props();
</script>

<span class="duration">
  {#if label}<span class="label">{label}</span>{/if}
  <span class="track" aria-hidden="true"><span class="bar" style:width="{max > 0 ? Math.min(100, (ms / max) * 100) : 0}%"></span></span>
  <span class="value">{fmtDuration(ms)}</span>
</span>

<style>
  .duration {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font: 11px var(--font-mono);
    color: var(--text);
  }
  .label {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track {
    height: 6px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  .bar {
    display: block;
    height: 100%;
    background: var(--declare);
  }
</style>
