<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { siblings } from '$lib/nav';

  // Prev/next derived from the flattened nav.ts order ("PREV / NEXT LEVEL").
  const pair = $derived(siblings(page.url.pathname));
</script>

{#if pair.prev || pair.next}
  <nav class="wy-pager" aria-label="Pagination">
    {#if pair.prev}
      <a class="pg pg-prev" href={`${base}${pair.prev.path}`} rel="prev">
        <span class="pg-dir">&larr; Prev level</span>
        <span class="pg-label">{pair.prev.label}</span>
      </a>
    {:else}
      <span></span>
    {/if}
    {#if pair.next}
      <a class="pg pg-next" href={`${base}${pair.next.path}`} rel="next">
        <span class="pg-dir">Next level &rarr;</span>
        <span class="pg-label">{pair.next.label}</span>
      </a>
    {/if}
  </nav>
{/if}

<style>
  .wy-pager {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin: 3rem 0 1rem;
  }
  .pg {
    display: flex;
    flex-direction: column;
    gap: 5px;
    text-decoration: none;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    padding: 12px 14px;
    transition:
      transform 0.08s,
      box-shadow 0.08s;
  }
  .pg:hover {
    transform: translate(-2px, -2px);
    box-shadow: 6px 6px 0 0 var(--shadow);
  }
  .pg-next {
    text-align: right;
  }
  .pg-dir {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .pg-label {
    font-family: var(--font-display);
    font-size: 0.95rem;
    line-height: 1.15;
  }
  /* Phone: stack prev/next so neither card crushes, and keep the next card
     left-aligned once it's full-width. */
  @media (max-width: 640px) {
    .wy-pager {
      grid-template-columns: 1fr;
      gap: 10px;
    }
    .pg-next {
      text-align: left;
    }
  }
</style>
