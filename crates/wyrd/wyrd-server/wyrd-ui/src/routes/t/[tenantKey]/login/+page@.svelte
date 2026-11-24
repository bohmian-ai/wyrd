<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const tenant = $derived(encodeURIComponent(data.tenant.key));
  const names = { google: 'Google', github: 'GitHub' } as const;
</script>

<svelte:head><title>Sign in · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Sign in to {data.tenant.key}" issue={form?.problem ?? data.problem}>
  {#if data.reset}<p role="status">Password set. Sign in with your new password.</p>{/if}
  {#if data.notice === 'no_account'}<p role="status">No account in {data.tenant.key} uses that sign-in. Open your invitation link, or ask an administrator to invite you.</p>{/if}
  {#if data.notice === 'account_exists'}<p role="status">That sign-in already has an account in {data.tenant.key}. Sign in with it below, or ask an administrator for more access.</p>{/if}
  {#if data.password}
    <form class="fields" method="POST" action="?/password">
      <label for="username">Username</label>
      <input id="username" class="app-input" name="username" required autocomplete="username" defaultValue={form && 'username' in form ? form.username : ''} />
      <label for="password">Password</label>
      <PasswordInput id="password" name="password" required autocomplete="current-password" />
      <button class="app-control primary wide" type="submit">Sign in</button>
    </form>
  {/if}
  {#if data.companySso}
    <form method="POST" action="?/sso"><button class="app-control wide" type="submit">Sign in with SSO</button></form>
  {/if}
  {#each data.providers as provider (provider)}
    <form method="POST" action="?/social">
      <input type="hidden" name="provider" value={provider} />
      <button class="app-control wide" type="submit">Continue with {names[provider]}</button>
    </form>
  {/each}
  <nav class="links" aria-label="Other sign-in">
    <a href="/t/{tenant}/signup">Sign up with an invitation</a>
    {#if data.password}<a href="/t/{tenant}/reset">Reset your password</a>{/if}
    <a class="recovery" href="/t/{tenant}/login/api-key">Sign in with an API key</a>
  </nav>
</Entry>
