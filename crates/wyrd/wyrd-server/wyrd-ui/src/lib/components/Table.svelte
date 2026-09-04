<script lang="ts">
  import type { Snippet } from 'svelte';

  // A styled wrapper. Pass a normal <table> (thead/tbody) as children; the wrapper styles
  // its descendants. Add class="sel" to a <tr> for the selection wash.
  //
  // Irreducibly wide tables scroll rather than drop columns, so the scroller is a labeled
  // region with tabindex="0": a keyboard-only reader can focus it and pan with the arrow
  // keys, which a plain overflow container does not allow.
  let { label, children }: { label: string; children?: Snippet } = $props();
</script>

<!-- A scrollable region is the one WCAG-sanctioned nonnegative tabindex on a non-widget:
     without it the horizontal scroller is unreachable by keyboard. role=region +
     aria-label make it a named landmark, which is exactly what the rule presumes absent. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="wy-table" role="region" aria-label={label} tabindex="0">{@render children?.()}</div>

<style>
  .wy-table {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow-x: auto;
  }
  .wy-table:focus-visible {
    outline: 2px solid var(--brand-strong);
    outline-offset: 2px;
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
  /* a linked cell reads as the row's destination, not as decorated prose */
  .wy-table :global(td a) {
    color: var(--text);
    text-decoration: none;
  }
  .wy-table :global(tbody tr:hover td a) {
    text-decoration: underline;
  }
  .wy-table :global(tbody tr:last-child td) {
    border-bottom: 0;
  }
  /* A row is only interactive when it actually carries a link — a version readout is a
     static table, and a blanket row hover would promise a click that is not there. */
  .wy-table :global(tbody tr:has(a):hover td) {
    background: var(--surface-2);
  }
  .wy-table :global(tbody tr:has(a:focus-visible) td) {
    background: var(--surface-2);
    box-shadow: inset 4px 0 0 0 var(--brand-strong);
  }
  .wy-table :global(tr.sel td) {
    background: var(--brand-soft);
    box-shadow: inset 4px 0 0 0 var(--brand-strong);
  }
</style>
