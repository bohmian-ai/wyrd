<script lang="ts">
  // Dedicated full-width home (the old `template: splash`). It renders the
  // hero/levelmap/cardtilegrid composition + prose from HomeContent.svx rather
  // than going through the `[...slug]` content catch-all, so the layout can drop
  // the sidebar/TOC rails on `/`.
  import HomeContent from '$lib/HomeContent.svx';
</script>

<svelte:head>
  <title>Wyrd docs</title>
  <meta name="description" content="Developer and agent documentation for Wyrd." />
</svelte:head>

<article class="sl-markdown-content">
  <HomeContent />
</article>
