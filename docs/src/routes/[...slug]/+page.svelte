<script lang="ts">
  import { getEntry } from '$lib/content';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // Resolve the component from the eager content map (not via serialized load
  // data) so it renders synchronously into the prerendered HTML.
  const Content = $derived(getEntry(data.slug)?.default);
</script>

<svelte:head>
  <title>{data.metadata.title ?? 'Wyrd docs'}</title>
  {#if data.metadata.description}
    <meta name="description" content={data.metadata.description} />
  {/if}
</svelte:head>

{#if Content}
  <article class="sl-markdown-content">
    <Content />
  </article>
{/if}
