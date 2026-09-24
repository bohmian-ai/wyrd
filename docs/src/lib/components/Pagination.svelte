<script lang="ts">
  // Prev/next pager (`.pag` in docs.css), derived from nav order via `siblings()`.
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { siblings } from '$lib/derived-nav';

  const pair = $derived(siblings(page.url.pathname, base || '/wyrd'));
</script>

{#if pair.prev || pair.next}
  <nav class="pag" aria-label="Pagination">
    {#if pair.prev}
      <a class="prev" href={`${base}${pair.prev.path}`} rel="prev">
        <div class="dir">&larr; Previous</div>
        <div class="lbl">{pair.prev.label}</div>
      </a>
    {/if}
    {#if pair.next}
      <a class="next" href={`${base}${pair.next.path}`} rel="next">
        <div class="dir">Next &rarr;</div>
        <div class="lbl">{pair.next.label}</div>
      </a>
    {/if}
  </nav>
{/if}
