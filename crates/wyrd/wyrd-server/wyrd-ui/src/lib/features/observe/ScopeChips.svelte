<script lang="ts">
  // The removable-filter row shared by every Observe page. Chips restore from
  // the URL; removing one re-renders the same route with only that filter
  // dropped, so canonical signal pages stay unfiltered homes.
  import Chip from '$lib/components/Chip.svelte';
  import type { ScopeChip } from './core/filter-state';

  let {
    chips,
    clearHref,
    note
  }: { chips: ScopeChip[]; clearHref?: string; note?: string } = $props();
</script>

<div class="chips" role="group" aria-label="Active filters">
  {#each chips as chip (chip.label)}
    <Chip label={chip.label} value={chip.value} removeHref={chip.removeHref} />
  {/each}
  {#if chips.length && clearHref}
    <a class="clear" href={clearHref}>Clear all</a>
  {/if}
  {#if note}<span class="note">{note}</span>{/if}
</div>

<style>
  .clear {
    font: 700 10px var(--font-mono);
    color: var(--muted);
    text-decoration: none;
  }
  .clear:hover,
  .clear:focus-visible {
    color: var(--text);
    text-decoration: underline;
  }
  .note {
    font: 11px var(--font-mono);
    color: var(--muted);
  }
</style>
