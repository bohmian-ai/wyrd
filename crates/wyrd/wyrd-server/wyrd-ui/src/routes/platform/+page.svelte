<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  // The form posts as a full page load, so a refusal renders a fresh page with this cleared.
  let busy = $state('');
</script>

<svelte:head><title>Platform admin · Wyrd</title></svelte:head>
<Entry heading="Platform admin" issue={form?.problem ?? data.problem}>
  {#if data.ended}<p class="center" role="status">Your console session ended. Sign in again.</p>{/if}
  <p class="center">For operators of this Wyrd deployment. Workspace accounts sign in on their workspace's page.</p>
  {#if data.methods.password}
    <form class="fields" method="POST" action="?/password" onsubmit={() => (busy = 'password')}>
      <label for="username">Username</label>
      <input id="username" class="app-input" name="username" required autocomplete="username" readonly={!!busy} defaultValue={form && 'username' in form ? form.username : ''} />
      <label for="password">Password</label>
      <PasswordInput id="password" name="password" required autocomplete="current-password" readonly={!!busy} />
      <button class="app-control primary wide" type="submit" disabled={!!busy} aria-busy={busy === 'password' ? 'true' : undefined}>{busy === 'password' ? 'Signing in…' : 'Sign in'}</button>
    </form>
  {/if}
  {#if data.methods.oidc}
    <form method="POST" action="?/sso" onsubmit={() => (busy = 'sso')}>
      <button class="app-control wide" type="submit" disabled={!!busy}>Sign in with platform SSO</button>
    </form>
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
  input[readonly] {
    opacity: 0.6;
  }
</style>
