<script lang="ts">
  import { getEntry } from '$lib/content';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // Resolve the component from the eager content map so it renders
  // synchronously into the prerendered HTML (no async-pending shell).
  const Content = $derived(getEntry(data.slug)?.default);
</script>

<svelte:head>
  <title>{data.metadata.title} — Wyrd docs</title>
  {#if data.metadata.description}
    <meta name="description" content={data.metadata.description} />
  {/if}
</svelte:head>

<!-- Archetype (article | hub | reference) is a styling/indexing hook only. -->
<article class="prose" data-archetype={data.archetype} data-pagefind-body>
  {#if Content}<Content />{/if}
</article>
