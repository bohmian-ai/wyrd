<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const tenant = $derived(encodeURIComponent(data.tenant.key));
</script>

<svelte:head><title>Sign in · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Sign in to {data.tenant.key}" issue={form?.problem ?? data.problem}>
  {#if data.reset}<p role="status">Password set. Sign in with your new password.</p>{/if}
  <form class="fields" method="POST" action="?/password">
    <label for="username">Username</label>
    <input id="username" class="app-input" name="username" required autocomplete="username" value={form && 'username' in form ? form.username : ''} />
    <label for="password">Password</label>
    <input id="password" class="app-input" type="password" name="password" required autocomplete="current-password" />
    <button class="app-control primary wide" type="submit">Sign in</button>
  </form>
  <form method="POST" action="?/sso"><button class="app-control wide" type="submit">Sign in with SSO</button></form>
  <nav class="links" aria-label="Other sign-in">
    <a href="/t/{tenant}/signup">Sign up with an invitation</a>
    <a href="/t/{tenant}/reset">Reset with a code</a>
    <a class="recovery" href="/t/{tenant}/login/api-key">Sign in with an API key</a>
  </nav>
</Entry>
