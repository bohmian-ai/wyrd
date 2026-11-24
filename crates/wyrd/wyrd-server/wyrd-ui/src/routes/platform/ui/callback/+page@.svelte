<!--
  The platform SSO return. A console re-authentication opened it as a
  pop-up: it tells the console how Rust answered, and the console closes it.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import Entry from '$lib/Entry.svelte';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  let reported = $state(false);
  onMount(() => {
    if (!window.opener) return;
    window.opener.postMessage({ type: 'wyrd-platform-reauth', problem: data.problem }, location.origin);
    // The console closes this pop-up once it has the report.
    reported = true;
  });
</script>

<svelte:head><title>Platform admin · Wyrd</title></svelte:head>
<Entry heading="Platform admin" issue={data.problem}>
  {#if reported}
    <p class="center" role="status">Returning to the console…</p>
  {:else if data.reauthenticated}
    <p class="center" role="status">Your sign-in is confirmed for five minutes.</p>
    <a class="app-control primary wide" href="/platform/ui">Back to the console</a>
  {:else}
    <a class="app-control primary wide" href="/platform">Back to sign-in</a>
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
</style>
