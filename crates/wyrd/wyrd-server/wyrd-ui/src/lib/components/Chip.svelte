<script lang="ts">
  // A removable filter chip. Removal is a link, never a callback: the chip's whole
  // contract is "this filter is applied, and here is the URL without it", which keeps
  // filter state in the URL and the chip safe to place from a serialized view.
  let {
    label,
    value,
    removeHref
  }: { label: string; value: string; removeHref?: string } = $props();
</script>

<span class="wy-chip">
  <span class="k">{label}</span><span class="v">{value}</span>
  {#if removeHref}
    <a class="x" href={removeHref} aria-label={`Remove filter ${label}: ${value}`}>✕</a>
  {/if}
</span>

<style>
  .wy-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    font-family: var(--fm);
    font-size: 10px;
    padding: 3px 4px 3px 8px;
    border: 2px solid var(--border);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text);
  }
  .k {
    color: var(--muted);
    text-transform: uppercase;
    font-size: 8.5px;
    letter-spacing: 0.5px;
  }
  .v {
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 15px;
    height: 15px;
    border-radius: 3px;
    color: var(--muted);
    text-decoration: none;
    font-size: 9px;
  }
  .x:hover,
  .x:focus-visible {
    background: var(--surface-2);
    color: var(--text);
  }
</style>
