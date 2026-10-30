<script lang="ts">
  import { getEntry } from '$lib/content';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // Resolve the component from the eager content map so it renders
  // synchronously into the prerendered HTML (no async-pending shell).
  const Content = $derived(getEntry(data.slug)?.default);


  // `updated` is the page source's last commit date (see $lib/updated.js), shown
  // in UTC so the calendar day matches the commit regardless of reader locale.
  const updated = $derived(
    data.metadata.updated &&
      new Date(`${data.metadata.updated}T00:00:00Z`).toLocaleDateString('en-US', {
        timeZone: 'UTC',
        dateStyle: 'long'
      })
  );
</script>

<svelte:head>
  <title>{data.metadata.title} — Wyrd docs</title>
  {#if data.metadata.description}
    <meta name="description" content={data.metadata.description} />
  {/if}
</svelte:head>

<!-- Archetype (article | hub | reference) is a styling/indexing hook only. -->
<article class="prose" data-archetype={data.archetype} data-pagefind-body>
  <!-- The frontmatter title is the page's only <h1>; content starts at ##. -->
  <h1>{data.metadata.title}</h1>
  {#if Content}<Content />{/if}
  {#if updated}
    <p class="page-updated" data-pagefind-ignore>
      Last updated <time datetime={data.metadata.updated}>{updated}</time>
    </p>
  {/if}
</article>
