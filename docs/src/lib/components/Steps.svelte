<script lang="ts">
  import type { Snippet } from 'svelte';

  // Numbered steps list. Children are arbitrary content; each direct child is
  // treated as one step. Renders as an <ol> with brutalist counter chrome
  // (token-bound border, radius, shadow) and no raw hex.
  let { children }: { children?: Snippet } = $props();
</script>

<ol class="wyrd-steps">
  {@render children?.()}
</ol>

<style>
  .wyrd-steps {
    list-style: none;
    padding: 0;
    margin: 1.5rem 0;
    counter-reset: step;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .wyrd-steps :global(li) {
    counter-increment: step;
    display: grid;
    grid-template-columns: 2rem 1fr;
    gap: 14px;
    align-items: start;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    padding: 14px 16px;
  }
  .wyrd-steps :global(li::before) {
    content: counter(step);
    font-family: var(--font-display);
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--rune-strong);
    line-height: 1;
    padding-top: 2px;
  }
  .wyrd-steps :global(li > :first-child) {
    margin-top: 0;
  }
  .wyrd-steps :global(li > :last-child) {
    margin-bottom: 0;
  }
</style>
