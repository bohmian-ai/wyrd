<script lang="ts">
  import type { Snippet } from 'svelte';

  // A styled wrapper. Pass a normal <table> (thead/tbody) as children; the wrapper
  // styles its descendants. Add class="sel" to a <tr> for the selection wash.
  let { children }: { children?: Snippet } = $props();
</script>

<div class="wy-table">{@render children?.()}</div>

<style>
  .wy-table {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow-x: auto;
  }
  .wy-table :global(th),
  .wy-table :global(td) {
    white-space: nowrap;
  }
  .wy-table :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-family: var(--fm);
    font-size: 12px;
  }
  .wy-table :global(th) {
    text-align: left;
    padding: 8px 9px;
    border-bottom: 2px solid var(--border);
    background: var(--surface);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .wy-table :global(td) {
    padding: 8px 9px;
    border-bottom: 2px solid var(--border);
    color: var(--text);
  }
  .wy-table :global(tbody tr:last-child td) {
    border-bottom: 0;
  }
  .wy-table :global(tr.sel td) {
    background: var(--rune-soft);
  }
</style>
