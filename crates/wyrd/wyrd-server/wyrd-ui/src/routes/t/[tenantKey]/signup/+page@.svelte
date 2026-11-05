<script lang="ts">
  import Entry from '$lib/Entry.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
</script>

<svelte:head><title>Sign up · {data.tenant.key} · Wyrd</title></svelte:head>
<Entry heading="Join {data.tenant.key}" issue={form?.problem ?? data.problem}>
  <form class="fields" method="POST">
    <label for="invitation-code">Invitation code</label>
    <input id="invitation-code" class="app-input" type="password" name="invitationCode" required autocomplete="off" />
    <label for="username">Username</label>
    <input id="username" class="app-input" name="username" required minlength="3" maxlength="64" autocomplete="username" value={form?.username ?? ''} />
    <p>3–64 characters: letters, digits, dot, underscore or hyphen, starting with a letter or digit. Stored in lowercase.</p>
    <label for="password">Password</label>
    <PasswordInput id="password" name="password" required autocomplete="new-password" />
    <p>8–128 characters. A passphrase works well; common passwords are refused.</p>
    <button class="app-control primary wide" type="submit">Create account</button>
  </form>
  <nav class="links"><a href="/t/{encodeURIComponent(data.tenant.key)}/login">Back to sign-in</a></nav>
</Entry>
