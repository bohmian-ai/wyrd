<!--
  A section a newer read is replacing: while `pending`, what it shows belongs
  to the previous population, so it is dimmed, busy and labelled Updating.
  The label overlays the content, so the section keeps its height.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let { pending, children }: { pending: boolean; children: Snippet } = $props();
</script>

<div class="updating" aria-busy={pending ? 'true' : undefined}>
  {#if pending}<p class="word" role="status" aria-label="Updating"><span aria-hidden="true">●</span> Updating</p>{/if}
  <div class="content" class:dim={pending}>{@render children()}</div>
</div>

<style>
  .updating {
    position: relative;
    min-width: 0;
  }
  .word {
    position: absolute;
    top: 6px;
    right: 8px;
    z-index: 1;
    margin: 0;
    padding: 2px 8px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--muted);
    font: 11px/1.5 var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .dim {
    opacity: 0.5;
  }
</style>
