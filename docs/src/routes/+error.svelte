<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import CoverScene from '$lib/components/CoverScene.svelte';
  import { firstPage } from '$lib/derived-nav';

  // Recovery links resolve through published content, so they exist even while
  // a section is still empty.
  const start = firstPage('Get started');
  const reference = firstPage('Reference');
</script>

<svelte:head>
  <title>{page.status} — Wyrd docs</title>
</svelte:head>

<!-- adapter-static emits this as 404.html via fallback: '404.html'. -->
<section class="nf dark-field">
  <CoverScene />
  <div class="nf-body">
    <p class="code-status">{page.status}</p>
    <h1>{page.status === 404 ? 'This page does not exist' : 'Something went wrong'}</h1>
    <p>{page.error?.message ?? 'The page could not be loaded.'}</p>
    <div class="start">
      <a class="button primary" href={`${base}/`}>Docs home</a>
      {#if start}<a class="button" href={`${base}${start.path}`}>Get started</a>{/if}
      {#if reference}<a class="button" href={`${base}${reference.path}`}>Reference</a>{/if}
    </div>
  </div>
</section>
