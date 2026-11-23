<script lang="ts">
  // The one owner of page-level head metadata: browser title, description,
  // canonical URL under the production site, and Open Graph/Twitter tags.
  // `path` is base-free ('/' or '/section/page/'); `updated` is a commit date.
  let {
    title,
    description,
    path,
    type,
    updated
  }: { title: string; description?: string; path: string; type: 'website' | 'article'; updated?: string } = $props();

  const url = $derived(`${__WYRD_SITE__}${path}`);
</script>

<svelte:head>
  <title>{title}</title>
  {#if description}
    <meta name="description" content={description} />
    <meta property="og:description" content={description} />
  {/if}
  <link rel="canonical" href={url} />
  <meta property="og:type" content={type} />
  <meta property="og:site_name" content="Wyrd" />
  <meta property="og:title" content={title} />
  <meta property="og:url" content={url} />
  {#if updated}<meta property="article:modified_time" content={updated} />{/if}
  <meta name="twitter:card" content="summary" />
</svelte:head>
