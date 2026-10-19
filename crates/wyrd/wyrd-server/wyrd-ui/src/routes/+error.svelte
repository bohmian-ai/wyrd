<script lang="ts">
  import { page } from '$app/state';
  let denied = $derived(page.status === 401 || page.status === 403);
</script>

<svelte:head><title>Wyrd · {page.status}</title></svelte:head>
<main>
  <h1>Unable to open this workspace</h1>
  <div class="app-problem" data-state={denied ? 'unauthorized' : 'error'} role="note">
    <strong>{denied ? '⊘' : '✕'} {page.error?.title ?? page.error?.message ?? 'Page unavailable'}</strong>
    {#if page.error?.remediation}<p>{page.error.remediation}</p>{/if}
    {#if page.error?.code}<code>{page.error.code}</code>{/if}
    <a href="/">Return to tenant entry</a>
  </div>
</main>

<style>
  main {
    max-width: 640px;
    min-height: 100dvh;
    margin: auto;
    padding: 10vh 20px;
  }
  h1 {
    font: var(--weight-strong) 26px var(--font-sans);
    margin-bottom: 24px;
  }
  a {
    display: inline-block;
    margin-top: 9px;
    font: var(--weight-strong) 11px var(--font-mono);
    color: var(--declare);
  }
</style>
