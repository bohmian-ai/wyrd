<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
</script>

<svelte:head>
  <title>{page.status} — Wyrd docs</title>
</svelte:head>

<!-- adapter-static emits this as 404.html via fallback: '404.html'. -->
<div class="nf">
  <p class="code-status">{page.status}</p>
  <h1>{page.status === 404 ? 'This page does not exist' : 'Something went wrong'}</h1>
  <p>{page.error?.message ?? 'The page could not be loaded.'} Try search, or start from a task.</p>
  <div class="start">
    <a class="button primary" href={`${base}/get-started/`}>Run Wyrd locally</a>
    <a class="button" href={`${base}/reference/`}>Reference</a>
  </div>
</div>
