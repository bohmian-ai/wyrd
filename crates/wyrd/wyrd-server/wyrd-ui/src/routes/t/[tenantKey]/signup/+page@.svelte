<script lang="ts">
  import { onMount } from 'svelte';
  import Entry from '$lib/Entry.svelte';
  import Mark from '$lib/Mark.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  let code = $state('');
  // An invitation link carries its code in the fragment, which browsers never
  // send to a server or in a referrer; only this page reads it. The form posts
  // to the bare path so the redirect after signup does not inherit it.
  onMount(() => {
    code = new URLSearchParams(window.location.hash.slice(1)).get('invitation') ?? '';
  });
  const names = { google: 'Google', github: 'GitHub' } as const;
</script>

<svelte:head><title>Sign up · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Join {data.tenant.key}" issue={form?.problem ?? data.problem}>
  <form class="fields" method="POST" action="?">
    <label for="invitation-code">Invitation code</label>
    <input id="invitation-code" class="app-input mono" type="password" name="invitationCode" required autocomplete="off" bind:value={code} />
    <label for="username">Username</label>
    <input id="username" class="app-input" name="username" required minlength="3" maxlength="64" autocomplete="username" defaultValue={form?.username ?? ''} />
    <p>3–64 characters: letters, digits, dot, underscore or hyphen, starting with a letter or digit. Stored in lowercase.</p>
    <label for="password">Password</label>
    <PasswordInput id="password" name="password" required autocomplete="new-password" />
    <p>15–128 characters. A passphrase works well; common passwords are refused.</p>
    <button class="app-control primary wide" type="submit">Create account</button>
  </form>
  {#each data.providers as provider (provider)}
    <form method="POST" action="/t/{encodeURIComponent(data.tenant.key)}/login?/social">
      <input type="hidden" name="provider" value={provider} />
      <input type="hidden" name="invitationCode" value={code} />
      <button class="app-control wide provider" type="submit" disabled={!code}><Mark method={provider} size={18} />Join with {names[provider]}</button>
    </form>
  {/each}
  <nav class="links"><a href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
