<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
</script>

<svelte:head><title>API key sign-in · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Sign in to {data.tenant.key}" issue={form?.problem ?? data.problem}>
  <form class="fields api-key" method="POST">
    <label for="api-key">API key</label>
    <input id="api-key" class="app-input" type="password" name="apiKey" required autocomplete="off" />
    <p>Recovery sign-in with an operator API key, for a tenant without SSO or when SSO is unavailable. This browser keeps the key only inside an encrypted, HttpOnly cookie.</p>
    <button class="app-control primary wide" type="submit">Sign in with API key</button>
  </form>
  <nav class="links"><a class="back" href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
