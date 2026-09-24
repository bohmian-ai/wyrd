<script lang="ts">
  import type { Snippet } from 'svelte';

  // Numbered procedure. Each direct <li> child is one step, drawn as a numbered
  // node on a 1px spine (Evidence Thread procedure grammar).
  let { children }: { children?: Snippet } = $props();
</script>

<ol class="wyrd-steps">
  {@render children?.()}
</ol>

<style>
  .wyrd-steps {
    position: relative;
    list-style: none;
    margin: 24px 0;
    padding: 0;
    counter-reset: step;
  }
  .wyrd-steps::before {
    content: '';
    position: absolute;
    top: 14px;
    bottom: 14px;
    left: 12px;
    width: 1px;
    background: var(--border);
  }
  .wyrd-steps :global(li) {
    position: relative;
    counter-increment: step;
    max-width: none;
    margin: 0;
    padding: 0 0 20px 42px;
  }
  .wyrd-steps :global(li::before) {
    content: counter(step);
    position: absolute;
    top: 0;
    left: 0;
    width: 25px;
    height: 25px;
    display: grid;
    place-items: center;
    border: 1px solid var(--text);
    border-radius: var(--r);
    background: var(--surface);
    font: 11px/1 var(--font-mono);
  }
  .wyrd-steps :global(li > :first-child) {
    margin-top: 0;
  }
  .wyrd-steps :global(li > :last-child) {
    margin-bottom: 0;
  }
</style>
