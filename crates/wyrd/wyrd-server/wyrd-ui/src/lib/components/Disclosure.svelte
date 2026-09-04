<script lang="ts">
  import type { Snippet } from 'svelte';

  // Progressive disclosure on native <details>: open/close state, keyboard operation and
  // the expanded/collapsed announcement come from the platform. `open` is the initial
  // state only — the element owns it afterwards.
  let {
    summary,
    meta,
    open = false,
    children
  }: { summary: string; meta?: string; open?: boolean; children?: Snippet } = $props();
</script>

<details class="wy-disclosure" {open}>
  <summary>
    <span class="s">{summary}</span>
    {#if meta}<span class="m">{meta}</span>{/if}
  </summary>
  <div class="body">{@render children?.()}</div>
</details>

<style>
  .wy-disclosure {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  summary {
    list-style: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 11px;
    font-family: var(--fm);
    font-size: 11px;
    font-weight: 700;
    color: var(--text);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary::before {
    content: '▸';
    color: var(--muted);
    font-size: 9px;
  }
  .wy-disclosure[open] > summary::before {
    content: '▾';
  }
  summary:hover,
  summary:focus-visible {
    background: var(--surface-2);
  }
  .m {
    margin-left: auto;
    font-weight: 400;
    font-size: 9.5px;
    color: var(--muted);
  }
  .body {
    padding: 11px;
    border-top: 2px dashed var(--border);
  }
</style>
