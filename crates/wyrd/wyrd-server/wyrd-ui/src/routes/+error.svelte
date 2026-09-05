<script lang="ts">
  import { page } from '$app/state';
  import StateBlock from '$lib/components/StateBlock.svelte';
</script>

<svelte:head><title>Wyrd · {page.status}</title></svelte:head>
<main>
  <h1>Unable to open this workspace</h1>
  <StateBlock state={page.status === 401 || page.status === 403 ? 'unauthorized' : 'error'}
    title={page.error?.title ?? page.error?.message ?? 'Page unavailable'}
    detail={page.error?.remediation} code={page.error?.code} actionLabel="Return to tenant entry" actionHref="/" />
</main>

<style>
  main {
    max-width: 640px;
    min-height: 100dvh;
    margin: auto;
    padding: 10vh 20px;
  }
  h1 {
    font: 700 26px var(--font-display);
    margin-bottom: 24px;
  }
</style>
